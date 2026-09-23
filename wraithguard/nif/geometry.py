"""Turning a parsed NIF into triangles positioned in world space.

:mod:`wraithguard.nif.reader` reads blocks; it does not assemble them. A mesh
is a *tree*: every shape carries its own transform, every node above it carries
another, and a shape's vertices mean nothing until those are composed down the
chain. A viewer that skipped that would draw every part of a mesh piled on the
origin.

This module does the composition and nothing else. It takes a file parsed with
``geometry=True`` and returns flat triangle soup with world-space coordinates,
which is the form both a renderer and a bounding-box calculation want.

**Roots are found, not assumed.** Block 0 is conventionally the root, and
conventionally is not good enough for files written by twenty years of
exporters. A root here is any node no other node claims as a child, which is
derived from the child links rather than trusted.
"""

from __future__ import annotations

from dataclasses import dataclass, field, replace
from typing import TYPE_CHECKING, Any, Final

from wraithguard.logging_setup import get_logger
from wraithguard.nif.report import COLLISION_NODES as COLLISION_HINT, normalise_texture

if TYPE_CHECKING:
    from wraithguard.nif.reader import Block, NifFile

LOG = get_logger(__name__)

#: Block types that hold children and therefore continue the walk.
_NODE_TYPES: frozenset[str] = frozenset(
    {
        "NiNode",
        "RootCollisionNode",
        "AvoidNode",
        "NiBSParticleNode",
        "NiBSAnimationNode",
        "NiBillboardNode",
        "NiSwitchNode",
        "NiLODNode",
        # A collision *switch* toggles collision on its children but they are
        # still drawable geometry -- it holds a full child list, so the walk must
        # recurse into it or every shape beneath it (common in vanilla meshes) is
        # silently lost. Not in COLLISION_NODES: its children are visible.
        "NiCollisionSwitch",
    }
)

#: Node types under which a ``NiTriShape`` is *particle* geometry, not a surface:
#: its vertices are particle positions the viewer draws as a drifting point cloud
#: (Morrowind mist and steam are usually a trishape under one of these, driven by
#: a particle controller, rather than a ``NiParticles`` block).
_PARTICLE_NODE_TYPES: frozenset[str] = frozenset({"NiBSParticleNode"})

#: Block types that carry drawable geometry.
_SHAPE_TYPES: frozenset[str] = frozenset({"NiTriShape"})

#: Particle-geometry blocks: their ``data`` block holds particle positions we
#: draw as a point cloud (mist, glowbugs, dust) rather than a surface.
_PARTICLE_GEOM_TYPES: frozenset[str] = frozenset(
    {"NiParticles", "NiAutoNormalParticles", "NiRotatingParticles"}
)

#: Block types that mean the file is a particle *emitter* -- mist and fog volumes,
#: glowbug swarms, dust, steam. A cell viewer wants these as their own category:
#: their whole purpose is an effect the player barely sees, so they clutter a
#: conflict check and are the first thing to toggle off. Covers the emitter node,
#: the particle geometry variants, and the controllers that drive them, so a file
#: is caught whether or not it also carries a visible placeholder shape.
_PARTICLE_TYPES: frozenset[str] = frozenset(
    {
        "NiBSParticleNode",
        "NiParticles",
        "NiAutoNormalParticles",
        "NiRotatingParticles",
        "NiParticleSystem",
        "NiParticleSystemController",
        "NiBSPArrayController",
    }
)

#: How deep the walk may go before it concludes the graph has a cycle. NIF
#: children are links by index and nothing in the format forbids a loop, so a
#: hostile or broken file could otherwise spin forever.
_MAX_DEPTH: int = 64

#: ``NiAVObject`` flag bit that hides a block from the render (OpenMW's
#: ``Flag_Hidden``). The engine keeps it for collision but never draws it, so a
#: viewer that wants to match the game treats it exactly like a collision node.
_HIDDEN_FLAG: Final[int] = 0x1


@dataclass(frozen=True, slots=True)
class Transform:
    """A rotation, a uniform scale and a translation, applied in that order.

    Attributes:
        rotation: Three rows of three, as the file stores it.
        scale: Uniform scale factor.
        translation: Offset applied after rotating and scaling.
    """

    rotation: tuple[tuple[float, float, float], ...] = (
        (1.0, 0.0, 0.0),
        (0.0, 1.0, 0.0),
        (0.0, 0.0, 1.0),
    )
    scale: float = 1.0
    translation: tuple[float, float, float] = (0.0, 0.0, 0.0)

    def apply(self, point: tuple[float, float, float]) -> tuple[float, float, float]:
        """Move one point into this transform's frame.

        Args:
            point: The point to move.

        Returns:
            The transformed point.
        """
        x, y, z = point
        rows = self.rotation
        return (
            (rows[0][0] * x + rows[0][1] * y + rows[0][2] * z) * self.scale + self.translation[0],
            (rows[1][0] * x + rows[1][1] * y + rows[1][2] * z) * self.scale + self.translation[1],
            (rows[2][0] * x + rows[2][1] * y + rows[2][2] * z) * self.scale + self.translation[2],
        )

    def then(self, child: Transform) -> Transform:
        """Compose a child's transform with this one.

        The child's rotation and scale are expressed in *this* frame, so the
        child's translation has to be carried through this transform rather
        than simply added -- adding them is the mistake that leaves a rotated
        parent's children in the wrong place.

        Args:
            child: The transform below this one in the tree.

        Returns:
            The combined transform.
        """
        rows = self.rotation
        other = child.rotation
        combined: tuple[tuple[float, float, float], ...] = tuple(
            (
                sum(rows[i][k] * other[k][0] for k in range(3)),
                sum(rows[i][k] * other[k][1] for k in range(3)),
                sum(rows[i][k] * other[k][2] for k in range(3)),
            )
            for i in range(3)
        )
        return Transform(
            rotation=combined,
            scale=self.scale * child.scale,
            translation=self.apply(child.translation),
        )


#: Bit in a ``NiAlphaProperty``'s flags that enables blending. Taken from
#: ``tes3``'s ``flag_props!`` block, which spells the same masks out.
_ALPHA_BLEND_MASK: Final[int] = 0x0001

#: Bit that enables alpha testing -- a cutout rather than a fade. Independent
#: of blending, and the distinction matters: foliage sets this one alone.
_ALPHA_TEST_MASK: Final[int] = 0x0200


#: One animation key: its time in seconds and its value.
AnimKey = tuple[float, float]


@dataclass(frozen=True, slots=True)
class UVAnimation:
    """A shape's scrolling/scaling texture animation (a ``NiUVController``).

    Morrowind animates a texture by sliding or stretching its coordinates over
    time rather than moving the geometry -- banners, force fields, conveyor
    belts, some water. Each field is that channel's ``(time, value)`` keys, empty
    when the channel does not animate. The viewer advances the material's UV
    offset and tiling from these on its clock.

    Attributes:
        u_offset: Horizontal scroll keys (added to U).
        v_offset: Vertical scroll keys (added to V).
        u_tiling: Horizontal repeat keys (multiplies U).
        v_tiling: Vertical repeat keys (multiplies V).
    """

    u_offset: tuple[AnimKey, ...] = ()
    v_offset: tuple[AnimKey, ...] = ()
    u_tiling: tuple[AnimKey, ...] = ()
    v_tiling: tuple[AnimKey, ...] = ()

    def is_empty(self) -> bool:
        """Whether no channel carries any key (so there is nothing to animate)."""
        return not (self.u_offset or self.v_offset or self.u_tiling or self.v_tiling)


#: A rotation key: its time and a ``(w, x, y, z)`` quaternion.
QuatKey = tuple[float, tuple[float, float, float, float]]
#: A translation key: its time and an ``(x, y, z)`` offset.
Vec3Key = tuple[float, tuple[float, float, float]]
#: A visibility key: its time and whether the shape is shown from then on.
VisKey = tuple[float, bool]


@dataclass(frozen=True, slots=True)
class TransformAnimation:
    """A node's keyframe animation as a direct scene-graph composition.

    The shape stays in model space, so an animated node can be evaluated exactly
    where it lives in the hierarchy: ``nodeWorld(t) = above · L(t) · below``.
    ``above`` is the composed rest transform before the animated node, ``rest``
    is that node's own local rest transform (used for channels with no keys),
    and ``below`` is the composed child chain from that node to the shape. This
    avoids reconstructing an animation delta and avoids an inverse matrix on every
    animated shape and frame.

    Attributes:
        above: The composed transform above the animated node, in model space.
        rest: The node's own local transform at rest.
        below: The composed child chain from the animated node to the shape.
        rotation: ``(time, quaternion)`` keys, empty when the node does not rotate
            (or stores rotation as euler keys, which this does not yet play).
        translation: ``(time, offset)`` keys, empty when it does not translate.
        scale: ``(time, factor)`` keys, empty when it does not scale.
    """

    above: Transform
    rest: Transform
    below: Transform = Transform()
    rotation: tuple[QuatKey, ...] = ()
    translation: tuple[Vec3Key, ...] = ()
    scale: tuple[AnimKey, ...] = ()

    def is_empty(self) -> bool:
        """Whether no channel carries any key (so there is nothing to animate)."""
        return not (self.rotation or self.translation or self.scale)


@dataclass(frozen=True, slots=True)
class MorphTarget:
    """One morph target of a ``NiGeomMorpherController``: a weight track + offsets.

    Attributes:
        weights: ``(time, weight)`` keys driving how much of this target blends in.
        deltas: A per-vertex offset added to the base pose, scaled by the weight.
            One entry per shape vertex, in order.
    """

    weights: tuple[AnimKey, ...]
    deltas: tuple[tuple[float, float, float], ...]


@dataclass(frozen=True, slots=True)
class MorphAnimation:
    """A shape's vertex-morph animation (a ``NiGeomMorpherController``).

    Hanging cloth -- banners, tapestries, flags -- ripples by blending the base
    geometry with morph targets over time: ``vertex = base + Σ weightᵢ(t)·deltaᵢ``,
    the blend OpenMW applies. Target 0 (the base pose) is the shape's own
    geometry and is not repeated here; :attr:`targets` are the delta targets
    1..N.

    Attributes:
        targets: The delta morph targets, each with its own weight track.
    """

    targets: tuple[MorphTarget, ...]

    def is_empty(self) -> bool:
        """Whether no target carries any weight key (so nothing animates)."""
        return not any(target.weights for target in self.targets)


@dataclass(frozen=True, slots=True)
class Mesh:
    """One drawable shape. Model-space NIF shapes retain their node transform separately.

    Attributes:
        name: The shape's name, as the exporter wrote it.
        vertices: Shape-local positions for :func:`model_shapes`; world-space positions
            for the explicit baked :func:`world_meshes` compatibility view.
        triangles: Index triples into :attr:`vertices`.
        uvs: Texture coordinates, one per vertex, empty when the shape has
            none. Only the first UV set is kept: Morrowind draws from it, and
            the rest exist for tools rather than for the game.
        texture: The base texture path, normalised, or ``""`` when untextured.
        glow: The self-illumination texture path, normalised, or ``""`` when
            the shape has none. Distinct from the normal and specular maps a
            viewer finds by filename: this one is a real NIF texture slot,
            the same way ``texture`` is.
        dark: The dark-map texture path, normalised, or ``""`` when the shape
            has none. Multiplied into the base color; the closest built-in
            equivalent a renderer has is an ambient-occlusion map.
        decals: Every decal texture path the shape names, normalised, in slot
            order -- ``decal_0`` first. Empty when it has none.

            A list rather than a single path because the format genuinely
            allows several and vanilla content uses them: ``7decals.NIF`` is
            named for the fact. The reader has always parsed past the first
            (see :func:`~wraithguard.nif.reader._slot_name`); only this layer
            was throwing the rest away.

            Order is load-bearing. Decals composite over one another, so
            slot order is paint order, and reversing it puts the wrong one
            on top.
        detail: The detail-map texture path, normalised, or ``""`` when the
            shape has none. Also multiplied into the base color, at a 2x UV
            tile -- a second multiply alongside ``dark``, not a substitute for
            it.
        gloss: The gloss-map texture path, normalised, or ``""`` when the
            shape has none. A single-channel specular *mask* -- brighter is
            shinier -- and distinct from the specular *map* an OpenMW-style
            ``_spec`` sibling provides, which carries color.
        bump: The bump-slot texture path, normalised, or ``""`` when the shape
            has none. What this means is convention-dependent and this module
            does not decide it: vanilla Morrowind ignores the slot outright,
            while MGE-XE and NifSkope repurpose it to carry tangent-space
            normals. See :func:`~wraithguard.images.roles.classify`.
        vertex_colors: One RGBA tuple per vertex, each channel 0-1, empty when
            the shape has none *or* when the count does not match the vertex
            count. That is the same all-or-nothing rule :attr:`uvs` uses and
            for the same reason: a partial set makes a renderer index past the
            end of an attribute and draw nothing, which is worse than drawing
            it uncoloured.
        diffuse: The material's diffuse color, or ``None`` when the shape has
            no ``NiMaterialProperty``. Multiplies with the base texture.
            ``None`` means *undescribed*, not *black* -- a caller with neither
            texture nor color must fall back to white, or every unmaterialed
            shape renders as a silhouette.
        emissive: The material's emissive color, or ``None`` when there is no
            material property. Combines with :attr:`glow` the way
            :attr:`diffuse` combines with :attr:`texture` -- multiplied -- so
            a renderer that already multiplies an emissive map by an emissive
            color is correct by construction, with no glow-specific case.
        opacity: The material's own alpha, ``1.0`` when there is no material
            property. Independent of :attr:`alpha_blend`: this is *how*
            transparent, blending is *whether the renderer looks at all*.
        alpha_blend: Whether the shape's ``NiAlphaProperty`` enables blending.
            ``False`` when the shape has no such property, which reads
            correctly: nothing describing transparency means opaque.
        alpha_test: Whether that property enables alpha testing -- a cutout,
            not a fade. Independent of :attr:`alpha_blend`; foliage commonly
            sets this and not that.
        alpha_threshold: The cutout reference, normalised to 0-1 from the
            byte the file stores. Only meaningful when :attr:`alpha_test`.
        emitter: Whether the shape's file is a particle emitter (mist, fog,
            glowbugs, dust). Set on every shape of a file that carries any
            particle block, so the cell viewer can group these under one
            "Emitter" toggle rather than leaving them scattered among the
            statics they are recorded as.
        water: Whether the shape is a cell's water surface (see
            :func:`wraithguard.scene.water.water_mesh`). The viewer draws these
            with an animated water shader rather than a flat translucent quad;
            nothing a real NIF produces sets this.
        blend_layer: For a terrain texture *layer*, its paint order (0 is the
            base, drawn opaque; higher layers are drawn over it, faded in by the
            per-vertex alpha in :attr:`vertex_colors`). ``-1`` for everything
            else. This is how the viewer reproduces Morrowind's soft texture
            boundaries: one layer per land texture, composited in order, rather
            than a single hard-edged splat. Nothing a real NIF produces sets this.
    """

    name: str
    vertices: list[tuple[float, float, float]] = field(default_factory=list)
    triangles: list[tuple[int, int, int]] = field(default_factory=list)
    uvs: list[tuple[float, float]] = field(default_factory=list)
    texture: str = ""
    glow: str = ""
    dark: str = ""
    decals: list[str] = field(default_factory=list)
    detail: str = ""
    gloss: str = ""
    bump: str = ""
    collision: bool = False
    vertex_colors: list[tuple[float, float, float, float]] = field(default_factory=list)
    diffuse: tuple[float, float, float] | None = None
    emissive: tuple[float, float, float] | None = None
    opacity: float = 1.0
    alpha_blend: bool = False
    alpha_test: bool = False
    alpha_threshold: float = 0.0
    emitter: bool = False
    water: bool = False
    blend_layer: int = -1
    points: bool = False
    """Whether this mesh is a *particle cloud* rather than a surface: its
    :attr:`vertices` are particle positions and :attr:`triangles` is empty, so the
    viewer draws them as ``THREE.Points`` (mist, glowbugs, dust) instead of a
    surface. Set by :func:`model_shapes` for a ``NiParticles`` block."""
    uv_anim: UVAnimation | None = None
    """The scrolling/scaling texture animation driving this shape, or ``None``
    when it has none. Populated only when the file was parsed with
    ``animation=True``; the geometry/conflict paths leave it ``None``."""
    transform_anim: TransformAnimation | None = None
    """The node keyframe animation (sway/spin/slide) driving this shape, or
    ``None``. The viewer composes its direct ``above · L(t) · below`` transform
    into the instance matrix. Populated only when parsed with ``animation=True``."""
    vis_anim: tuple[VisKey, ...] | None = None
    """The visibility animation (a ``NiVisController``) that blinks this shape on
    and off over time, or ``None``. Each key is ``(time, visible)``. Populated
    only when parsed with ``animation=True``."""
    morph_anim: MorphAnimation | None = None
    """The vertex-morph animation (a ``NiGeomMorpherController``) rippling this
    shape's cloth, or ``None``. The viewer blends the targets over the base each
    frame. Populated only when parsed with ``animation=True``."""
    node_world: Transform = field(default_factory=Transform)
    """The shape's node-chain transform, composed to the file's root. Identity
    on a *baked* mesh (:func:`world_meshes`), where the transform is already in
    :attr:`vertices`; the rest composition on a *model-space* shape
    (:func:`model_shapes`), where the vertices are the shape's own local
    coordinates and the viewer folds this into the instance matrix instead. This
    is the seam of the un-baking migration -- see ``UNBAKE_MIGRATION.md``."""


def _transform_of(block: Block) -> Transform:
    """Read a block's own transform.

    Args:
        block: A scene object.

    Returns:
        Its transform, defaulting to identity when the fields are absent --
        which happens for a block read without ``geometry=True``.
    """
    rotation = block.fields.get("rotation_m3")
    translation = block.fields.get("translation_xyz")
    scale = block.fields.get("scale", 1.0)
    rows: tuple[tuple[float, float, float], ...] = (
        tuple((float(r[0]), float(r[1]), float(r[2])) for r in rotation)
        if rotation
        else Transform().rotation
    )
    return Transform(
        rotation=rows,
        scale=float(scale) if isinstance(scale, (int, float)) else 1.0,
        translation=(
            (float(translation[0]), float(translation[1]), float(translation[2]))
            if translation
            else (0.0, 0.0, 0.0)
        ),
    )


#: How many controllers deep a chain may run before the walk concludes it loops.
#: Controllers link to one another by index and nothing forbids a cycle.
_MAX_CONTROLLER_CHAIN: Final[int] = 64

#: The identity transform, for the "already baked / directly built" fast path in
#: :func:`bake_mesh` -- a mesh whose ``node_world`` is this needs no baking.
_IDENTITY: Final[Transform] = Transform()


def _keys_of(data: Block, field_name: str) -> tuple[AnimKey, ...]:
    """Read a retained float key list off a data block as ``(time, value)`` keys.

    Args:
        data: The ``NiUVData`` (or similar) block.
        field_name: The companion field, e.g. ``"u_keys_values"``.

    Returns:
        The keys, or empty when the field is absent (the file was parsed without
        ``animation=True``, or the channel has no keys).
    """
    values = data.fields.get(field_name)
    if not isinstance(values, list):
        return ()
    return tuple((float(time), float(value)) for time, value in values)


def _uv_animation_of(block: Block, by_index: dict[int, Block]) -> UVAnimation | None:
    """Find the ``NiUVController`` driving a block, if any, as a :class:`UVAnimation`.

    Walks the block's controller chain (``controller`` then each
    ``next_controller``) for a ``NiUVController``, follows its ``data`` link to
    the ``NiUVData``, and reads the retained key channels.

    Args:
        block: A node or shape that may own controllers.
        by_index: Every parsed block, by index.

    Returns:
        The animation, or ``None`` when there is no UV controller or it carries
        no keys (or the file was parsed without ``animation=True``).
    """
    controller_index = block.link("controller")
    seen: set[int] = set()
    depth = 0
    while controller_index >= 0 and depth < _MAX_CONTROLLER_CHAIN:
        if controller_index in seen:
            break
        seen.add(controller_index)
        controller = by_index.get(controller_index)
        if controller is None:
            break
        if controller.type_name == "NiUVController":
            data = by_index.get(controller.link("data"))
            if data is not None:
                animation = UVAnimation(
                    u_offset=_keys_of(data, "u_keys_values"),
                    v_offset=_keys_of(data, "v_keys_values"),
                    u_tiling=_keys_of(data, "u_scale_keys_values"),
                    v_tiling=_keys_of(data, "v_scale_keys_values"),
                )
                return None if animation.is_empty() else animation
        controller_index = controller.link("next_controller")
        depth += 1
    return None


def _vis_animation_of(block: Block, by_index: dict[int, Block]) -> tuple[VisKey, ...] | None:
    """Find the ``NiVisController`` blinking a block, if any, as visibility keys.

    Walks the block's controller chain for a ``NiVisController``, follows its
    ``data`` link to the ``NiVisData``, and reads the retained visibility keys.

    Args:
        block: A node or shape that may own controllers.
        by_index: Every parsed block, by index.

    Returns:
        ``(time, visible)`` keys, or ``None`` when there is no visibility
        controller or it carries no keys (or the file was parsed without
        ``animation=True``).
    """
    controller_index = block.link("controller")
    seen: set[int] = set()
    depth = 0
    while controller_index >= 0 and depth < _MAX_CONTROLLER_CHAIN:
        if controller_index in seen:
            break
        seen.add(controller_index)
        controller = by_index.get(controller_index)
        if controller is None:
            break
        if controller.type_name == "NiVisController":
            data = by_index.get(controller.link("data"))
            keys = data.fields.get("vis_keys_values") if data is not None else None
            if isinstance(keys, list) and keys:
                return tuple((float(time), bool(visible)) for time, visible in keys)
        controller_index = controller.link("next_controller")
        depth += 1
    return None


def _morph_animation_of(
    block: Block, by_index: dict[int, Block], vertex_count: int
) -> MorphAnimation | None:
    """Find the ``NiGeomMorpherController`` morphing a shape, as a :class:`MorphAnimation`.

    Walks the shape's controller chain for a ``NiGeomMorpherController``, follows
    its ``data`` link to the ``NiMorphData``, and reads the retained morph
    targets. Target 0 is the base pose (the shape's own geometry); targets 1..N
    are the deltas the viewer blends by weight.

    Args:
        block: The shape that may own the controller.
        by_index: Every parsed block, by index.
        vertex_count: The shape's vertex count; a target whose vertex count
            disagrees is a mismatch and the whole morph is skipped rather than
            blended wrong.

    Returns:
        The animation, or ``None`` when there is no morph controller, it has
        fewer than two targets, a target's vertex count disagrees, or no target
        carries weight keys (or the file was parsed without ``animation=True``).
    """
    controller_index = block.link("controller")
    seen: set[int] = set()
    depth = 0
    while controller_index >= 0 and depth < _MAX_CONTROLLER_CHAIN:
        if controller_index in seen:
            break
        seen.add(controller_index)
        controller = by_index.get(controller_index)
        if controller is None:
            break
        if controller.type_name == "NiGeomMorpherController":
            data = by_index.get(controller.link("data"))
            morphs = data.fields.get("morphs_values") if data is not None else None
            if isinstance(morphs, list) and len(morphs) >= 2:
                targets: list[MorphTarget] = []
                for target in morphs[1:]:  # target 0 is the base pose
                    verts = target.get("vertices") or []
                    if len(verts) != vertex_count:
                        return None  # count mismatch: skip, safer than a wrong blend
                    targets.append(
                        MorphTarget(
                            weights=tuple(
                                (float(t), float(w)) for t, w in target.get("weights") or []
                            ),
                            deltas=tuple((float(v[0]), float(v[1]), float(v[2])) for v in verts),
                        )
                    )
                animation = MorphAnimation(targets=tuple(targets))
                return None if animation.is_empty() else animation
        controller_index = controller.link("next_controller")
        depth += 1
    return None


def _transform_animation_of(
    block: Block, by_index: dict[int, Block], above: Transform, rest: Transform
) -> TransformAnimation | None:
    """Find the ``NiKeyframeController`` driving a node, as a :class:`TransformAnimation`.

    Walks the node's controller chain for a ``NiKeyframeController``, follows its
    ``data`` link to the ``NiKeyframeData``, and reads the retained rotation,
    translation and scale keys.

    Args:
        block: The node that may own the controller.
        by_index: Every parsed block, by index.
        above: The composed transform above this node (its rest frame's parent).
        rest: This node's own local transform.

    Returns:
        The animation, or ``None`` when there is no keyframe controller or it
        carries no playable keys (or the file was parsed without
        ``animation=True``). Rotation stored as euler keys is treated as no
        rotation for now -- translation and scale still play.
    """
    controller_index = block.link("controller")
    seen: set[int] = set()
    depth = 0
    while controller_index >= 0 and depth < _MAX_CONTROLLER_CHAIN:
        if controller_index in seen:
            break
        seen.add(controller_index)
        controller = by_index.get(controller_index)
        if controller is None:
            break
        if controller.type_name == "NiKeyframeController":
            data = by_index.get(controller.link("data"))
            keys = data.fields.get("keyframe_data_values") if data is not None else None
            if isinstance(keys, dict):
                return _transform_anim_from_track(keys, above, rest)
        controller_index = controller.link("next_controller")
        depth += 1
    return None


def _transform_anim_from_track(
    track: dict[str, object], above: Transform, rest: Transform
) -> TransformAnimation | None:
    """Build a :class:`TransformAnimation` from a keyframe track, or ``None`` if empty.

    Shared by the embedded ``NiKeyframeController`` path and the external ``.kf``
    binding: both hold the same ``{rotation, translation, scale}`` track and want
    the same direct ``above · L(t) · below`` representation.

    Args:
        track: The keyframe data (``keyframe_data_values``).
        above: The composed transform above the animated node.
        rest: The node's own local transform.

    Returns:
        The animation, or ``None`` when no channel carries a playable key.
    """
    animation = TransformAnimation(
        above=above,
        rest=rest,
        rotation=_quat_keys(track.get("rotation")),
        translation=_vec3_keys(track.get("translation")),
        scale=_scalar_keys(track.get("scale")),
    )
    return None if animation.is_empty() else animation


def _quat_keys(value: object) -> tuple[QuatKey, ...]:
    """Coerce retained rotation keys to ``(time, (w, x, y, z))``, or empty.

    A ``dict`` here is euler rotation, which is not yet played, so it yields no
    keys (translation and scale still animate).
    """
    if not isinstance(value, list):
        return ()
    out: list[QuatKey] = []
    for time, quat in value:
        w, x, y, z = quat
        out.append((float(time), (float(w), float(x), float(y), float(z))))
    return tuple(out)


def _vec3_keys(value: object) -> tuple[Vec3Key, ...]:
    """Coerce retained translation keys to ``(time, (x, y, z))``, or empty."""
    if not isinstance(value, list):
        return ()
    return tuple((float(time), (float(v[0]), float(v[1]), float(v[2]))) for time, v in value)


def _scalar_keys(value: object) -> tuple[AnimKey, ...]:
    """Coerce retained scalar keys to ``(time, value)``, or empty."""
    if not isinstance(value, list):
        return ()
    return tuple((float(time), float(v)) for time, v in value)


def find_roots(parsed: NifFile) -> list[int]:
    """Find the blocks that nothing claims as a child.

    Args:
        parsed: A parsed file.

    Returns:
        Root block indices, in file order.
    """
    claimed: set[int] = set()
    for block in parsed.blocks:
        for child in block.fields.get("children_links") or []:
            if child >= 0:
                claimed.add(int(child))
    return [b.index for b in parsed.blocks if b.index not in claimed]


def model_shapes(parsed: NifFile, kf_tracks: dict[str, dict[str, Any]] | None = None) -> list[Mesh]:
    """Collect every drawable shape in the file's own model space.

    Each shape's vertices stay in its local coordinates and its composed node
    transform is carried in :attr:`Mesh.node_world`, ready for the viewer to fold
    into a per-instance matrix (or to animate). :func:`world_meshes` is the baked
    view of the same shapes, for callers that want flat world space. See
    ``UNBAKE_MIGRATION.md`` for why the two exist.

    Args:
        parsed: A file parsed with ``geometry=True``. Parsed without it the
            vertex arrays are absent and the result is empty, which is honest:
            the data was never read.
        kf_tracks: An external ``.kf`` file's ``{node name: keyframe track}`` map
            (from :func:`wraithguard.nif.kf.load_kf`), bound to nodes by name as
            though each named node carried the controller itself. ``None`` for a
            model with no keyframe file.

    Returns:
        One :class:`Mesh` per shape that has geometry, in model space.
    """
    by_index = {block.index: block for block in parsed.blocks}
    kf_tracks = kf_tracks or {}
    meshes: list[Mesh] = []
    seen: set[int] = set()

    def walk(
        index: int,
        parent: Transform,
        depth: int,
        collision: bool,
        uv_anim: UVAnimation | None = None,
        xform_anim: TransformAnimation | None = None,
        xform_below: Transform | None = None,
        vis_anim: tuple[VisKey, ...] | None = None,
        in_particle: bool = False,
    ) -> None:
        """Visit one block and its children, accumulating world transforms.

        A NIF is a graph rather than a tree: a block can be referenced twice,
        and a malformed file can reference itself. ``seen`` and ``_MAX_DEPTH``
        are what stop this recursing forever on a file that is merely wrong
        rather than malicious.

        The root node's *own rotation* is ignored (``depth == 0``). Morrowind
        orients a placed object by its reference rotation alone and does not
        honour a rotation baked into the file's root node -- which is why mesh
        authors are told to leave the root unrotated. Some meshes (and mod
        replacers) ship a rotated root anyway; baking it in turned every such
        object, most visibly rotating architecture corners 90 degrees away from
        where the Construction Set draws them. The root's translation and scale
        are kept (a root offset is a legitimate placement), and children are
        always placed relative to the root, so only the root's own rotation is
        dropped, never a child's.

        Args:
            index: The block to visit.
            parent: The accumulated transform of everything above it.
            depth: How far down the graph this is, for the depth guard.
            collision: Whether an ancestor marked this branch as invisible
                geometry (a collision node, or a hidden flag), which is not drawn.
            uv_anim: The scrolling-texture animation inherited from an ancestor
                node's ``NiUVController``, or ``None``. A controller found on this
                block replaces it for this block and everything below.
            xform_anim: The node keyframe animation inherited from an ancestor's
                ``NiKeyframeController``, or ``None``. A controller found on this
                node replaces it for this node and everything below.
            xform_below: The accumulated child transform from the active animated
                node to the current block. It resets when a nearer animation is
                found on the current node.
            vis_anim: The visibility animation inherited from an ancestor's
                ``NiVisController``, or ``None``. A controller found on this node
                replaces it for this node and everything below.
            in_particle: Whether an ancestor is a particle node, so a shape here
                is particle geometry (drawn as points), not a surface.
        """
        block = by_index.get(index)
        if xform_below is None:
            xform_below = Transform()
        if block is None or depth > _MAX_DEPTH or index in seen:
            if depth > _MAX_DEPTH:
                LOG.warning("scene graph deeper than %d at block %d; stopping", _MAX_DEPTH, index)
            return
        seen.add(index)
        own = _transform_of(block)
        if depth == 0:
            # Drop the root's own rotation but keep its translation and scale.
            own = Transform(scale=own.scale, translation=own.translation)
        here = parent.then(own)
        # Two ways a branch is invisible in game, both inherited by everything
        # under it. RootCollisionNode marks physics-only geometry. The hidden
        # flag (``NiAVObject`` flag ``0x1``, OpenMW's ``Flag_Hidden``) marks the
        # collision-only and barrier meshes that carry ordinary NiTriShapes the
        # engine simply does not draw -- invisible rugs, ship-launch walls. Both
        # are folded into ``collision`` so the one toggle reveals either.
        collision = (
            collision
            or block.type_name in COLLISION_HINT
            or bool(int(block.fields.get("flags", 0)) & _HIDDEN_FLAG)
        )
        # A UV controller on this node drives every shape beneath it; one on the
        # shape itself wins over an inherited one. Inheriting down the branch is
        # how a controller on a parent node reaches the child NiTriShape.
        uv_anim = _uv_animation_of(block, by_index) or uv_anim
        # A keyframe controller drives its whole subtree. Keep the animation as
        # the direct scene-graph split ``above · L(t) · below``: an inner
        # controller replaces an inherited one; otherwise this block becomes one
        # more link in the inherited animation's ``below`` chain.
        block_name = _name_of(block)
        kf_track = kf_tracks.get(block_name) if block_name else None
        local_xform_anim = _transform_animation_of(block, by_index, parent, own) or (
            _transform_anim_from_track(kf_track, parent, own) if kf_track else None
        )
        if local_xform_anim is not None:
            xform_anim = local_xform_anim
            xform_below = Transform()
        elif xform_anim is not None:
            xform_below = xform_below.then(own)
            xform_anim = replace(xform_anim, below=xform_below)
        vis_anim = _vis_animation_of(block, by_index) or vis_anim
        # A shape anywhere under a particle node is the emitter's particle
        # geometry, drawn as a point cloud rather than a surface.
        in_particle = in_particle or block.type_name in _PARTICLE_NODE_TYPES
        if block.type_name in _SHAPE_TYPES:
            mesh = _shape_to_mesh(
                block,
                here,
                by_index,
                collision,
                uv_anim,
                xform_anim,
                vis_anim,
                as_points=in_particle,
            )
            if mesh is not None:
                meshes.append(mesh)
        if block.type_name in _PARTICLE_GEOM_TYPES:
            cloud = _particles_to_mesh(block, here, by_index, collision)
            if cloud is not None:
                meshes.append(cloud)
        if block.type_name in _NODE_TYPES:
            for child in block.fields.get("children_links") or []:
                if child >= 0:
                    walk(
                        int(child),
                        here,
                        depth + 1,
                        collision,
                        uv_anim,
                        xform_anim,
                        xform_below,
                        vis_anim,
                        in_particle,
                    )

    for root in find_roots(parsed):
        walk(root, Transform(), 0, False)
    # A file that carries any particle block is an emitter; tag every shape it
    # produced so the cell viewer can categorise the whole object at once. Done
    # file-wide rather than per-branch because a mist or glowbug file is *about*
    # its emitter -- its one visible ring or box belongs with the effect, not
    # with the architecture it is recorded alongside.
    if any(block.type_name in _PARTICLE_TYPES for block in parsed.blocks):
        meshes = [replace(mesh, emitter=True) for mesh in meshes]
    return meshes


def bake_mesh(mesh: Mesh) -> Mesh:
    """Fold a model-space shape's ``node_world`` into its vertices.

    Args:
        mesh: A shape from :func:`model_shapes` (or an already-baked mesh, whose
            ``node_world`` is the identity and which is returned unchanged).

    Returns:
        The shape with world-space vertices and an identity ``node_world``.
    """
    if mesh.node_world == _IDENTITY:
        return mesh  # already baked, or a directly-built (terrain/water) mesh
    world = mesh.node_world
    return replace(
        mesh,
        vertices=[world.apply(v) for v in mesh.vertices],
        node_world=Transform(),
    )


def world_meshes(parsed: NifFile) -> list[Mesh]:
    """Collect every drawable shape, with vertices baked into world space.

    The baked view of :func:`model_shapes`: each shape's ``node_world`` is applied
    to its vertices, so callers that want flat world-space triangle soup (bounds,
    the conflict diff, the current viewer path) are unchanged. New code that can
    keep the hierarchy should prefer :func:`model_shapes` -- see
    ``UNBAKE_MIGRATION.md``.

    Args:
        parsed: A file parsed with ``geometry=True``.

    Returns:
        One :class:`Mesh` per shape that has geometry, in world space.
    """
    return [bake_mesh(mesh) for mesh in model_shapes(parsed)]


def _shape_to_mesh(
    block: Block,
    world: Transform,
    by_index: dict[int, Block],
    collision: bool = False,
    uv_anim: UVAnimation | None = None,
    xform_anim: TransformAnimation | None = None,
    vis_anim: tuple[VisKey, ...] | None = None,
    *,
    as_points: bool = False,
) -> Mesh | None:
    """Build one mesh from a shape and its data block.

    Args:
        block: The ``NiTriShape``.
        world: Its composed node transform, stored as ``node_world`` rather than
            applied -- the vertices stay in the shape's own local space.
        by_index: Every parsed block, by index.
        collision: Whether this shape sits under a ``RootCollisionNode``.
        uv_anim: The scrolling-texture animation inherited for this shape, or
            ``None``.
        xform_anim: The node keyframe animation inherited for this shape, or
            ``None``.
        vis_anim: The visibility animation inherited for this shape, or ``None``.
        as_points: Whether to build this as a particle point cloud (``points``
            set, filed under Emitter) rather than a drawn surface.

    Returns:
        The mesh, or ``None`` when its data block was not reached or carries no
        retained vertices.
    """
    data = by_index.get(block.link("data"))
    if data is None:
        return None
    vertices = data.fields.get("vertices_xyz")
    triangles = data.fields.get("triangles_indices")
    if not vertices or not triangles:
        return None
    # A shape can declare several UV sets. Only the first is Morrowind's, and
    # taking more would mean guessing which one a texture belongs to.
    uvs = (data.fields.get("uv_sets_uv") or [])[: len(vertices)]
    diffuse, emissive, opacity = _material(block, by_index)
    blend, test, threshold = _alpha(block, by_index)
    # The vertex-morph controller lives on the shape itself (it targets this
    # geometry), so it is read here rather than inherited down the branch.
    morph_anim = _morph_animation_of(block, by_index, len(vertices))
    # Vertices are kept in the shape's *own local* space and the composed node
    # transform is carried in ``node_world``; :func:`world_meshes` bakes it in for
    # the callers that still want flat world space. Un-baking is what lets a node
    # animate and a particle emitter stay a live node -- see UNBAKE_MIGRATION.md.
    return Mesh(
        name=_name_of(block),
        vertices=[(float(v[0]), float(v[1]), float(v[2])) for v in vertices],
        # A particle cloud is drawn as points, so it drops its surface topology.
        triangles=[] if as_points else list(triangles),
        uvs=[(float(u), float(v)) for u, v in uvs],
        texture=_texture_slot(block, by_index, "base"),
        glow=_texture_slot(block, by_index, "glow"),
        dark=_texture_slot(block, by_index, "dark"),
        decals=_decal_slots(block, by_index),
        detail=_texture_slot(block, by_index, "detail"),
        gloss=_texture_slot(block, by_index, "gloss"),
        bump=_texture_slot(block, by_index, "bump"),
        collision=collision,
        vertex_colors=_vertex_colors(data, len(vertices)),
        diffuse=diffuse,
        emissive=emissive,
        opacity=opacity,
        alpha_blend=blend,
        alpha_test=test,
        alpha_threshold=threshold,
        uv_anim=uv_anim,
        transform_anim=xform_anim,
        vis_anim=vis_anim,
        morph_anim=morph_anim,
        node_world=world,
        # Under a particle node this shape's vertices are particles: draw them as
        # a drifting point cloud, and file it under the Emitter category.
        points=as_points,
        emitter=as_points,
    )


def _particles_to_mesh(
    block: Block, world: Transform, by_index: dict[int, Block], collision: bool
) -> Mesh | None:
    """Build a point-cloud mesh from a particle block's data.

    A ``NiParticles``/``NiAutoNormalParticles`` names a data block whose vertices
    are the particle positions. Those become a :class:`Mesh` with ``points=True``
    and no triangles, in the shape's local space with the node transform in
    ``node_world`` -- the viewer draws it as drifting points (mist, glowbugs).

    Args:
        block: The particle-geometry block.
        world: Its composed node transform (kept in ``node_world``).
        by_index: Every parsed block, by index.
        collision: Whether an ancestor marked this branch invisible.

    Returns:
        The point-cloud mesh, or ``None`` when its data or positions are absent.
    """
    data = by_index.get(block.link("data"))
    if data is None:
        return None
    positions = data.fields.get("vertices_xyz")
    if not positions:
        return None
    return Mesh(
        name=_name_of(block),
        vertices=[(float(p[0]), float(p[1]), float(p[2])) for p in positions],
        triangles=[],
        texture=_texture_slot(block, by_index, "base"),
        collision=collision,
        points=True,
        emitter=True,
        node_world=world,
    )


def _vertex_colors(data: Block, vertices: int) -> list[tuple[float, float, float, float]]:
    """Read a shape's per-vertex colors, if it has a usable set.

    Reads ``vertex_colors_rgba`` -- the *decoded* companion -- rather than
    ``vertex_colors``, which holds only the count the array gate produced. An
    earlier draft of this read the latter, found an integer where it expected a
    list, and silently produced no colors at all: the field existed and held a
    number, so nothing looked wrong.

    Args:
        data: The shape's geometry data block.
        vertices: How many vertices the shape has.

    Returns:
        One RGBA tuple per vertex, or empty when there are none or the count
        disagrees with the vertex count.
    """
    colors = data.fields.get("vertex_colors_rgba")
    if not isinstance(colors, list) or len(colors) != vertices:
        return []
    return [(float(r), float(g), float(b), float(a)) for r, g, b, a in colors]


def _material(
    block: Block, by_index: dict[int, Block]
) -> tuple[tuple[float, float, float] | None, tuple[float, float, float] | None, float]:
    """Read a shape's ``NiMaterialProperty``.

    The layout is confirmed against Greatness7's ``tes3`` (MIT): ambient,
    diffuse, specular and emissive colors, then shine, then alpha. Only the
    three a renderer here needs are returned; specular and shine are read past
    rather than dropped by guessing at their width.

    Args:
        block: The shape.
        by_index: Every parsed block, by index.

    Returns:
        Diffuse color, emissive color and opacity. The colors are ``None`` when
        the shape has no material property -- *undescribed*, not black -- and
        opacity is ``1.0``.
    """
    for link in block.fields.get("properties_links") or []:
        prop = by_index.get(int(link))
        if prop is None or prop.type_name != "NiMaterialProperty":
            continue
        diffuse = prop.fields.get("diffuse_xyz")
        emissive = prop.fields.get("emissive_xyz")
        alpha = prop.fields.get("alpha", 1.0)
        return (
            _triple(diffuse),
            _triple(emissive),
            float(alpha) if isinstance(alpha, (int, float)) else 1.0,
        )
    return (None, None, 1.0)


def _triple(value: object) -> tuple[float, float, float] | None:
    """Coerce a retained vector to a color, or ``None``.

    Args:
        value: The decoded ``vector3``, or whatever was in its place.

    Returns:
        Three floats, or ``None`` when the field was absent.
    """
    if not isinstance(value, (list, tuple)) or len(value) != 3:
        return None
    return (float(value[0]), float(value[1]), float(value[2]))


def _alpha(block: Block, by_index: dict[int, Block]) -> tuple[bool, bool, float]:
    """Read a shape's ``NiAlphaProperty``.

    Blending and testing live in the *property flags*, not in a field of their
    own -- confirmed against ``tes3``, which exposes them as masks over the
    same ``u16``: blending at ``0x0001`` and testing at ``0x0200``. The byte
    after the flags is the test reference.

    The two are independent, and conflating them is the mistake worth avoiding:
    foliage routinely sets testing without blending, and treating "has an alpha
    property" as "is translucent" makes every leaf in the game fade.

    Args:
        block: The shape.
        by_index: Every parsed block, by index.

    Returns:
        Whether blending is on, whether testing is on, and the test reference
        normalised to 0-1.
    """
    for link in block.fields.get("properties_links") or []:
        prop = by_index.get(int(link))
        if prop is None or prop.type_name != "NiAlphaProperty":
            continue
        flags = prop.fields.get("flags", 0)
        flags = int(flags) if isinstance(flags, int) else 0
        threshold = prop.fields.get("threshold", 0)
        threshold = int(threshold) if isinstance(threshold, int) else 0
        return (
            bool(flags & _ALPHA_BLEND_MASK),
            bool(flags & _ALPHA_TEST_MASK),
            threshold / 255.0,
        )
    return (False, False, 0.0)


def _decal_slots(block: Block, by_index: dict[int, Block]) -> list[str]:
    """Collect every decal a shape names, in slot order.

    The reader numbers them ``decal_0``, ``decal_1`` and upward for as many as
    the property declares, so this walks until one is missing rather than
    stopping at a fixed count -- the format sets no limit, and ``7decals.NIF``
    is in the vanilla corpus specifically because it exceeds any guess.

    Stops at the first gap rather than scanning to some ceiling: the slots are
    written consecutively, so a missing one means the end. Scanning past it
    would be looking for something the format cannot produce.

    Args:
        block: The shape.
        by_index: Every parsed block, by index.

    Returns:
        The decal texture paths, first slot first. Empty when there are none.
    """
    found: list[str] = []
    index = 0
    while True:
        path = _texture_slot(block, by_index, f"decal_{index}")
        if not path:
            return found
        found.append(path)
        index += 1


def _texture_slot(block: Block, by_index: dict[int, Block], slot: str) -> str:
    """Find the texture a shape's ``NiTexturingProperty`` names in one slot.

    Args:
        block: The shape.
        by_index: Every parsed block, by index.
        slot: Which slot to read, matching the names
            :func:`~wraithguard.nif.reader._slot_name` assigns -- ``"base"``
            for the diffuse texture, ``"glow"`` for the self-illumination map,
            ``"dark"``, ``"decal_0"``, and so on. All of these are real NIF
            texture slots the shape names directly, unlike the normal and
            specular maps a viewer finds by filename convention instead.

    Returns:
        The normalised texture path, or ``""`` when the shape has no
        ``NiTexturingProperty``, or that property has nothing in this slot.
    """
    for prop_index in block.fields.get("properties_links") or []:
        prop = by_index.get(int(prop_index))
        if prop is None or prop.type_name != "NiTexturingProperty":
            continue
        slots = prop.fields.get("textures")
        # A parsed NiTexturingProperty always carries its texture_slots as a dict;
        # this guards a property whose field never parsed (a truncated block), which
        # the geometry path never reaches, so the guarded skip is dead.
        if not isinstance(slots, dict):  # pragma: no cover
            continue  # pragma: no cover
        source = by_index.get(int(slots.get(slot, -1)))
        if source is None:
            continue
        reference = source.fields.get("external_or_internal")
        if isinstance(reference, str) and reference.strip():
            return normalise_texture(reference)
    return ""


@dataclass(frozen=True, slots=True)
class TreeNode:
    """One entry in a mesh's block hierarchy.

    Attributes:
        index: The block index, so a reader can match it against a survey.
        type_name: The block's type.
        name: Its name, empty for blocks that have none.
        note: A short summary of what it carries, when that is worth saying.
        children: Nested entries.
    """

    index: int
    type_name: str
    name: str = ""
    note: str = ""
    children: list[TreeNode] = field(default_factory=list)


#: Blocks worth listing under their parent even though they are not children in
#: the scene-graph sense. These are exactly the things the 3D view cannot show:
#: a property is not geometry, a controller draws nothing, and a collision node
#: is invisible in a render but decides whether you can walk through the thing.
_ATTACHMENT_LINKS: tuple[str, ...] = ("properties_links", "data", "controller", "skin_instance")


def _name_of(block: Block) -> str:
    """A block's name, or empty when it has none.

    Args:
        block: Any block.

    Returns:
        The name as text.
    """
    name = block.fields.get("name")
    return name if isinstance(name, str) else ""


def block_tree(parsed: NifFile) -> list[TreeNode]:
    """Describe a file's block hierarchy.

    A structural companion to :func:`world_meshes`: that returns what can be
    drawn, this returns everything, including the blocks that never appear in
    a render and are often the reason a conflict matters.

    Args:
        parsed: A parsed file. Child links need ``geometry=True``; without them
            the result is a flat list of roots, which is honest rather than
            wrong.

    Returns:
        One entry per root, with children nested.
    """
    by_index = {block.index: block for block in parsed.blocks}
    seen: set[int] = set()

    def describe(block: Block) -> str:
        """A short summary of what a block holds, for its tree row.

        Args:
            block: The block to describe.

        Returns:
            A few words, or an empty string for block types with nothing worth
            summarising -- the row still shows the type name.
        """
        if block.type_name == "NiTriShapeData":
            return f"{block.fields.get('num_vertices', 0)} verts, {block.fields.get('num_triangles', 0)} tris"
        if block.type_name == "NiSourceTexture":
            reference = block.fields.get("external_or_internal")
            return normalise_texture(reference) if isinstance(reference, str) else "embedded"
        if block.type_name in COLLISION_HINT:
            return "collision"
        if block.type_name.endswith("Controller"):
            return "animated"
        if block.type_name == "NiSkinInstance":
            return "skinned"
        return ""

    def build(index: int, depth: int) -> TreeNode | None:
        """Build the tree node for one block and everything under it.

        Args:
            index: The block to build from.
            depth: How far down the graph this is, for the depth guard.

        Returns:
            The node, or ``None`` when the block is missing, already placed, or
            deeper than the guard allows.
        """
        block = by_index.get(index)
        if block is None or index in seen or depth > _MAX_DEPTH:
            return None
        seen.add(index)
        node = TreeNode(
            index=block.index,
            type_name=block.type_name,
            name=_name_of(block),
            note=describe(block),
        )
        for key in _ATTACHMENT_LINKS:
            value = block.fields.get(key)
            targets = value if isinstance(value, list) else [value]
            for target in targets:
                if isinstance(target, int) and target >= 0:
                    child = build(int(target), depth + 1)
                    if child is not None:
                        node.children.append(child)
        for child_index in block.fields.get("children_links") or []:
            if child_index >= 0:
                child = build(int(child_index), depth + 1)
                if child is not None:
                    node.children.append(child)
        return node

    roots = [build(index, 0) for index in find_roots(parsed)]
    trees = [node for node in roots if node is not None]
    # Anything the walk never reached is still in the file and still worth
    # showing -- an orphaned block is exactly the sort of thing a person
    # opening this view wants to know about.
    trees.extend(
        TreeNode(b.index, b.type_name, _name_of(b), describe(b))
        for b in parsed.blocks
        if b.index not in seen
    )
    return trees


def bounds(meshes: list[Mesh]) -> tuple[tuple[float, float, float], tuple[float, float, float]]:
    """Axis-aligned bounds of a whole mesh set.

    A viewer needs this before it can frame anything, and it is cheap once the
    vertices are already in world space.

    Args:
        meshes: The meshes to measure.

    Returns:
        Minimum and maximum corners. Both are the origin when there is nothing
        to measure, so a caller never has to special-case an empty file.
    """
    points = [v for mesh in meshes for v in mesh.vertices]
    if not points:
        return ((0.0, 0.0, 0.0), (0.0, 0.0, 0.0))
    return (
        (min(p[0] for p in points), min(p[1] for p in points), min(p[2] for p in points)),
        (max(p[0] for p in points), max(p[1] for p in points), max(p[2] for p in points)),
    )
