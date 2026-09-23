"""A self-contained 3D page for looking at one or two meshes.

Answers the question a triangle count cannot: *does the winner actually look
different?* Two meshes side by side, orbitable, is the shortest route from "this
conflict is flagged" to a decision.

**Why three.js is embedded as a classic script.** Modern three.js ships ESM
only, split across ``three.module.js`` and ``three.core.js``, and **ES module
scripts do not load from ``file://``** -- the origin is ``null`` and the CORS
check fails. These pages are written to disk and opened in a browser, so a
module build cannot work here regardless of how it is packaged. A single
CommonJS file with no ``require()`` of its own runs as an ordinary script behind
a three-line ``exports`` shim, which is what these pages need. That was verified
rather than assumed: the shim was exercised and used to build a real
``BufferGeometry`` with computed normals before any of this was written. The
file is vendored at :mod:`wraithguard.viz.library`; since r186 removed upstream's
own ``build/three.cjs`` it is bundled from the ESM sources by
``tools/build_three_cjs.py`` -- see that module for the full story.

**Why the orbit controls are ours.** three.js ships ``OrbitControls.js``, but it
imports the bare specifier ``'three'``, which would drag ESM and an import map
back into a page that has just gone to some trouble to avoid both. Dragging to
rotate is forty lines; an import map that works from ``file://`` is not.

**Why the page is self-contained.** The existing visualisations are single files
a user can move, keep or send to someone. Referencing a sibling script would
break that, and would also behave differently in the in-app viewers
(``pywebview``, ``tkinterweb``) than in a browser. One file behaves the same
everywhere.

**Finding the three.js build itself, and ``ViewerError``, now live in**
:mod:`wraithguard.viz.library`. That code was never specific to a NIF -- it
locates and reads one vendored asset -- and the texture comparison's WebGL
wipe view needs the identical bytes. This module still owns everything that
*is* NIF-specific: the scene payload, the orbit controls, and the page
template below.
"""

from __future__ import annotations

import base64
import html
import json
import struct
import zlib
from collections.abc import Callable
from typing import TYPE_CHECKING, Final, cast

from wraithguard.images import browser_image, dds_passthrough
from wraithguard.logging_setup import get_logger
from wraithguard.viz import ViewerError, three_source
from wraithguard.viz.library import EXTRA_SLOTS_JS

if TYPE_CHECKING:
    from collections.abc import Mapping, Sequence

    from wraithguard.nif.geometry import (
        Mesh,
        MorphAnimation,
        Transform,
        TransformAnimation,
        TreeNode,
        UVAnimation,
    )
    from wraithguard.nif.textures import Resolved, TextureResolver

LOG = get_logger(__name__)

#: Colors for the two sides of a comparison: the overridden mesh and the one
#: that wins. Deliberately not red and green -- the point is to tell them apart,
#: not to say which is better, and roughly 1 in 12 men cannot separate those.
_COLOURS: Final[tuple[str, str]] = ("#6ba3ff", "#ffb86b")

__all__ = [
    "BlobSink",
    "ViewerError",
    "build_cell_viewer_page",
    "build_viewer_page",
    "inline_blob",
    "three_source",
]


#: Turns one deflated blob into whatever the page should read it from: a
#: ``{"b64": ...}`` for a standalone file, or a ``{"url": ...}`` for a served
#: one. The page branches on which key is present, so the two modes share every
#: line of rendering code and cannot drift apart.
BlobSink = Callable[[bytes, str], dict[str, str]]


def inline_blob(blob: bytes, content_type: str = "") -> dict[str, str]:
    """Carry a blob inside the document.

    Args:
        blob: The bytes. Deflated for geometry, a PNG for a texture.
        content_type: The MIME type, used only when serving. Inline blobs that
            the page hands to an ``Image`` need it as a data URL prefix; the
            rest are read as raw bytes and do not.

    Returns:
        A base64 entry, or a data URL when one is needed.
    """
    encoded = base64.b64encode(blob).decode("ascii")
    if content_type.startswith("image/"):
        return {"url": f"data:{content_type};base64,{encoded}"}
    return {"b64": encoded}


def _packed(values: list[float] | list[int], fmt: str) -> bytes:
    """Pack numbers as a deflated, base64 binary blob.

    Measured on a 204k-triangle mesh, against writing the same numbers as JSON
    decimals:

    * JSON decimals -- 5.40 MB.
    * base64 typed arrays -- 4.91 MB. Almost no gain: base64 costs 33% and
      hands most of the binary saving straight back.
    * base64 of *deflated* typed arrays -- 1.86 MB, a third of the JSON.

    So the compression is doing the work, not the binary encoding, and it costs
    nothing on the page: browsers inflate this natively with
    ``DecompressionStream`` and no library.

    Args:
        values: The numbers to pack.
        fmt: A :mod:`struct` format character, ``f`` or ``I``.

    Returns:
        The deflated bytes. How they reach the page -- inline as base64, or
        over loopback as a fetch -- is the caller's decision, which is what
        lets one builder produce both a served page and a standalone file.
    """
    raw = struct.pack(f"<{len(values)}{fmt}", *values)
    return zlib.compress(raw, 6)


def texture_bytes(
    resolved: Resolved, resolver: TextureResolver, max_dimension: int | None = None
) -> tuple[bytes, str] | None:
    """Turn a resolved texture into something a browser shows, or give up quietly.

    Read through the resolver rather than off the path directly, because most
    of the base game's textures are inside ``Morrowind.bsa`` and have no path
    at all.

    Args:
        resolved: The outcome of a texture lookup.
        resolver: The resolver that produced it, which knows how to fetch the
            bytes whether they are loose or archived.
        max_dimension: When set, a DDS is decoded from a mip within this size
            rather than full -- what a cell view uses to cap textures at, say,
            1K. The decode memo keys on this too, so a texture decoded small for
            a cell and full for a comparison do not shadow one another.

    Returns:
        Bytes a browser can display and their MIME type, or ``None`` when
        nothing provides the texture or it is in a format the decoders do not
        handle. Both are ordinary in a mod collection, and a broken texture is
        a finding about the mod rather than a reason to fail the whole view.

        A PNG comes back as the bytes it arrived as -- browsers decode PNG
        better than anything here would, and re-encoding could only lose
        fidelity. Everything else is decoded and re-encoded as PNG.
    """
    if not resolved.found:
        return None
    # A session-wide memo on the resolver: DDS decoding is slow and the same
    # texture recurs across meshes and across cells, so decode each once. Read
    # via getattr so any resolver-shaped double (the tests use several) still
    # works -- it simply decodes every time, which is only a test's concern.
    cache: dict[str, tuple[bytes, str] | None] | None = getattr(resolver, "_decode_cache", None)
    key = f"{resolved.reference}@{max_dimension or 0}"
    if cache is not None and key in cache:
        return cache[key]
    raw = resolver.read(resolved)
    result: tuple[bytes, str] | None = None
    if raw is not None:
        try:
            result = browser_image(raw, max_dimension)
        except Exception as exc:  # noqa: BLE001 - any bad texture is a finding, never a crash
            # Broad on purpose: this now runs inside a served request (a lazy
            # texture decode), and an odd format or malformed file must leave
            # the mesh untextured, not raise out of the request handler.
            LOG.debug("cannot decode %s: %s", resolved.reference, exc)
    if cache is not None:
        cache[key] = result
    return result


#: The block-compressed formats a desktop GPU can always upload (S3TC). Only
#: these are passed through; BC7 (bptc) is not universal and BC4/BC5 need channel
#: reconstruction, so both keep the CPU decode.
_S3TC: Final = frozenset({"dxt1", "dxt3", "dxt5"})


def _inline_texture_slot(
    resolved: Resolved, resolver: TextureResolver, sink: BlobSink
) -> dict[str, str] | None:
    """A texture slot for the eager path, GPU-compressed when it safely can be.

    For an S3TC (DXT1/3/5) DDS -- a format every desktop GPU uploads directly --
    the slot carries the raw compressed mip chain and its dimensions, so the page
    hands the blocks straight to the GPU and never decodes them on the CPU.
    Anything else (BC7, BC4/BC5, uncompressed, an actual PNG) is decoded to an
    image exactly as before, so the change only ever removes work, never adds a
    format the page cannot show.

    Used by the eager ``sink`` path (the single-mesh viewer, inline or served).
    The cell viewer's lazy ``publish`` path is deliberately left on the decoder:
    detecting the format there would mean reading every texture up front, the very
    cost its on-demand streaming exists to avoid.

    Args:
        resolved: The resolved texture.
        resolver: The resolver that fetches its bytes.
        sink: Turns a blob into a page reference (base64 or a served URL).

    Returns:
        The slot dict -- ``{"url"|"b64", ...}`` plus, for a passed-through
        texture, ``"compressed"``, ``"cw"``, ``"ch"`` and a ``"levels"`` string
        (``"w,h,len;..."``) -- or ``None`` when nothing could be read.
    """
    raw = resolver.read(resolved)
    if raw is not None:
        compressed = dds_passthrough(raw)
        if compressed is not None and compressed.format in _S3TC:
            slot = sink(compressed.data, "application/octet-stream")
            slot["compressed"] = compressed.format
            slot["cw"] = str(compressed.width)
            slot["ch"] = str(compressed.height)
            slot["levels"] = ";".join(f"{w},{h},{n}" for w, h, n in compressed.levels)
            return slot
    shown = texture_bytes(resolved, resolver)
    return sink(*shown) if shown else None


def _mesh_payload(
    meshes: list[Mesh],
    sink: BlobSink,
    resolver: TextureResolver | None = None,
    cache: dict[str, dict[str, str] | None] | None = None,
    publish: Callable[[Resolved], dict[str, str] | None] | None = None,
) -> list[dict[str, object]]:
    """Reduce meshes to the packed arrays the page needs.

    Positions are flattened before packing, because the page inflates straight
    into a ``Float32Array``: any nesting would only cost the browser a pass to
    undo.

    Args:
        meshes: World-space meshes.
        sink: Turns a blob into something the page can read -- base64 for a
            standalone file, a URL for a served one.
        resolver: Finds texture files across the data folders. Without one the
            meshes are sent untextured, which is a complete and useful view.
        cache: Decoded textures by reference, shared across sides. One texture
            is routinely used by many shapes and by both sides of a
            comparison; decoding a 2048px image once per shape would dominate
            the time to open a view.
        publish: When given, called with each resolved texture to obtain a page
            reference *without decoding it here* -- it registers a lazy server
            endpoint that decodes on first fetch and returns ``{"url": ...}`` (or
            ``None``). This is what keeps a cell's build fast: textures stream and
            decode on demand rather than blocking the page on hundreds of decodes.
            When omitted, textures are decoded inline through ``sink`` as before.

    Returns:
        One entry per mesh, JSON-ready.
    """
    decoded: dict[str, dict[str, str] | None] = cache if cache is not None else {}
    payload: list[dict[str, object]] = []
    for mesh in meshes:
        # A particle cloud has no triangles but is still drawn (as points); a
        # surface with no triangles is nothing to draw.
        if not mesh.triangles and not mesh.points:
            continue
        positions: list[float] = []
        for vertex in mesh.vertices:
            positions.extend(vertex)
        indices: list[int] = []
        for triangle in mesh.triangles:
            indices.extend(triangle)
        uvs: list[float] = []
        # Only send UVs when there is one per vertex. A partial set would make
        # three.js index past the end of the attribute and draw nothing at all,
        # which is a worse outcome than an untextured mesh.
        if len(mesh.uvs) == len(mesh.vertices):
            for u, v in mesh.uvs:
                # NIF measures V downward and OpenGL upward, so an untouched
                # copy renders every texture upside down.
                uvs.extend((u, 1.0 - v))
        image: dict[str, str] | None = None
        extras: dict[str, dict[str, str]] = {}

        def resolve_slot(reference: str) -> dict[str, str] | None:
            """Resolve one texture-slot reference, sharing the page's cache.

            Every optional slot below -- glow, dark, decal, detail, gloss,
            bump -- is a real ``NiTexturingProperty`` entry rather than a
            filename guess, so each is resolved the same way the base texture
            is: through the folder order, not by pattern-matching a sibling
            file.

            Args:
                reference: The slot's normalised texture path, already
                    checked for truthiness by the caller.

            Returns:
                The decoded blob for the page, or ``None`` when it could not
                be read.
            """
            if reference not in decoded:
                found = resolver.resolve(reference)  # type: ignore[union-attr]
                if publish is not None:
                    decoded[reference] = publish(found)
                else:
                    decoded[reference] = _inline_texture_slot(found, resolver, sink)  # type: ignore[arg-type]
            return decoded[reference]

        if resolver is not None and mesh.texture and uvs:
            image = resolve_slot(mesh.texture)
            # The mesh names only its diffuse texture; OpenMW finds the rest by
            # name. Offering them is the only way a normal or specular map in
            # a texture pack is ever visible here, since no NIF mentions one.
            for suffix, resolved in resolver.siblings(mesh.texture).items():
                key = f"{mesh.texture}{suffix}"
                if key not in decoded:
                    if publish is not None:
                        decoded[key] = publish(resolved)
                    else:
                        aux = texture_bytes(resolved, resolver)
                        decoded[key] = sink(*aux) if aux else None
                # Bound to a name rather than subscripted twice: a subscript is
                # re-evaluated, so the None check does not narrow the second
                # one -- which is a real hole, not a typing nicety, if the
                # cache is ever mutated between the two.
                found_aux = decoded[key]
                if found_aux is not None:
                    extras[suffix] = found_aux
        glow = resolve_slot(mesh.glow) if resolver is not None and mesh.glow and uvs else None
        dark = resolve_slot(mesh.dark) if resolver is not None and mesh.dark and uvs else None
        decals = (
            [found for found in (resolve_slot(path) for path in mesh.decals) if found]
            if resolver is not None and mesh.decals and uvs
            else []
        )
        detail = resolve_slot(mesh.detail) if resolver is not None and mesh.detail and uvs else None
        gloss = resolve_slot(mesh.gloss) if resolver is not None and mesh.gloss and uvs else None
        bump = resolve_slot(mesh.bump) if resolver is not None and mesh.bump and uvs else None
        # A terrain blend layer needs its per-vertex alpha (the layer's coverage);
        # everything else drops it, so a NIF's own vertex alpha cannot silently
        # turn a static translucent. Four components for a layer, three otherwise.
        if mesh.blend_layer >= 0 and mesh.vertex_colors:
            color_values = [c for rgba in mesh.vertex_colors for c in rgba]
            color_items = 4
        elif mesh.vertex_colors:
            color_values = [c for rgba in mesh.vertex_colors for c in rgba[:3]]
            color_items = 3
        else:
            color_values = []
            color_items = 3
        payload.append(
            {
                "name": mesh.name,
                "texture": mesh.texture,
                "collision": mesh.collision,
                "water": mesh.water,
                "image": image,
                "glow": glow,
                "dark": dark,
                # A list, in slot order, because that is paint order: decals
                # composite over one another and the last one declared is the
                # one on top. Unresolvable ones are dropped rather than sent
                # as holes, so the order that reaches the shader is the order
                # of the decals that actually exist.
                "decals": decals,
                # dark, detail, gloss and decal are all drawn through the
                # onBeforeCompile shader hook (attachExtraSlots, in the JS
                # below) rather than a MeshPhongMaterial property -- Phong has
                # exactly one multiply-the-surface slot and one
                # modulate-specular slot, not four, and specularMap is
                # already spoken for by an OpenMW-style _spec map.
                "detail": detail,
                "gloss": gloss,
                # bump is sent unconditionally too -- what it means depends on
                # which convention drew the file, and only the caller (via the
                # "Bump as normal (MGE)" control) decides whether to use it.
                "bump": bump,
                "extras": extras,
                "positions": sink(_packed(positions, "f"), ""),
                "indices": sink(_packed(indices, "I"), ""),
                "uvs": sink(_packed(uvs, "f"), "") if uvs else None,
                # Per-vertex colors as a packed float run, like every other
                # attribute, rather than JSON numbers: a 30,000-vertex shape
                # is 120,000 floats, and spelling those out as text is larger
                # than the mesh.
                "colors": sink(_packed(color_values, "f"), "") if color_values else None,
                # 3 or 4, so the page sets the color attribute's itemSize right --
                # a blend layer's fourth channel is its coverage alpha.
                "colorItems": color_items,
                # -1 for anything but a terrain blend layer; 0 is the opaque base,
                # 1+ are faded in over it in order.
                "blendLayer": mesh.blend_layer,
                # The material the *file* describes. Until now the viewer's
                # alpha controls applied one global guess -- a 0.5 cutoff on
                # everything -- because nothing carried the real values. These
                # let each shape use its own, and turn those controls from a
                # guess into an override of a known default.
                "diffuse": list(mesh.diffuse) if mesh.diffuse else None,
                "emissive": list(mesh.emissive) if mesh.emissive else None,
                "opacity": mesh.opacity,
                "alphaBlend": mesh.alpha_blend,
                "alphaTest": mesh.alpha_test,
                "alphaThreshold": mesh.alpha_threshold,
                "vertexCount": len(mesh.vertices),
                "triangleCount": len(mesh.triangles),
                # A scrolling/scaling texture animation, or None. Each channel is
                # a list of [time, value] keys; the page slides the material's UV
                # offset and tiling from these on its clock.
                "uvAnim": _uv_anim_payload(mesh.uv_anim),
                # A node keyframe animation (sway/spin/slide), or None. Carries
                # the parent/rest matrices and the rotation/translation/scale
                # keys; the page applies the delta as a per-frame matrix.
                "transformAnim": _transform_anim_payload(mesh.transform_anim),
                # A visibility animation (blink on/off), or None: [[time, 0|1], ...].
                "visAnim": (
                    [[time, int(visible)] for time, visible in mesh.vis_anim]
                    if mesh.vis_anim
                    else None
                ),
                # The shape's node transform as a 4x4, folded by the page into the
                # instance matrix. Identity on a baked mesh (the transform is in
                # the vertices already), so this changes nothing there.
                "nodeWorld": _mat4_cols(mesh.node_world),
                # A vertex-morph animation (cloth ripple), or None. The page
                # blends the delta targets over the base each frame on the CPU.
                "morphAnim": _morph_anim_payload(mesh.morph_anim),
                # A particle cloud (mist, glowbugs): drawn as points, not a
                # surface, and drifted on the clock.
                "points": mesh.points,
            }
        )
    return payload


def _morph_anim_payload(morph_anim: MorphAnimation | None) -> dict[str, object] | None:
    """Serialise a :class:`~wraithguard.nif.geometry.MorphAnimation` for the page.

    Args:
        morph_anim: The shape's vertex-morph animation, or ``None``.

    Returns:
        A dict of ``targets`` (each ``{"weights": [[t, w], ...], "deltas":
        [dx, dy, dz, ...]}`` -- deltas flattened to match the position buffer's
        layout), or ``None``. Morph meshes are low-poly cloth, so plain JSON is
        small enough and avoids the async buffer plumbing.
    """
    if morph_anim is None:
        return None
    return {
        "targets": [
            {
                "weights": [[time, weight] for time, weight in target.weights],
                "deltas": [component for delta in target.deltas for component in delta],
            }
            for target in morph_anim.targets
        ]
    }


def _mat4_cols(transform: Transform) -> list[float]:
    """A :class:`~wraithguard.nif.geometry.Transform` as a column-major 4x4.

    The 16 floats a ``THREE.Matrix4`` reads: rotation scaled into the upper-left
    3x3, translation in the last column, laid out column by column. Local to the
    viewer (rather than importing the scene layer's twin) so ``nif`` does not
    depend on ``scene``.

    Args:
        transform: The transform to lay out.

    Returns:
        Sixteen floats, column-major.
    """
    r = transform.rotation
    s = transform.scale
    tx, ty, tz = transform.translation
    return [
        r[0][0] * s, r[1][0] * s, r[2][0] * s, 0.0,
        r[0][1] * s, r[1][1] * s, r[2][1] * s, 0.0,
        r[0][2] * s, r[1][2] * s, r[2][2] * s, 0.0,
        tx, ty, tz, 1.0,
    ]  # fmt: skip


def _transform_anim_payload(anim: TransformAnimation | None) -> dict[str, object] | None:
    """Serialise a :class:`~wraithguard.nif.geometry.TransformAnimation` for the page.

    Args:
        anim: The node keyframe animation, or ``None``.

    Returns:
        A dict with the ``parent``/``rest`` matrices and the ``rotation``
        (``[t, w, x, y, z]``), ``translation`` (``[t, x, y, z]``) and ``scale``
        (``[t, v]``) key lists, or ``None`` when there is no animation. Empty key
        lists are kept so the page can fall back to the rest value per channel.
    """
    if anim is None:
        return None
    return {
        "parent": _mat4_cols(anim.parent),
        "rest": _mat4_cols(anim.rest),
        "rotation": [[time, w, x, y, z] for time, (w, x, y, z) in anim.rotation],
        "translation": [[time, x, y, z] for time, (x, y, z) in anim.translation],
        "scale": [[time, value] for time, value in anim.scale],
    }


def _uv_anim_payload(uv_anim: UVAnimation | None) -> dict[str, list[list[float]]] | None:
    """Serialise a :class:`~wraithguard.nif.geometry.UVAnimation` for the page.

    Args:
        uv_anim: The shape's UV animation, or ``None``.

    Returns:
        A dict of the four channels (each ``[[time, value], ...]``), or ``None``
        when there is no animation. Only non-empty channels are included, so the
        payload stays small for the common single-channel scroll.
    """
    if uv_anim is None:
        return None
    channels = {
        "uOffset": uv_anim.u_offset,
        "vOffset": uv_anim.v_offset,
        "uTiling": uv_anim.u_tiling,
        "vTiling": uv_anim.v_tiling,
    }
    return {
        name: [[time, value] for time, value in keys] for name, keys in channels.items() if keys
    }


#: The single-blob texture slots on a mesh payload (``decals`` is a list and
#: ``extras`` a map, handled separately).
_TEXTURE_SLOTS = ("image", "glow", "dark", "detail", "gloss", "bump")


def _hoist_textures(scenes: list[dict[str, object]]) -> list[dict[str, str]]:
    """Collapse texture blobs shared across meshes into one indexed table.

    A decoded texture is a single dict object reused -- by identity -- for every
    mesh that references it (see the cache in :func:`_mesh_payload`). Serialising
    the mesh payloads as they stand writes that texture's bytes once *per mesh*,
    which is fine for a single NIF but ruinous for a cell: hundreds of meshes
    share a handful of wall and crate textures, and the inline copies balloon the
    page to gigabytes. This walks every texture slot, replaces each blob with an
    integer index into a de-duplicated table (keyed by object identity), and
    returns the table for the page to rehydrate from -- so each unique texture is
    carried exactly once no matter how many meshes draw it.

    Args:
        scenes: The scene payloads, mutated in place: every texture slot becomes
            an ``int`` index, or is left ``None``.

    Returns:
        The unique texture blobs, in index order.
    """
    table: list[dict[str, str]] = []
    by_id: dict[int, int] = {}

    def index_of(blob: dict[str, str]) -> int:
        """The blob's index in the table, appending it on first sight."""
        found = by_id.get(id(blob))
        if found is None:
            found = len(table)
            by_id[id(blob)] = found
            table.append(blob)
        return found

    for scene in scenes:
        meshes = cast("list[dict[str, object]]", scene.get("meshes") or [])
        for mesh in meshes:
            for slot in _TEXTURE_SLOTS:
                blob = mesh.get(slot)
                if blob is not None:
                    mesh[slot] = index_of(cast("dict[str, str]", blob))
            decals = cast("list[dict[str, str]]", mesh.get("decals") or [])
            mesh["decals"] = [index_of(blob) for blob in decals]
            extras = cast("dict[str, dict[str, str]]", mesh.get("extras") or {})
            mesh["extras"] = {suffix: index_of(blob) for suffix, blob in extras.items()}
    return table


def _tree_payload(nodes: list[TreeNode]) -> list[dict[str, object]]:
    """Reduce a block tree to JSON the page can render.

    Args:
        nodes: Roots from :func:`~wraithguard.nif.geometry.block_tree`.

    Returns:
        Nested entries.
    """
    return [
        {
            "index": node.index,
            "type": node.type_name,
            "name": node.name,
            "note": node.note,
            "children": _tree_payload(node.children),
        }
        for node in nodes
    ]


def build_viewer_page(
    sides: list[tuple[str, list[Mesh]]],
    title: str = "Mesh viewer",
    *,
    sink: BlobSink | None = None,
    library_url: str = "",
    trees: list[list[TreeNode]] | None = None,
    resolver: TextureResolver | None = None,
    edit: Mapping[str, object] | None = None,
) -> str:
    """Build an HTML page showing one or more meshes.

    Two shapes from one template. With no arguments beyond the meshes it
    produces a **standalone file**: three.js and every byte of geometry inline,
    portable, and multi-megabyte. Given a ``sink`` and a ``library_url`` it
    produces a **served page** of a few kilobytes that fetches both.

    The difference is confined to how bytes arrive. The rendering code is
    identical in both, which is the point: a fallback that shares no code with
    the primary path is a second implementation waiting to rot.

    Args:
        sides: ``(label, meshes)`` pairs. One side shows a single mesh; two
            shows a comparison, each in its own color and its own viewport.
        title: The page title.
        sink: How geometry reaches the page. Defaults to inlining it.
        library_url: Where to fetch three.js. Empty means inline it.
        resolver: Finds texture files across the data folders. Omitted, the
            meshes render in a flat color, which stays a complete view rather
            than a degraded one.
        trees: Block hierarchies, one per side. Optional because the geometry
            view stands on its own; supplied, it fills the structure pane with
            what a render cannot show -- collision nodes, controllers, and
            every block that never draws.
        edit: When given, turns the page into an editor. A mapping of
            ``{"url": <apply endpoint>, "filename": <download name>, "blocks":
            {index: {"type": ..., "fields": [FieldView-shaped dicts]}}}`` -- the
            inspector reads it to show each block's fields, and POSTs edits to
            ``url``. Omitted (the default), the page is the read-only viewer
            exactly as before, carrying none of the editor code.

    Returns:
        The whole HTML document.

    Raises:
        ViewerError: If three.js is needed and missing.
    """
    blob_sink = sink or inline_blob
    shared_textures: dict[str, dict[str, str] | None] = {}
    scenes: list[dict[str, object]] = [
        {
            "label": label,
            "color": _COLOURS[index % len(_COLOURS)],
            "meshes": _mesh_payload(meshes, blob_sink, resolver, shared_textures),
            "tree": _tree_payload(trees[index]) if trees and index < len(trees) else [],
        }
        for index, (label, meshes) in enumerate(sides)
    ]
    return _assemble_page(
        scenes, title=title, library_url=library_url, single_sided=False, edit=edit
    )


def build_cell_viewer_page(
    label: str,
    groups: Sequence[tuple[list[Mesh], Sequence[float]]],
    *,
    sink: BlobSink | None = None,
    library_url: str = "",
    resolver: TextureResolver | None = None,
    single_sided: bool = True,
    title: str = "Cell preview",
    publish_texture: Callable[[Resolved], dict[str, str] | None] | None = None,
    focus_indices: set[int] | None = None,
    ref_ids: list[list[str]] | None = None,
    record_types: list[str] | None = None,
    adjacent_flags: list[bool] | None = None,
    skirt_alone_flags: list[bool] | None = None,
    sky_texture_url: str = "",
    sky_textures: Mapping[str, str] | None = None,
    star_texture_url: str = "",
    cell_info: Mapping[str, object] | None = None,
    object_info: Mapping[str, object] | None = None,
) -> str:
    """Build a viewer page that draws a cell with instanced geometry.

    Each group is one unique model plus the per-placement transforms that put it
    everywhere it appears. The model's meshes are sent once and drawn as a
    ``THREE.InstancedMesh``; a cell that reuses a crate three hundred times costs
    one crate mesh and three hundred matrices, not three hundred meshes. Intended
    for the *served* path (a ``sink`` + ``library_url``), so geometry, textures
    and instance matrices stream as separate blobs rather than inlining into one
    document the in-app view cannot hold.

    Args:
        label: The cell's label (the single side's name).
        groups: ``(meshes, matrices)`` per unique model, where ``matrices`` is a
            flat run of column-major 4x4s (16 floats per instance) from
            :func:`wraithguard.scene.build.matrix4_columns`.
        sink: How blobs reach the page (loopback publisher, or inline default).
        library_url: Where to fetch three.js; empty inlines it.
        resolver: Texture resolver for the meshes, or ``None`` for untextured.
        single_sided: Render opaque faces one-sided so interior walls do not
            block the view; on by default for cells.
        title: The page title.
        publish_texture: When given, textures are served lazily (decoded on first
            fetch) rather than decoded into the page at build time -- see
            :func:`_mesh_payload`. This is what lets a cell open on its geometry
            in seconds and have textures stream in.
        focus_indices: Indices into ``groups`` the camera should frame on. Set for
            an exterior cell's terrain, which spans exactly the cell, so a stray
            object far outside it cannot throw the view off. Empty (the default)
            frames on everything, which is right for an interior.
        ref_ids: The object id each instance places, one list per group (same
            order and length as that group's instances). Clicking an instance
            names its id -- the CS "Object ID". Omitted, a click names the shape.
        record_types: The record type per group (Static, Light, ...), for the
            viewer's per-type visibility toggles. Omitted, no type toggles appear.
        adjacent_flags: Whether each group belongs to a neighbouring cell rather
            than the picked one, one per group. When any are set, the viewer
            offers an "Adjacent cells" toggle that shows or hides them together;
            they start hidden so the opening view is the picked cell alone.
        skirt_alone_flags: Whether each group is the picked cell's lone-cell skirt,
            one per group. The viewer shows these only while neighbours are hidden,
            so the plinth wraps the single cell then and the block-perimeter skirt
            (an ``adjacent`` group) takes over when neighbours are shown.
        sky_texture_url: A Morrowind sky texture for the water to reflect and the
            scene to sit against, or ``""`` for a plain gradient sky. Only used
            when the cell has water to reflect it.
        sky_textures: A ``{weather: url}`` map of per-weather sky textures for the
            viewer's Weather selector to swap between (the sky dome and the water
            reflection). ``sky_texture_url`` is the default (clear) shown at load.
        star_texture_url: Morrowind's starfield texture, faded in behind the sky
            as the sun drops below the horizon, or ``""`` for no stars.
        cell_info: When given, puts the page into *cell mode* -- a purpose-built
            layout with a cell-info panel (the audit numbers) instead of a lone
            NIF's shape list, the single-mesh map toggles tucked away, and grouped
            controls. A mapping of ``{"label", "stats": [(name, value), ...],
            "missing": [model, ...]}``. Omitted, the page is the single-mesh
            viewer's layout exactly as before.
        object_info: Per-object provenance keyed by lower-cased object id, each
            ``{"id", "type", "model", "definedBy": [...], "placedBy": [...]}`` --
            the plugins that define and place the object, in load order. Powers the
            "ori" readout the left panel shows when a mesh is clicked.

    Returns:
        The whole HTML document.
    """
    blob_sink = sink or inline_blob
    focus = focus_indices or set()
    per_group_ids = ref_ids or []
    per_group_types = record_types or []
    per_group_adjacent = adjacent_flags or []
    per_group_skirt_alone = skirt_alone_flags or []
    shared_textures: dict[str, dict[str, str] | None] = {}
    meshes: list[dict[str, object]] = []
    for index, (base_meshes, matrices) in enumerate(groups):
        count = len(matrices) // 16
        if count == 0:
            continue
        entries = _mesh_payload(
            base_meshes, blob_sink, resolver, shared_textures, publish=publish_texture
        )
        if not entries:
            continue
        # One matrix blob per group, shared (by identity) across the group's
        # meshes -- hoisting is only for textures, so this stays inline in each
        # entry as a fetchable blob ref, published once by the sink.
        instances = blob_sink(_packed(list(matrices), "f"), "")
        group_ids = per_group_ids[index] if index < len(per_group_ids) else None
        group_type = per_group_types[index] if index < len(per_group_types) else ""
        is_adjacent = per_group_adjacent[index] if index < len(per_group_adjacent) else False
        is_skirt_alone = (
            per_group_skirt_alone[index] if index < len(per_group_skirt_alone) else False
        )
        for entry in entries:
            entry["instances"] = instances
            entry["instanceCount"] = count
            if group_ids:
                entry["refIds"] = group_ids
            if group_type:
                entry["recordType"] = group_type
            if index in focus:
                entry["focus"] = True
            if is_adjacent:
                entry["adjacent"] = True
            if is_skirt_alone:
                entry["skirtAlone"] = True
        meshes.extend(entries)
    # White, not a side colour: the coloured sides exist to tell two compared
    # meshes apart, but a cell is one scene, and that tint multiplies into every
    # untextured or vertex-coloured mesh -- turning a whole cell's worth of them
    # blue. White lets textures and vertex colours read true.
    scenes: list[dict[str, object]] = [
        {"label": label, "color": "#ffffff", "meshes": meshes, "tree": []}
    ]
    return _assemble_page(
        scenes,
        title=title,
        library_url=library_url,
        single_sided=single_sided,
        edit=None,
        sky_texture_url=sky_texture_url,
        sky_textures=sky_textures,
        star_texture_url=star_texture_url,
        cell_info=cell_info,
        object_info=object_info,
    )


def _assemble_page(
    scenes: list[dict[str, object]],
    *,
    title: str,
    library_url: str,
    single_sided: bool,
    edit: Mapping[str, object] | None,
    sky_texture_url: str = "",
    sky_textures: Mapping[str, str] | None = None,
    star_texture_url: str = "",
    cell_info: Mapping[str, object] | None = None,
    object_info: Mapping[str, object] | None = None,
) -> str:
    """Render assembled scene payloads into the final HTML document.

    The shared tail of :func:`build_viewer_page` and :func:`build_cell_viewer_page`:
    hoist shared textures, serialise, and fill the template. Kept in one place so
    the single-mesh and cell builders cannot drift in how they escape data, embed
    three.js, or wire the editor.

    Args:
        scenes: The per-side scene payloads (meshes already reduced).
        title: The page title.
        library_url: Where to fetch three.js; empty inlines it.
        single_sided: Render opaque faces one-sided (Morrowind's backface
            culling), so an interior's near walls do not block the view.
        edit: Editor descriptor, or ``None`` for a read-only view.
        sky_texture_url: A sky texture for the water to reflect, or ``""`` for a
            plain gradient sky.
        sky_textures: A ``{weather: url}`` map for the Weather selector, or
            ``None``.
        star_texture_url: The starfield texture faded in at night, or ``""``.
        cell_info: The cell's audit info (see
            :func:`build_cell_viewer_page`), or ``None`` for the single-mesh
            layout. Puts the page into cell mode when present.
        object_info: Per-object provenance keyed by lower-cased id (which plugins
            define and place each object, plus its winning mesh), for the "ori"
            readout shown when a mesh is clicked; ``None`` when not in cell mode.

    Returns:
        The whole HTML document.
    """
    empty = all(not scene["meshes"] for scene in scenes)
    if empty:
        LOG.info("viewer built with no geometry: %s", title)
    # Shared textures are carried once in a side table and referenced by index;
    # without this a cell's meshes -- hundreds sharing a few wall textures --
    # would each inline their own copy and the page would reach gigabytes.
    textures = _hoist_textures(scenes)
    # json.dumps escapes nothing HTML-significant by default, and the payload
    # carries mod-authored names. "</script>" inside a string would end the
    # element early, so the sequence is broken up rather than trusted.
    data = json.dumps(scenes, separators=(",", ":")).replace("</", "<\\/")
    textures_data = json.dumps(textures, separators=(",", ":")).replace("</", "<\\/")
    cell_info_data = json.dumps(cell_info or None, separators=(",", ":")).replace("</", "<\\/")
    object_info_data = json.dumps(object_info or None, separators=(",", ":")).replace("</", "<\\/")
    sky_textures_data = json.dumps(sky_textures or {}, separators=(",", ":")).replace("</", "<\\/")
    library = "" if library_url else three_source()
    # The CommonJS build needs its two globals to exist *before* it runs and
    # the namespace pulled back out *after*, whether it arrives inline or over
    # the wire. Serving it without the shim was a real bug: the file ran
    # against an undefined ``exports`` and the page reported "THREE is not
    # defined". Classic scripts execute in order, so three tags do for the
    # served case exactly what one does for the inline one.
    prologue = "<script>var module = {exports:{}}, exports = module.exports;</script>"
    epilogue = "<script>var THREE = module.exports;</script>"
    if library_url:
        middle = f'<script src="{html.escape(library_url, quote=True)}"></script>'
    else:
        middle = f"<script>\n{library}\n</script>"
    library_block = f"{prologue}\n{middle}\n{epilogue}"
    if edit:
        edit_data = json.dumps(edit, separators=(",", ":")).replace("</", "<\\/")
        editor_block = _EDITOR_JS.replace("__EDIT_DATA__", edit_data)
    else:
        editor_block = ""
    return (
        _PAGE.replace("__TITLE__", html.escape(title))
        .replace("__LIBRARY_BLOCK__", library_block)
        .replace("__TEXTURES__", textures_data)
        .replace("__DATA__", data)
        .replace("__EMPTY__", "true" if empty else "false")
        .replace("__SINGLE_SIDED__", "true" if single_sided else "false")
        .replace("__EXTRA_SLOTS__", EXTRA_SLOTS_JS)
        .replace("__SKY_URL__", html.escape(sky_texture_url, quote=True))
        .replace("__STAR_URL__", html.escape(star_texture_url, quote=True))
        .replace("__SKY_TEXTURES__", sky_textures_data)
        .replace("__CELL_INFO__", cell_info_data)
        .replace("__OBJECT_INFO__", object_info_data)
        .replace("__EDIT_JS__", editor_block)
    )


#: The page. Written as one template rather than assembled from fragments: it
#: is read far more often than it is edited, and a reader needs to see the
#: whole document to judge it.
# A **raw** string, and it has to be. The template is verbatim HTML, CSS and
# JavaScript, so a backslash in it belongs to the language being emitted rather
# than to Python. Without the `r`, the eleven `\n` sequences in the shader
# assembly below are turned into real newlines *by Python* and land inside
# JavaScript string literals, which is a syntax error that takes the whole
# page down with it -- the viewer renders nothing and the console blames a line
# that looks fine in the source.
#
# That is exactly how this broke: the shader code was the first thing in the
# template to need an escape, so the missing `r` had been harmless until then.
_PAGE: Final[str] = r"""<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"><title>__TITLE__</title>
<style>
 :root{--ink:#e6e6e6;--dim:#9aa0aa;--line:#333945;--panel:#20242c}
 html,body{margin:0;height:100%;background:#15171c;color:var(--ink);
   font:13px/1.5 "Segoe UI",system-ui,sans-serif;display:flex;flex-direction:column}
 .panel{display:flex;flex-wrap:wrap;gap:6px 16px;align-items:center;
   margin:0;padding:8px 10px;border-bottom:1px solid var(--line);
   background:var(--panel);font-size:12.5px}
 .panel .ctl{display:flex;align-items:center;gap:5px}
 .panel .ctl.off{opacity:.45}
 .panel label{color:var(--dim);cursor:pointer}
 .panel .swatch{width:10px;height:10px;border-radius:2px;display:inline-block}
 .panel button{background:#2c313a;color:#d7dae0;border:1px solid var(--line);
   border-radius:4px;padding:4px 9px;cursor:pointer;font:inherit}
 .panel button:hover{background:#3d4450}
 .panel .spacer{flex:1}
 .panel input[type=range]{width:78px;accent-color:#6f8fb8;cursor:pointer}
 #body{flex:1;display:flex;min-height:0}
 body.nopanel #tree,body.nopanel #inspector{display:none}
 #rpanel{display:none;width:260px;min-width:200px;max-width:40%;overflow:auto;
   border-left:1px solid var(--line);background:var(--panel);padding:8px 10px;
   font-size:12.5px;resize:horizontal}
 body.rpanel #rpanel{display:block}
 body.norpanel #rpanel{display:none}
 #rpanel h3{margin:0 0 8px;font:600 12px/1.4 "Segoe UI",system-ui,sans-serif;color:#9ecbff}
 #rpanel details{margin-bottom:6px;border:1px solid var(--line);border-radius:5px;
   background:#20242c;overflow:hidden}
 #rpanel details>summary{cursor:pointer;padding:6px 9px;list-style:none;
   font:600 11px/1.4 "Segoe UI",system-ui,sans-serif;color:var(--dim);
   text-transform:uppercase;letter-spacing:.05em;background:#262b33}
 #rpanel details>summary::-webkit-details-marker{display:none}
 #rpanel details[open]>summary{color:#9ecbff}
 #rpanel .accbody{padding:6px 9px}
 #rpanel .row{display:flex;align-items:center;justify-content:space-between;gap:8px;padding:3px 0}
 #rpanel .row label{color:var(--dim)}
 #rpanel .row input[type=range]{width:120px;accent-color:#6f8fb8;cursor:pointer}
 #rpanel .row select{background:#2c313a;color:var(--ink);border:1px solid var(--line);
   border-radius:4px;padding:3px 6px;font:inherit}
 #tree h3.cellhead{margin:0 0 6px;font:600 12px/1.4 "Segoe UI",system-ui,sans-serif;
   color:#9ecbff;text-transform:none;letter-spacing:0}
 #tree table.audit{border-collapse:collapse;width:100%;margin-bottom:8px;
   font-family:Consolas,"Cascadia Mono",monospace}
 #tree table.audit td{padding:1px 4px}
 #tree table.audit td.n{text-align:right;color:#cfe3ff}
 #tree details.acc{margin-bottom:6px}
 #tree details.acc>summary{cursor:pointer;color:var(--dim);
   font:600 11px/1.6 "Segoe UI",system-ui,sans-serif;text-transform:uppercase;letter-spacing:.05em}
 #tree{width:300px;min-width:160px;max-width:50%;overflow:auto;padding:8px 10px;
   border-right:1px solid var(--line);background:#181b21;font-size:12px;
   font-family:Consolas,"Cascadia Mono",monospace;white-space:nowrap;resize:horizontal}
 #tree ul{list-style:none;margin:0;padding-left:14px}
 #tree>ul{padding-left:0}
 #tree li{line-height:1.55}
 #tree .ty{color:#9ecbff}
 #tree .nm{color:var(--ink)}
 #tree .no{color:var(--dim)}
 #tree .ix{color:#5b6270;margin-right:5px}
 #tree h4{margin:10px 0 3px;font:600 11px/1.4 "Segoe UI",system-ui,sans-serif;
   color:var(--dim);text-transform:uppercase;letter-spacing:.05em}
 #tree h4:first-child{margin-top:0}
 #tree ul.shapes{padding-left:0}
 #tree ul.shapes li{display:flex;align-items:baseline;gap:5px;white-space:normal}
 #tree ul.shapes input{margin:0;flex:none;accent-color:#6f8fb8;cursor:pointer}
 #tree .shapename{cursor:pointer;text-decoration:underline dotted transparent}
 #tree .shapename:hover{text-decoration-color:#9ecbff;color:#fff}
 #tree .orisel{margin:0 0 12px;padding-bottom:10px;border-bottom:1px solid var(--line)}
 #tree .orisel h3.cellhead{color:#cfe3ff}
 #tree .oriid{color:#fff;font-weight:600;white-space:normal;word-break:break-all;margin-bottom:5px}
 #tree .orikv{color:var(--dim);white-space:normal;word-break:break-all;line-height:1.5}
 #tree .orik{color:#9ecbff}
 #tree .orisel li.oriwin{color:#cfe3ff}
 /* min-width:0 lets the stage shrink below the canvas's current pixel width when
    a side panel opens -- without it a flex item's min size is its content, so the
    panel pushes the row wider than the window and a horizontal scrollbar appears.
    overflow:hidden keeps a momentarily-oversized canvas (before resize() runs)
    from doing the same. */
 #stage{flex:1;position:relative;min-height:0;min-width:0;overflow:hidden}
 canvas{display:block}
 #stats{position:absolute;bottom:8px;left:10px;opacity:.75;pointer-events:none}
 #hint{position:absolute;bottom:8px;right:10px;opacity:.55;pointer-events:none}
 #picked{position:absolute;top:8px;left:10px;background:rgba(0,0,0,.6);color:#cfe3ff;
   padding:3px 8px;border-radius:4px;font:12px monospace;pointer-events:none;display:none}
 #none{position:absolute;inset:0;display:flex;align-items:center;justify-content:center;
   text-align:center;padding:2rem;opacity:.8}
 #tree li[data-block-index]{cursor:pointer}
 #tree li.sel{background:#094771;box-shadow:inset 3px 0 0 #6ba3ff}
 #inspector{display:none;flex-direction:column;width:360px;min-width:220px;max-width:50%;
   overflow:auto;border-left:1px solid var(--line);background:var(--panel);
   font-family:Consolas,"Cascadia Mono",monospace;font-size:12px;resize:horizontal}
 #inspector .insp-head{padding:11px 12px;border-bottom:1px solid var(--line)}
 #inspector .insp-head .nm{color:#9ecbff;font-weight:600}
 #inspector .insp-head .sub{color:var(--dim);font-size:11px;margin-top:2px}
 #inspector .insp-empty{padding:14px 12px;color:var(--dim)}
 #inspector .sect .sh{padding:8px 12px;color:var(--dim);font-size:11px;letter-spacing:1px;
   font-weight:600;border-bottom:1px solid var(--line);
   font-family:"Segoe UI",system-ui,sans-serif}
 #inspector .sect .body{padding:4px 12px 10px}
 #inspector .frow{padding:6px 0;border-bottom:1px solid #262b33}
 #inspector .frow:last-child{border-bottom:0}
 #inspector .lab{display:flex;justify-content:space-between;align-items:center;gap:8px;margin-bottom:4px}
 #inspector .fname{color:var(--ink)}
 #inspector .frow.ro .fname{color:var(--dim)}
 #inspector .kind{color:var(--dim);font-size:10px;border:1px solid var(--line);border-radius:4px;padding:0 5px}
 #inspector input.f{width:100%;background:#2c313a;border:1px solid var(--line);border-radius:5px;
   color:var(--ink);padding:5px 8px;font:inherit}
 #inspector input.f:focus{outline:none;border-color:#6ba3ff}
 #inspector .roval{color:var(--dim)}
 #inspector .frow.dirty input.f{border-color:#c9a227}
 .panel .save-btn{border-color:#3f6a99;color:#9ecbff}
 .panel .save-status{color:var(--dim);font-size:12px}
</style></head><body>
<div class="panel" id="controls"></div>
<div id="body">
  <div id="tree"></div>
  <div id="stage">
    <div id="stats"></div>
    <div id="picked"></div>
    <div id="hint">drag to orbit &middot; WASD to move &middot; shift/right-drag to pan &middot; wheel to zoom &middot; click to name</div>
  </div>
  <div id="rpanel"></div>
  <div id="inspector"></div>
</div>
__LIBRARY_BLOCK__
<script>
(function () {
  var scenes = __DATA__;
  // Morrowind faces are backface-culled (one-sided); for a cell we honour that
  // so an interior's near walls cull away and you can see in. Off for a single
  // item, where two-sided is the more forgiving default.
  var singleSided = __SINGLE_SIDED__;
  // A Morrowind sky texture to reflect in the water (and hang behind the scene),
  // or "" for a plain gradient sky. Served over loopback like any other texture.
  var skyUrl = "__SKY_URL__";
  // Per-weather sky textures ({weather: url}) the Weather selector swaps between.
  var skyTextures = __SKY_TEXTURES__;
  // Morrowind's starfield, faded in behind the night sky, or "" for none.
  var starUrl = "__STAR_URL__";
  // In cell mode the page carries the cell's audit numbers and uses a layout
  // built for a whole cell (info panel, grouped controls) rather than for a lone
  // NIF. Null for the single-mesh viewer, which keeps its original layout.
  var cellInfo = __CELL_INFO__;
  var cellMode = !!cellInfo;
  // Per-object provenance ({id_lower: {id, type, model, definedBy, placedBy}}),
  // and the currently clicked object -- together they drive the "ori" readout the
  // left panel shows when a mesh is picked. Null until something is clicked.
  var objectInfo = __OBJECT_INFO__ || {};
  var selected = null;
  // Textures are carried once in a side table and referenced by index (see
  // _hoist_textures); put each blob back on the mesh so the render code below
  // reads m.image/m.glow/... exactly as before. A null slot stays null; an
  // index of 0 is a real entry, so the test is against null, not truthiness.
  var textures = __TEXTURES__;
  function tex(i) { return (i === null || i === undefined) ? null : textures[i]; }
  scenes.forEach(function (spec) {
    (spec.meshes || []).forEach(function (m) {
      m.image = tex(m.image); m.glow = tex(m.glow); m.dark = tex(m.dark);
      m.detail = tex(m.detail); m.gloss = tex(m.gloss); m.bump = tex(m.bump);
      m.decals = (m.decals || []).map(tex);
      if (m.extras) {
        var rebuilt = {};
        Object.keys(m.extras).forEach(function (s) { rebuilt[s] = tex(m.extras[s]); });
        m.extras = rebuilt;
      }
    });
  });
  var stage = document.getElementById("stage");
  var controls = document.getElementById("controls");
  if (__EMPTY__) {
    stage.innerHTML = '<div id="none">No geometry could be read from this mesh.' +
      '<br>That is a limit of the reader, not a statement about the file.</div>';
    return;
  }

  function inflateBytes(bytes) {
    if (typeof DecompressionStream === "undefined") {
      return Promise.reject(new Error("this browser has no DecompressionStream"));
    }
    var stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream("deflate"));
    return new Response(stream).arrayBuffer();
  }

  // A blob is either carried in the document or fetched from the loopback
  // server. Everything after this point is identical either way.
  function load(ref) {
    if (ref.url) {
      return fetch(ref.url).then(function (r) {
        if (!r.ok) throw new Error("could not fetch geometry (" + r.status + ")");
        return r.arrayBuffer();
      }).then(function (buf) { return inflateBytes(new Uint8Array(buf)); });
    }
    var bin = atob(ref.b64), bytes = new Uint8Array(bin.length);
    for (var i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    return inflateBytes(bytes);
  }

  function unpack(spec) {
    return Promise.all(spec.meshes.map(function (m) {
      var jobs = [load(m.positions), load(m.indices)];
      jobs.push(m.uvs ? load(m.uvs) : Promise.resolve(null));
      jobs.push(m.colors ? load(m.colors) : Promise.resolve(null));
      // Instance matrices for a cell group: one deflated run of column-major
      // 4x4s, decoded like any other attribute blob.
      jobs.push(m.instances ? load(m.instances) : Promise.resolve(null));
      return Promise.all(jobs).then(function (bufs) {
        m.positions = new Float32Array(bufs[0]);
        m.indices = new Uint32Array(bufs[1]);
        m.uvs = bufs[2] ? new Float32Array(bufs[2]) : null;
        m.colors = bufs[3] ? new Float32Array(bufs[3]) : null;
        m.instances = bufs[4] ? new Float32Array(bufs[4]) : null;
        return m;
      });
    }));
  }

  Promise.all(scenes.map(unpack)).then(build).catch(function (err) {
    stage.innerHTML = '<div id="none">Could not decode the geometry in this page.' +
      '<br>' + String(err && err.message ? err.message : err) + '</div>';
  });

  function build() {
    var renderer = new THREE.WebGLRenderer({antialias: true});
    renderer.setPixelRatio(window.devicePixelRatio || 1);
    stage.appendChild(renderer.domElement);
    // S3TC (DXT1/3/5) lets a texture's compressed blocks go straight to the GPU
    // with no CPU decode. Universal on desktop, but checked rather than assumed:
    // without it, a "compressed" slot falls back to leaving that map off (the
    // shape still draws, just untextured) rather than uploading garbage.
    var S3TC_FORMATS = null;
    (function () {
      var ext = renderer.getContext().getExtension("WEBGL_compressed_texture_s3tc");
      if (ext) S3TC_FORMATS = {
        dxt1: ext.COMPRESSED_RGBA_S3TC_DXT1_EXT,
        dxt3: ext.COMPRESSED_RGBA_S3TC_DXT3_EXT,
        dxt5: ext.COMPRESSED_RGBA_S3TC_DXT5_EXT
      };
    })();
    // Build one texture for a slot: a compressed passthrough when the slot names
    // a GPU format we support, else the ordinary image path (unchanged). Returns
    // the texture synchronously -- the right object type for the wiring below --
    // and fills its pixels once the bytes arrive, exactly like the image path.
    function makeSlotTexture(slot, sRGB) {
      if (!slot) return null;
      if (slot.compressed) {
        if (!S3TC_FORMATS || !S3TC_FORMATS[slot.compressed]) return null;  // untextured, not broken
        var levels = slot.levels ? slot.levels.split(";").map(function (s) {
          return s.split(",").map(Number);
        }) : [];
        var ctex = new THREE.CompressedTexture([], slot.cw | 0, slot.ch | 0,
                                               S3TC_FORMATS[slot.compressed]);
        if (sRGB) ctex.colorSpace = THREE.SRGBColorSpace;
        ctex.wrapS = ctex.wrapT = THREE.RepeatWrapping;
        ctex.magFilter = THREE.LinearFilter;
        ctex.minFilter = levels.length > 1 ? THREE.LinearMipmapLinearFilter : THREE.LinearFilter;
        ctex.generateMipmaps = false;
        function fillCompressed(bytes) {
          var off = 0, mips = [];
          levels.forEach(function (lv) {  // lv = [width, height, byteLength]
            mips.push({data: bytes.subarray(off, off + lv[2]), width: lv[0], height: lv[1]});
            off += lv[2];
          });
          ctex.mipmaps = mips;
          ctex.image = {width: slot.cw | 0, height: slot.ch | 0};
          ctex.needsUpdate = true;
          draw();
        }
        if (slot.b64) {
          // Inline: the blocks are already in the page, so decode them straight
          // away -- no fetch, no decode, the pixels are ready this frame.
          var bin = atob(slot.b64), buf = new Uint8Array(bin.length);
          for (var i = 0; i < bin.length; i++) buf[i] = bin.charCodeAt(i);
          fillCompressed(buf);
        } else if (slot.url) {
          fetch(slot.url).then(function (r) { return r.arrayBuffer(); }).then(function (b) {
            fillCompressed(new Uint8Array(b));
          }).catch(function () { /* a bad fetch leaves the map off, never throws */ });
        }
        return ctex;
      }
      if (!slot.url) return null;  // the image path needs a data/loopback URL
      var image = new Image();
      var tex = new THREE.Texture(image);
      if (sRGB) tex.colorSpace = THREE.SRGBColorSpace;
      tex.wrapS = tex.wrapT = THREE.RepeatWrapping;
      image.onload = function () { tex.needsUpdate = true; draw(); };
      image.src = slot.url;
      return tex;
    }
    var scene = new THREE.Scene();
    scene.background = new THREE.Color(0x15171c);
    var camera = new THREE.PerspectiveCamera(45, 1, 0.1, 200000);

    // One viewport, one camera, one set of geometry groups toggled on and off.
    // Separate panes each framed their own mesh independently, which is the
    // one thing a comparison must not do: two objects at different scales look
    // identical when each is fitted to its own viewport.
    var pivot = new THREE.Group();
    var groups = [];
    scene.add(pivot);

    // Record types present (Static, Light, Activator, ...) -> a checkbox each, so
    // a whole category (light halos, marker boxes) can be hidden at once.
    var typesInScene = {};
    var typeStates = {};
    var collisionOn = false;
    // Adjacent (neighbouring-cell) geometry starts shown: the user opted into it
    // in the picker, so it is visible from the off, with a toggle to hide it.
    var adjacentOn = true;
    function applyVisibility() {
      // Neighbours are "on screen" only when some exist and the toggle is on. The
      // lone-cell skirt (the picked cell's full plinth) shows exactly when they are
      // not, handing the outer edge over to the block-perimeter skirt when they are.
      var adjacentActive = anyAdjacent && adjacentOn;
      scene.traverse(function (o) {
        if (!o.isMesh) return;
        var typeOk = typeStates[o.userData.recordType || ""] !== false;
        var colOk = !o.userData.collision || collisionOn;
        var adjOk = !o.userData.adjacent || adjacentOn;
        var skirtOk = !o.userData.skirtAlone || !adjacentActive;
        // A visibility animation blinks the shape off for stretches; it composes
        // with the toggles rather than overriding them (a hidden type stays hidden).
        var visOk = o.userData.visHidden !== true;
        o.visible = typeOk && colOk && adjOk && skirtOk && visOk;
      });
      // What the water refracts changed, so its cached target is stale.
      viewDirty = true;
    }

    var textured = true;
    // Whether anything in this view has an OpenMW auxiliary normal map, which
    // decides whether the control for them is worth offering at all.
    var anyNormalMaps = false;
    // Same reasoning, for the specular maps OpenMW finds beside the diffuse
    // texture by name.
    var anySpecularMaps = false;
    // And for glow: a real NIF texture slot rather than a filename guess, but
    // still worth gating the control on, since most meshes have none.
    var anyGlowMaps = false;
    // Same reasoning as glow -- a real slot, just a rarer one.
    var anyDarkMaps = false;
    var anyDetailMaps = false;
    var anyGlossMaps = false;
    var anyDecalMaps = false;
    // Whether any mesh has a bump-slot texture at all -- not whether it is
    // currently being drawn as anything, since what it means is a choice
    // the "Bump as normal (MGE)" checkbox makes, not this flag.
    var anyBumpMaps = false;
    // Whether anything is collision-only geometry -- physics shapes a
    // RootCollisionNode carries that the game never draws. Same reasoning:
    // no point offering a toggle that would always be a no-op.
    var anyCollision = false;
    // Whether any geometry belongs to a neighbouring cell, which decides
    // whether the "Adjacent cells" toggle is worth offering at all.
    var anyAdjacent = false;

    // Injects dark, detail, gloss and decal straight into MeshPhongMaterial's
    // own fragment shader, string-patched at the two chunks that already do
    // the equivalent work for the base map and the specular map. Kept to
    // three.js's built-in Phong lighting rather than a shader written from
    // scratch -- the risk in a hand-rolled lighting model is getting the
    // lighting wrong, and nothing here needs to touch it, only what feeds it.
    //
    // Checked live, not baked in at attach time: a material only recompiles
    // when something sets needsUpdate, so every checkbox below that toggles
    // one of these sets it on the meshes it affects, and the next compile
    // reads whatever is checked *then*. Whether a given layer is even a
    // candidate is decided once, up front (only mount the ones the mesh
    // actually has); whether it is currently drawn is decided every compile.
__EXTRA_SLOTS__
    // Which layers the *mesh viewer* draws: one checkbox per slot. Passed in
    // rather than read inside the shared helper, because the texture
    // comparison has entirely different controls over the same slots.
    function wantsSlot(slot) {
      var box = {detail: typeof detailBox !== "undefined" ? detailBox : null,
                 dark: typeof darkBox !== "undefined" ? darkBox : null,
                 gloss: typeof glossBox !== "undefined" ? glossBox : null,
                 decal: typeof decalBox !== "undefined" ? decalBox : null}[slot];
      return !!(box && box.checked);
    }

    // The sun the water glints off. A fixed direction in the view's Y-up world
    // frame (the group's Z-up->Y-up rotation is already applied by the time
    // anything reads a world vector).
    var sunDirection = new THREE.Vector3(0.6, 0.7, 0.4).normalize();

    // Morrowind's sky texture, loaded once and sampled by the water shader as its
    // reflection (its gradient runs horizon-to-zenith). Null when none was served,
    // in which case the shader falls back to a procedural blue gradient.
    // Morrowind does not stretch one sky texture across the whole dome -- it tiles
    // a small cloud texture many times over it (Skies IV notes ~25 repeats in the
    // default game). So the sky textures repeat rather than clamp: SKY_TILE * SKY_TILE
    // copies across the dome.
    var SKY_TILE = 5;
    function tileSky(tex) {
      tex.wrapS = tex.wrapT = THREE.RepeatWrapping;
      tex.repeat.set(SKY_TILE, SKY_TILE);
      return tex;
    }
    var skyTex = null;
    if (skyUrl) {
      skyTex = new THREE.TextureLoader().load(skyUrl, function () { draw(); });
      skyTex.colorSpace = THREE.SRGBColorSpace;
      tileSky(skyTex);
    }

    // Sky dome + time-of-day. The dome hangs Morrowind's own sky texture behind
    // the scene; the time control swings the sun and re-tints the sky (and the
    // water's reflection of it) from night through dawn to day to dusk.
    var skyMesh = null;
    var starMesh = null;
    var skyTint = new THREE.Color(1.0, 1.0, 1.0);
    var timeOfDay = 12.0;
    // A weather colour multiplier applied to the sky and the water's reflection --
    // clear is white (no change); cloud/fog/storm grey it down, ashstorm/blight
    // tint it. Set by the Weather selector, folded in by applyTimeOfDay.
    var weatherMult = new THREE.Color(1.0, 1.0, 1.0);
    function clamp01(v) { return Math.max(0.0, Math.min(1.0, v)); }

    // A large inside-out sphere carrying the sky texture (or a plain tint when
    // none was served). Drawn behind everything and never writing depth.
    function makeSky(radius) {
      var geom = new THREE.SphereGeometry(radius, 32, 16);
      var mat = skyTex
        ? new THREE.MeshBasicMaterial({map: skyTex, side: THREE.BackSide, depthWrite: false, fog: false})
        : new THREE.MeshBasicMaterial({color: 0x6a86a8, side: THREE.BackSide, depthWrite: false, fog: false});
      var mesh = new THREE.Mesh(geom, mat);
      mesh.renderOrder = -1000;
      return mesh;
    }

    // The starfield: a dome just inside the sky dome, drawn additively so its
    // black background adds nothing and only the stars show. Faded in by opacity
    // as the sun drops (set in applyTimeOfDay). Null when no star texture served.
    function makeStars(radius) {
      if (!starUrl) return null;
      var tex = new THREE.TextureLoader().load(starUrl, function () { draw(); });
      tex.colorSpace = THREE.SRGBColorSpace;
      tex.wrapS = tex.wrapT = THREE.RepeatWrapping;
      var mat = new THREE.MeshBasicMaterial({
        map: tex, side: THREE.BackSide, depthWrite: false, fog: false,
        transparent: true, blending: THREE.AdditiveBlending, opacity: 0.0
      });
      var mesh = new THREE.Mesh(new THREE.SphereGeometry(radius, 32, 16), mat);
      mesh.renderOrder = -999;
      return mesh;
    }

    // Drive the sun and the sky/water tints from a time of day (0..24). This does
    // not touch the scene's own Phong lighting -- that stays on the Light / Ambient
    // controls -- so the two never fight; it is the sky and the water's look that
    // follow the clock.
    function applyTimeOfDay(t) {
      timeOfDay = t;
      var dayFrac = (t - 6.0) / 12.0;          // 0 at 06:00, 1 at 18:00
      var a = dayFrac * Math.PI;               // 0..PI across the daylit arc
      var elev = Math.sin(a);                  // > 0 sun up, < 0 sun down
      sunDirection.set(Math.cos(a), Math.max(elev, -0.35), 0.45).normalize();
      var up = clamp01((elev + 0.12) / 0.4);   // 0 night .. 1 full day
      var dusk = clamp01(1.0 - Math.abs(elev) / 0.22);  // peaks at the horizon
      var horizon = new THREE.Color().lerpColors(
        new THREE.Color(0.04, 0.06, 0.14), new THREE.Color(0.55, 0.70, 0.82), up
      ).lerp(new THREE.Color(0.85, 0.42, 0.28), dusk * 0.7);
      var zenith = new THREE.Color().lerpColors(
        new THREE.Color(0.02, 0.03, 0.10), new THREE.Color(0.18, 0.36, 0.60), up
      ).lerp(new THREE.Color(0.30, 0.25, 0.45), dusk * 0.5);
      skyTint.copy(new THREE.Color().lerpColors(
        new THREE.Color(0.12, 0.16, 0.30), new THREE.Color(1.0, 1.0, 1.0), up
      ).lerp(new THREE.Color(1.0, 0.62, 0.42), dusk * 0.6));
      // Weather greys down / tints the whole sky and its reflection.
      horizon.multiply(weatherMult);
      zenith.multiply(weatherMult);
      skyTint.multiply(weatherMult);
      for (var i = 0; i < waterMaterials.length; i++) {
        var u = waterMaterials[i].uniforms;
        u.uSunDir.value.copy(sunDirection);
        u.uHorizon.value.copy(horizon);
        u.uZenith.value.copy(zenith);
        u.uSkyTint.value.copy(skyTint);
      }
      if (skyMesh) skyMesh.material.color.copy(skyTint);
      // Stars fade in once the sun is below the horizon, full by deep night.
      if (starMesh) {
        starMesh.material.opacity = clamp01(-elev / 0.18);
        starMesh.visible = starMesh.material.opacity > 0.001;
      }
      viewDirty = true;
    }

    // Per-weather sky textures load lazily and cache. Swapping one points both the
    // sky dome and the water's reflection at the new texture (or the gradient, if
    // that weather's texture was not served).
    var skyTexCache = {};
    if (skyTex) skyTexCache["clear"] = skyTex;  // the default, already loading
    function setSkyTexture(tex) {
      skyTex = tex;
      if (skyMesh) {
        skyMesh.material.map = tex || null;
        skyMesh.material.needsUpdate = true;
      }
      for (var i = 0; i < waterMaterials.length; i++) {
        waterMaterials[i].uniforms.uSky.value = tex || null;
        waterMaterials[i].uniforms.uHasSky.value = tex ? 1.0 : 0.0;
      }
      viewDirty = true;
    }
    function loadSky(weather) {
      if (skyTexCache[weather]) { setSkyTexture(skyTexCache[weather]); return; }
      var url = skyTextures[weather];
      if (!url) { setSkyTexture(null); return; }
      var tex = new THREE.TextureLoader().load(url, function () { draw(); });
      tex.colorSpace = THREE.SRGBColorSpace;
      tileSky(tex);
      skyTexCache[weather] = tex;
      setSkyTexture(tex);
    }

    // Every water material (to advance its clock) and every water mesh (to hide
    // while the refraction/depth target is rendered). Empty for a cell with no
    // water, which then stays on-demand and skips the extra render pass.
    var waterMaterials = [];
    var waterMeshes = [];
    // Whether clock-driven animation (water, UV, sway, blink, morph) plays. A
    // "Play" checkbox drives it; the loop keeps its rAF alive while paused so it
    // can resume. animLoopStarted guards against starting the loop twice.
    var animateOn = true;
    var animLoopStarted = false;
    // Textures that scroll or scale on the clock (a NIF NiUVController): each
    // entry is {tex, anim} where anim carries the offset/tiling key channels.
    // Non-empty here (like waterMaterials) is what keeps the animation loop
    // running for a scene that has no water.
    var uvAnimated = [];
    // Advance a UV animation channel's [[t, v], ...] keys to time ``t`` seconds,
    // looping over the channel's own span. Linear between keys, clamped to the
    // ends; returns ``dflt`` when the channel has no keys.
    function sampleUvKeys(keys, t, dflt) {
      if (!keys || !keys.length) return dflt;
      if (keys.length === 1) return keys[0][1];
      var span = keys[keys.length - 1][0];
      var localT = span > 0 ? (t % span) : 0;
      for (var i = 1; i < keys.length; i++) {
        if (localT <= keys[i][0]) {
          var a = keys[i - 1], b = keys[i];
          var dt = b[0] - a[0];
          var f = dt > 0 ? (localT - a[0]) / dt : 0;
          return a[1] + (b[1] - a[1]) * f;
        }
      }
      return keys[keys.length - 1][1];
    }
    // Set every scrolling texture's offset/repeat for time ``seconds``.
    function advanceUvAnimations(seconds) {
      for (var i = 0; i < uvAnimated.length; i++) {
        var a = uvAnimated[i].anim, tex = uvAnimated[i].tex;
        // NIF V grows downward but the UVs were flipped to (u, 1 - v) on load,
        // so a positive V scroll in the file is a negative offset here.
        tex.offset.set(
          sampleUvKeys(a.uOffset, seconds, 0),
          -sampleUvKeys(a.vOffset, seconds, 0));
        tex.repeat.set(
          sampleUvKeys(a.uTiling, seconds, 1),
          sampleUvKeys(a.vTiling, seconds, 1));
      }
    }

    // Meshes that blink on and off on the clock (a NIF NiVisController). Each
    // entry is {obj, keys}; the loop sets obj.userData.visHidden and re-applies
    // the visibility toggles so the blink composes with them rather than fighting.
    var visAnimated = [];
    // The visibility of a key list [[t, 0|1], ...] at time t: the last key at or
    // before t (a step, not a ramp), looping over the list's own span.
    function visibleAtKeys(keys, t) {
      if (!keys || !keys.length) return true;
      var span = keys[keys.length - 1][0];
      var lt = span > 0 ? (t % span) : 0;
      var vis = keys[0][1];
      for (var i = 0; i < keys.length; i++) {
        if (keys[i][0] <= lt) vis = keys[i][1]; else break;
      }
      return vis !== 0;
    }
    function advanceVisAnimations(t) {
      for (var i = 0; i < visAnimated.length; i++) {
        visAnimated[i].obj.userData.visHidden = !visibleAtKeys(visAnimated[i].keys, t);
      }
      if (visAnimated.length) applyVisibility();  // recompose with the toggles
    }

    // Cloth that ripples by vertex morph (a NIF NiGeomMorpherController). Each
    // entry is {posAttr, base, targets}: the base pose kept pristine, the live
    // position attribute rewritten each frame as base + sum(weight_t * delta_t).
    var morphAnimated = [];
    // A morph weight track [[t, w], ...] at time t, looping over its own span.
    function sampleMorphWeight(keys, t) {
      if (!keys || !keys.length) return 0;
      if (keys.length === 1) return keys[0][1];
      var span = keys[keys.length - 1][0];
      var lt = span > 0 ? (t % span) : 0;
      for (var i = 1; i < keys.length; i++) {
        if (lt <= keys[i][0]) {
          var a = keys[i - 1], b = keys[i], dt = b[0] - a[0], f = dt > 0 ? (lt - a[0]) / dt : 0;
          return a[1] + (b[1] - a[1]) * f;
        }
      }
      return keys[keys.length - 1][1];
    }
    function advanceMorphAnimations(t) {
      for (var e = 0; e < morphAnimated.length; e++) {
        var m = morphAnimated[e], arr = m.posAttr.array, base = m.base, n = base.length;
        for (var i = 0; i < n; i++) arr[i] = base[i];  // start from the base pose
        for (var ti = 0; ti < m.targets.length; ti++) {
          var w = sampleMorphWeight(m.targets[ti].weights, t);
          if (w === 0) continue;
          var d = m.targets[ti].deltas;
          for (var j = 0; j < n; j++) arr[j] += w * d[j];  // base + weight * delta
        }
        m.posAttr.needsUpdate = true;
      }
    }

    // Particle clouds (mist, glowbugs) that drift on the clock. Each entry is
    // {attr, base}: the point positions rewritten each frame as a slow swirl
    // around the emitter's own particle positions.
    var pointsAnimated = [];
    function advancePointsAnimations(t) {
      for (var e = 0; e < pointsAnimated.length; e++) {
        var p = pointsAnimated[e], arr = p.attr.array, base = p.base, n = base.length;
        for (var i = 0; i < n; i += 3) {
          var bx = base[i], by = base[i + 1], bz = base[i + 2];
          // A gentle, per-particle swirl -- amplitudes in game units, phases from
          // the particle's own position so neighbours do not move in lockstep.
          arr[i] = bx + 10.0 * Math.sin(t * 0.5 + by * 0.03);
          arr[i + 1] = by + 7.0 * Math.sin(t * 0.7 + bx * 0.03);
          arr[i + 2] = bz + 10.0 * Math.cos(t * 0.5 + bx * 0.03);
        }
        p.attr.needsUpdate = true;
      }
    }

    // Nodes that sway/spin/slide on the clock (a NIF NiKeyframeController). Each
    // entry precomputes the fixed parts of its delta; ``base`` (an instanced
    // group's per-instance matrices) is set for a group, absent for a lone mesh.
    var xformAnimated = [];
    var _xfL = new THREE.Matrix4();     // the animated local transform L(t)
    var _xfDelta = new THREE.Matrix4(); // parent * L(t) * below (see registerXformAnim)
    var _xfIm = new THREE.Matrix4();    // one instance's base matrix
    var _xfOut = new THREE.Matrix4();   // base * delta for that instance
    var _xfPos = new THREE.Vector3(), _xfScl = new THREE.Vector3();
    var _xfQ = new THREE.Quaternion();
    // Linearly interpolate a vec3 key list [[t,x,y,z],...] to time t (looping
    // over its own span), into ``out``; ``rest`` when the channel has no keys.
    function sampleVec3(keys, t, rest, out) {
      if (!keys || !keys.length) return out.copy(rest);
      var span = keys[keys.length - 1][0];
      var lt = span > 0 ? (t % span) : 0;
      for (var i = 1; i < keys.length; i++) {
        if (lt <= keys[i][0]) {
          var a = keys[i - 1], b = keys[i], dt = b[0] - a[0], f = dt > 0 ? (lt - a[0]) / dt : 0;
          return out.set(
            a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f, a[3] + (b[3] - a[3]) * f);
        }
      }
      var last = keys[keys.length - 1];
      return out.set(last[1], last[2], last[3]);
    }
    // Slerp a quaternion key list [[t,w,x,y,z],...] (NIF w-first order) to time t,
    // into ``out``; ``rest`` when the channel has no keys.
    function sampleQuat(keys, t, rest, out) {
      if (!keys || !keys.length) return out.copy(rest);
      var span = keys[keys.length - 1][0];
      var lt = span > 0 ? (t % span) : 0;
      function set(o, k) { return o.set(k[2], k[3], k[4], k[1]); }  // NIF (w,x,y,z) -> THREE (x,y,z,w)
      for (var i = 1; i < keys.length; i++) {
        if (lt <= keys[i][0]) {
          var a = keys[i - 1], b = keys[i], dt = b[0] - a[0], f = dt > 0 ? (lt - a[0]) / dt : 0;
          set(out, a);
          var qb = set(_xfQ, b);
          return out.slerp(qb, f);
        }
      }
      return set(out, keys[keys.length - 1]);
    }
    function sampleScalar(keys, t, rest) {
      if (!keys || !keys.length) return rest;
      var span = keys[keys.length - 1][0];
      var lt = span > 0 ? (t % span) : 0;
      for (var i = 1; i < keys.length; i++) {
        if (lt <= keys[i][0]) {
          var a = keys[i - 1], b = keys[i], dt = b[0] - a[0], f = dt > 0 ? (lt - a[0]) / dt : 0;
          return a[1] + (b[1] - a[1]) * f;
        }
      }
      return keys[keys.length - 1][1];
    }
    // Set every animated node's matrix (or its group's instance matrices) for t.
    function advanceXformAnimations(seconds) {
      for (var i = 0; i < xformAnimated.length; i++) {
        var e = xformAnimated[i], a = e.anim;
        sampleVec3(a.translation, seconds, e.restP, _xfPos);
        sampleQuat(a.rotation, seconds, e.restQ, _xfQ);
        var s = sampleScalar(a.scale, seconds, e.restS);
        _xfScl.set(s, s, s);
        _xfL.compose(_xfPos, _xfQ, _xfScl);
        // Direct form: above * L(t) * below. No per-frame inverse and no
        // separate nodeWorld fold -- `below` already carries both (parent*rest)^-1
        // and the shape's own node transform, folded once at registration (see
        // registerXformAnim), not every frame.
        _xfDelta.multiplyMatrices(e.parentM, _xfL).multiply(e.below);
        if (e.base) {
          for (var k = 0; k < e.obj.count; k++) {
            _xfIm.fromArray(e.base, k * 16);
            _xfOut.multiplyMatrices(_xfIm, _xfDelta);
            e.obj.setMatrixAt(k, _xfOut);
          }
          e.obj.instanceMatrix.needsUpdate = true;
        } else {
          e.obj.matrix.copy(_xfDelta);
        }
      }
    }
    // Register a drawn mesh's node animation, precomputing its fixed matrices.
    function registerXformAnim(drawn, spec) {
      var pM = new THREE.Matrix4().fromArray(spec.parent);
      var rM = new THREE.Matrix4().fromArray(spec.rest);
      // below = (parent*rest)^-1 * nodeWorld -- everything between the animated
      // node and this shape, as one fixed matrix. The inverse is computed here,
      // once per shape at registration, and never again: previously it was
      // stored as invParentRest and re-multiplied into a fresh delta every
      // frame; folding nodeWorld in now instead means the frame loop above has
      // nothing left to invert or cancel.
      var below = new THREE.Matrix4().multiplyMatrices(pM, rM).invert();
      if (drawn.nodeWorldMat) below.multiply(drawn.nodeWorldMat);
      var rP = new THREE.Vector3(), rQ = new THREE.Quaternion(), rS = new THREE.Vector3();
      rM.decompose(rP, rQ, rS);
      var entry = {obj: drawn, anim: spec, parentM: pM, below: below,
                   restP: rP, restQ: rQ, restS: rS.x};
      if (drawn.isInstancedMesh) entry.base = drawn.instanceBase;
      else drawn.matrixAutoUpdate = false;  // we drive .matrix directly each frame
      xformAnimated.push(entry);
    }
    // Whether the fancy water shader (surface reflection/refraction/caustics and
    // the underwater pass) is on. Off falls the surface back to a plain blue tint
    // and skips the underwater effect -- a lighter, simpler look, and a fallback
    // if the shader misbehaves on a given GPU. Distinct from hiding water entirely.
    var waterShaderOn = true;
    // The plain fallback surface, shared by every water plane while the shader is
    // off: a flat translucent blue, matching the shader's own no-compile fallback.
    var simpleWaterMaterial = null;
    function getSimpleWaterMaterial() {
      if (!simpleWaterMaterial) {
        simpleWaterMaterial = new THREE.MeshBasicMaterial({
          color: new THREE.Color(0.16, 0.34, 0.52),
          transparent: true, opacity: 0.5, depthWrite: false, side: THREE.DoubleSide, fog: false
        });
      }
      return simpleWaterMaterial;
    }
    // Swap each water plane between its shader material and the plain tint.
    function applyWaterShader() {
      for (var i = 0; i < waterMeshes.length; i++) {
        var mesh = waterMeshes[i];
        mesh.material = waterShaderOn ? mesh.userData.shaderMat : getSimpleWaterMaterial();
      }
      viewDirty = true;
    }
    // The refraction pass: the scene rendered without the water, so the water
    // shader can read what lies beneath it (colour) and how deep it is (depth)
    // to tint by depth, refract the bottom, and lay caustics on it. Built lazily
    // at the drawing-buffer size and rebuilt when that changes.
    var refractRT = null;
    var _invViewProj = new THREE.Matrix4();
    // The refraction target shows the *static* scene beneath the water, which only
    // changes when the camera moves -- not every animation frame. So it is
    // re-rendered only when this is set (on any camera change), and reused across
    // the ripple frames in between: the difference between one full scene render
    // per frame and two or three, which is what the depth pass otherwise costs.
    var viewDirty = true;
    // The refraction pass renders at half the drawing-buffer size: it is sampled
    // with distortion, so the softness never shows, and it quarters that pass's
    // fill cost.
    var REFRACT_SCALE = 0.5;
    // The underwater pass: when the camera drops below the surface, the whole
    // frame is rendered to a target and run through a full-screen shader that
    // fogs it toward the murky water colour, lays caustics on the seabed, and
    // wobbles it -- Hrnchamd's underwater effect, in screen space. Built lazily.
    var sceneRT = null;
    var underMat = null, fsScene = null, fsCamera = null;
    // Underwater tunables, held here because the material is built lazily (only
    // once the camera first dips below the surface): the panel writes these and
    // ensureUnderwater seeds the uniforms from them whenever it does build.
    var underState = {fog: 0.0011, causticSharp: 1.5, hue: 0.0};
    var _tmpV = new THREE.Vector3();

    // The water shader. Positions arrive in the mesh's Z-up local frame (the group
    // applies the Z-up->Y-up rotation), so the swell rides local +Z and the local
    // ripple normal is rotated into the view's world frame by modelMatrix before
    // the reflection is computed. The look, in order: a small low swell so the
    // surface never clips the shore; fine per-pixel ripples for the tight,
    // close-together sparkle; a Fresnel-weighted reflection of the sky (strongly,
    // for the ocean sheen); and a sharp sun glint. Deep-water colour is Morrowind's
    // own (openmw.cfg Water_UnderwaterColor 12,30,37).
    var WATER_VERT = `
      uniform float uTime;
      uniform float uWaveScale;
      varying vec3 vWorld;
      varying vec2 vXY;
      // The surface's world-space tangent basis, computed here where modelMatrix
      // is available (it is not declared in the fragment stage), so the fragment
      // can turn a local ripple normal into a world normal without it.
      varying vec3 vT;
      varying vec3 vB;
      varying vec3 vN;
      void main() {
        vec3 pos = position;
        // A gentle, long-wavelength swell. Amplitudes sum to 1.0, and the peak
        // fix below subtracts that sum so the crest only ever reaches the resting
        // level (z += 0) and everything else dips below -- vtastek's fix, so the
        // surface never climbs over the shore or the scum ring at the waterline.
        float t = uTime;
        float swell = sin(dot(pos.xy, vec2(0.0016, 0.0009)) + t * 0.7) * 0.6
                    + sin(dot(pos.xy, vec2(-0.0011, 0.0019)) + t * 0.9) * 0.4;
        // Peak fix + a little extra sink: subtract the swell's max (1.0) so crests
        // reach at most the resting level, then drop 3 more units so even the
        // crests sit just below the waterline and never wash over the shore scum.
        pos.z += (swell - 1.0) * uWaveScale - 3.0;
        vXY = pos.xy;
        mat3 m = mat3(modelMatrix);
        vT = m * vec3(1.0, 0.0, 0.0);
        vB = m * vec3(0.0, 1.0, 0.0);
        vN = m * vec3(0.0, 0.0, 1.0);
        vec4 world = modelMatrix * vec4(pos, 1.0);
        vWorld = world.xyz;
        gl_Position = projectionMatrix * viewMatrix * world;
      }
    `;
    var WATER_FRAG = `
      uniform float uTime;
      uniform vec3 uDeep;
      uniform vec3 uShallow;
      uniform vec3 uSunDir;
      uniform vec3 uHorizon;
      uniform vec3 uZenith;
      uniform vec3 uSkyTint;
      uniform float uHasSky;
      uniform sampler2D uSky;
      uniform sampler2D uSceneColor;
      uniform sampler2D uSceneDepth;
      uniform vec2 uResolution;
      uniform float uCamNear;
      uniform float uCamFar;
      uniform mat4 uInvViewProj;
      uniform float uChop;
      uniform float uClarity;
      uniform float uSeeThrough;
      uniform float uHueShift;
      uniform float uCausticStr;
      uniform float uCausticSharp;
      uniform float uSparkle;
      uniform float uTintStr;
      uniform float uTintDepth;
      varying vec3 vWorld;
      varying vec2 vXY;
      varying vec3 vT;
      varying vec3 vB;
      varying vec3 vN;
      // 2D simplex noise (Ashima Arts / Stefan Gustavson, MIT -- credited in
      // CREDITS.md). Smooth and grid-free, unlike value noise, so the surface
      // reads as liquid rather than as fuzzy static.
      vec3 mod289(vec3 x) { return x - floor(x * (1.0 / 289.0)) * 289.0; }
      vec2 mod289(vec2 x) { return x - floor(x * (1.0 / 289.0)) * 289.0; }
      vec3 permute(vec3 x) { return mod289(((x * 34.0) + 1.0) * x); }
      float snoise(vec2 v) {
        const vec4 C = vec4(0.211324865405187, 0.366025403784439,
                            -0.577350269189626, 0.024390243902439);
        vec2 i = floor(v + dot(v, C.yy));
        vec2 x0 = v - i + dot(i, C.xx);
        vec2 i1 = (x0.x > x0.y) ? vec2(1.0, 0.0) : vec2(0.0, 1.0);
        vec4 x12 = x0.xyxy + C.xxzz;
        x12.xy -= i1;
        i = mod289(i);
        vec3 p = permute(permute(i.y + vec3(0.0, i1.y, 1.0)) + i.x + vec3(0.0, i1.x, 1.0));
        vec3 m = max(0.5 - vec3(dot(x0, x0), dot(x12.xy, x12.xy), dot(x12.zw, x12.zw)), 0.0);
        m = m * m; m = m * m;
        vec3 x = 2.0 * fract(p * C.www) - 1.0;
        vec3 h = abs(x) - 0.5;
        vec3 ox = floor(x + 0.5);
        vec3 a0 = x - ox;
        m *= 1.79284291400159 - 0.85373472095314 * (a0 * a0 + h * h);
        vec3 g;
        g.x = a0.x * x0.x + h.x * x0.y;
        g.yz = a0.yz * x12.xz + h.yz * x12.yw;
        return 130.0 * dot(m, g);
      }
      // A couple of octaves of simplex, each flowing on its own heading. Two,
      // not three: the third octave is high-frequency and mostly adds the peaks
      // that alias into static -- the coarser sum reads calmer.
      float fbm(vec2 p, float t) {
        float v = 0.0, a = 0.6;
        vec2 flow = vec2(t * 0.25, -t * 0.18);
        for (int i = 0; i < 2; i++) {
          v += a * snoise(p + flow);
          p *= 2.0;
          flow *= 1.6;
          a *= 0.5;
        }
        return v;
      }
      // Wave height: layered simplex, directionally stretched along the wind and
      // domain-warped by a slow low-frequency field, so the chop elongates and
      // flows organically instead of scrolling like wallpaper. Low amplitudes and
      // slow, gentle motion keep the contrast soft -- no jagged spikes.
      float ripple(vec2 p, float t) {
        // Rotate into a wind frame and stretch along it, so ripples elongate
        // rather than form circles.
        vec2 q = mat2(0.92, -0.39, 0.39, 0.92) * p;
        q.x *= 0.55;
        // Warp the coordinates with a slow noise -- fluid distortion, not a scroll.
        vec2 w = vec2(fbm(q * 0.006 + 19.0, t * 0.15), fbm(q * 0.006 + 41.0, t * 0.12));
        q += w * 60.0;
        // A large slow swell, a finer chop, and a fine near-surface detail layer.
        // The detail aliases at distance, but the distance fade in main() smooths
        // it out there, so it only shows on the close water where it is wanted.
        return fbm(q * 0.010, t * 0.4)
             + fbm(q * 0.026, t * 0.7) * 0.35
             + fbm(q * 0.060, t * 1.0) * 0.18;
      }
      vec3 skyColor(vec3 dir) {
        float el = clamp(dir.y * 0.5 + 0.5, 0.0, 1.0);
        vec3 c = (uHasSky > 0.5) ? texture2D(uSky, vec2(0.5, 1.0 - el)).rgb
                                 : mix(uHorizon, uZenith, el);
        return c * uSkyTint;
      }
      // Rotate a colour's hue by an angle (radians) about the grey axis -- a
      // Rodrigues rotation of the RGB vector around (1,1,1), no HSV round-trip.
      vec3 hueShift(vec3 col, float a) {
        const vec3 k = vec3(0.57735026);  // normalized (1,1,1)
        float c = cos(a);
        return col * c + cross(k, col) * sin(a) + k * dot(k, col) * (1.0 - c);
      }
      // Window-space depth (0..1) to a positive view-space distance, for a
      // standard perspective projection -- so two depths can be subtracted to get
      // how much water lies between the surface and the bottom.
      float linearDepth(float d) {
        float z = d * 2.0 - 1.0;
        return (2.0 * uCamNear * uCamFar) / (uCamFar + uCamNear - z * (uCamFar - uCamNear));
      }
      // A rippling caustic pattern, the well-worn summed-distortion form (after
      // the caustic technique in Hrnchamd's underwater shader). Laid on the lit
      // bottom seen through shallow water.
      //
      // Rather than filling the bright cells of the field -- which read as blobs --
      // this traces the *crest* of each ridge as a thin bright filament, the way
      // real caustics fall as a moving net of lines on a seabed. The field rises
      // to a peak along each ridge; a narrow band around a high level of it is the
      // line, and uCausticSharp sets how tight (higher = thinner).
      float caustics(vec2 p, float t) {
        vec2 i = p;
        float c = 1.0;
        float inten = 0.005;
        for (int n = 0; n < 3; n++) {
          float tt = t * (1.0 - (3.5 / float(n + 1)));
          i = p + vec2(cos(tt - i.x) + sin(tt + i.y), sin(tt - i.y) + cos(tt + i.x));
          c += 1.0 / length(vec2(p.x / (sin(i.x + tt) / inten), p.y / (cos(i.y + tt) / inten)));
        }
        c /= 3.0;
        float f = clamp(pow(abs(1.17 - pow(c, 1.4)), 5.0), 0.0, 1.0);
        // The line is the level set f == 0.5: a thin closed contour around each
        // ridge crest, so the net reads as lines, not filled patches. Width is
        // 0.35 at sharp 1, narrowing as sharp climbs.
        float w = 0.35 / max(uCausticSharp, 0.5);
        return 1.0 - smoothstep(0.0, w, abs(f - 0.5));
      }
      void main() {
        float t = uTime;
        // Local surface normal from the ripple gradient, then into world space.
        // The gradient is scaled up so the small ripples still break the
        // reflection into moving highlights -- the sparkle that reads as water.
        float e = 0.9;
        float nstr = uChop;
        float h = ripple(vXY, t);
        float dhdx = (ripple(vXY + vec2(e, 0.0), t) - h) / e * nstr;
        float dhdy = (ripple(vXY + vec2(0.0, e), t) - h) / e * nstr;
        // Fade the ripple detail out with distance. Far off, many ripples fall in
        // one pixel and their flickering normals alias into bright static; letting
        // the surface go smooth with distance is the standard cure -- and it also
        // stops the far water reading brighter than the near water.
        float dist = length(cameraPosition - vWorld);
        float detail = clamp(1.0 - dist / 6000.0, 0.03, 1.0);
        dhdx *= detail;
        dhdy *= detail;
        // World normal from the local ripple gradient, using the tangent basis
        // the vertex stage handed down (the basis is orthonormal, modelMatrix
        // being a pure rotation here, so this is just mat3(modelMatrix)*nLocal).
        vec3 nLocal = vec3(-dhdx, -dhdy, 1.0);
        vec3 N = normalize(nLocal.x * vT + nLocal.y * vB + nLocal.z * vN);
        vec3 V = normalize(cameraPosition - vWorld);

        // How deep the water is here: the bottom's view distance (from the
        // refraction pass's depth) minus this surface fragment's own.
        vec2 uv = gl_FragCoord.xy / uResolution;
        float surfZ = linearDepth(gl_FragCoord.z);
        float sceneZ = linearDepth(texture2D(uSceneDepth, uv).r);
        float waterDepth = max(sceneZ - surfZ, 0.0);

        // Refract the bottom: nudge the lookup by the surface tilt, more in deeper
        // water. If the nudged pixel is actually in front of the water (a shore or
        // an object breaking the surface), keep the straight lookup so the edge
        // does not smear.
        vec2 distort = normalize(nLocal).xy * 0.04 * clamp(waterDepth / 150.0, 0.0, 1.0);
        vec2 refrUV = clamp(uv + distort, vec2(0.001), vec2(0.999));
        float rawDr = texture2D(uSceneDepth, refrUV).r;
        float sceneZr = linearDepth(rawDr);
        if (sceneZr < surfZ) { refrUV = uv; rawDr = texture2D(uSceneDepth, uv).r; sceneZr = sceneZ; }
        float wdepth = max(sceneZr - surfZ, 0.0);
        vec3 refracted = texture2D(uSceneColor, refrUV).rgb;

        // Caustics on the reconstructed world position of the bottom. Banded by
        // depth: they *fade in* over the first stretch of water rather than
        // hitting full strength right at the waterline (which blew out to white
        // splotches over the light sandy shallows), hold through the shallows,
        // then fade out into the deep. Kept soft and dim so they read as light on
        // the seabed, not as noise on the surface.
        vec4 wp = uInvViewProj * vec4(refrUV * 2.0 - 1.0, rawDr * 2.0 - 1.0, 1.0);
        vec3 bottom = wp.xyz / wp.w;
        float sun = clamp(uSunDir.y, 0.0, 1.0);
        float caus = caustics(bottom.xz * 0.02, t * 0.6);
        // Banded by depth, and faded out with camera distance -- far off the fine
        // caustic ridges just alias into speckle, so they belong on the near water.
        float causFar = clamp(1.0 - dist / 3000.0, 0.0, 1.0);
        float causW = clamp(wdepth / 60.0, 0.0, 1.0) * clamp(1.0 - wdepth / 400.0, 0.0, 1.0) * causFar;
        refracted += caus * causW * (0.15 + 0.35 * sun) * uCausticStr * vec3(0.5, 0.7, 0.65);

        // The water body: the bottom shows through the shallows, the deep colour
        // taking over with depth, plus a broad turquoise tint band over the
        // shallows for the coast. uDeep never fully wins.
        //
        // uSeeThrough dials the blue's *opacity* down without touching its hue, so
        // the bottom reads through more the higher it goes. It scales the deep
        // blend toward a floor rather than to zero, so even the clearest setting
        // keeps a blue cast rather than turning the water into plain glass.
        float dfac = 0.85 * clamp(wdepth / uClarity, 0.0, 1.0) * mix(1.0, 0.12, uSeeThrough);
        vec3 body = mix(refracted, uDeep, dfac);
        // A restrained turquoise cast, only over the true shallows -- the broad,
        // strong band was washing the whole surface pale; this keeps the body of
        // the water deep and darker while still greening the shoreline.
        body = mix(body, uShallow, uTintStr * clamp(1.0 - wdepth / uTintDepth, 0.0, 1.0));

        // Reflection of the sky, Fresnel-weighted (near-mirror at grazing angles),
        // plus a bright sun glint -- the sparkle scattered across the surface.
        vec3 sky = skyColor(reflect(-V, N));
        float fres = 0.02 + 0.98 * pow(1.0 - max(dot(N, V), 0.0), 5.0);
        vec3 color = mix(body, sky, fres);
        color += vec3(1.0) * pow(max(dot(N, normalize(uSunDir + V)), 0.0), 180.0) * uSparkle;
        // Darken with distance toward the deep colour -- forced dark, so the far
        // water sinks into shadow rather than lightening off into the distance.
        color = mix(color, uDeep, clamp(dist / 7000.0, 0.0, 1.0) * 0.7);
        if (uHueShift != 0.0) color = max(hueShift(color, uHueShift), 0.0);
        gl_FragColor = vec4(color, 1.0);
      }
    `;
    function makeWaterMaterial() {
      return new THREE.ShaderMaterial({
        uniforms: {
          uTime: {value: 0.0},
          // openmw.cfg Water_UnderwaterColor 12,30,37 -> the deep-water tint.
          uDeep: {value: new THREE.Color(12 / 255, 30 / 255, 37 / 255)},
          uShallow: {value: new THREE.Color(0.13, 0.33, 0.33)},
          uSunDir: {value: sunDirection.clone()},
          uHorizon: {value: new THREE.Color(0.55, 0.70, 0.82)},
          uZenith: {value: new THREE.Color(0.18, 0.36, 0.60)},
          uSkyTint: {value: new THREE.Color(1.0, 1.0, 1.0)},
          uHasSky: {value: skyTex ? 1.0 : 0.0},
          uSky: {value: skyTex},
          uSceneColor: {value: null},
          uSceneDepth: {value: null},
          uResolution: {value: new THREE.Vector2(1.0, 1.0)},
          uCamNear: {value: 0.1},
          uCamFar: {value: 20000.0},
          uInvViewProj: {value: new THREE.Matrix4()},
          // Fine-tunables, exposed in the water panel.
          uWaveScale: {value: 1.0},
          uChop: {value: 8.0},
          uClarity: {value: 1800.0},
          uSeeThrough: {value: 0.0},
          uHueShift: {value: 0.0},
          uCausticStr: {value: 1.0},
          uCausticSharp: {value: 1.5},
          uSparkle: {value: 2.5},
          // Shore-tint band: restrained by default so the water stays deep, not
          // washed pale. Strength and how far out (world units) it reaches.
          uTintStr: {value: 0.14},
          uTintDepth: {value: 500.0}
        },
        vertexShader: WATER_VERT,
        fragmentShader: WATER_FRAG,
        // Opaque: the shader composites the refracted bottom itself, so there is
        // no transparency to sort, and the surface writes depth normally.
        transparent: false,
        side: THREE.DoubleSide
      });
    }

    // The full-screen underwater pass. A plane covering clip space, drawn with no
    // depth test over the rendered frame: it fogs the image toward the murky water
    // colour by distance, lays caustics on the reconstructed seabed, and wobbles
    // the whole thing -- the view when the camera is below the surface.
    var UNDER_VERT = `
      varying vec2 vUv;
      void main() { vUv = uv; gl_Position = vec4(position.xy, 0.0, 1.0); }
    `;
    var UNDER_FRAG = `
      uniform sampler2D uColor;
      uniform sampler2D uDepth;
      uniform vec2 uResolution;
      uniform float uCamNear;
      uniform float uCamFar;
      uniform float uTime;
      uniform vec3 uWaterColor;
      uniform float uFogDensity;
      uniform float uCausticSharp;
      uniform float uHueShift;
      uniform mat4 uInvViewProj;
      uniform float uWaterY;
      varying vec2 vUv;
      vec3 hueShift(vec3 col, float a) {
        const vec3 k = vec3(0.57735026);
        float c = cos(a);
        return col * c + cross(k, col) * sin(a) + k * dot(k, col) * (1.0 - c);
      }
      float linearDepth(float d) {
        float z = d * 2.0 - 1.0;
        return (2.0 * uCamNear * uCamFar) / (uCamFar + uCamNear - z * (uCamFar - uCamNear));
      }
      float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123); }
      float vnoise(vec2 p) {
        vec2 i = floor(p), f = fract(p);
        vec2 u = f * f * (3.0 - 2.0 * f);
        return mix(mix(hash(i + vec2(0.0, 0.0)), hash(i + vec2(1.0, 0.0)), u.x),
                   mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), u.x), u.y);
      }
      // Same caustic net as the surface pass: the crest of each ridge traced as a
      // thin line rather than a filled blob. uCausticSharp sets the line width.
      float caustics(vec2 p, float t) {
        vec2 i = p; float c = 1.0; float inten = 0.005;
        for (int n = 0; n < 3; n++) {
          float tt = t * (1.0 - (3.5 / float(n + 1)));
          i = p + vec2(cos(tt - i.x) + sin(tt + i.y), sin(tt - i.y) + cos(tt + i.x));
          c += 1.0 / length(vec2(p.x / (sin(i.x + tt) / inten), p.y / (cos(i.y + tt) / inten)));
        }
        c /= 3.0;
        float f = clamp(pow(abs(1.17 - pow(c, 1.4)), 5.0), 0.0, 1.0);
        float w = 0.35 / max(uCausticSharp, 0.5);
        return 1.0 - smoothstep(0.0, w, abs(f - 0.5));
      }
      void main() {
        // A slow wobble, so the whole underwater image sways.
        vec2 wob = 0.004 * vec2(vnoise(vUv * 6.0 + uTime * 0.4) - 0.5,
                                vnoise(vUv * 6.0 - uTime * 0.5 + 5.0) - 0.5);
        vec2 uv = clamp(vUv + wob, vec2(0.0), vec2(1.0));
        vec3 col = texture2D(uColor, uv).rgb;
        float d = texture2D(uDepth, uv).r;
        float viewZ = linearDepth(d);
        float fog = clamp(1.0 - exp(-viewZ * uFogDensity), 0.0, 1.0);
        // Caustics on the reconstructed seabed, only on surfaces below the water
        // line and fading out into the murk.
        vec4 wp = uInvViewProj * vec4(uv * 2.0 - 1.0, d * 2.0 - 1.0, 1.0);
        vec3 world = wp.xyz / wp.w;
        float below = clamp((uWaterY - world.y) / 60.0, 0.0, 1.0);
        float caus = caustics(world.xz * 0.02, uTime * 0.6);
        col += caus * below * (1.0 - fog) * vec3(0.35, 0.55, 0.5);
        col = mix(col, uWaterColor, fog);
        if (uHueShift != 0.0) col = max(hueShift(col, uHueShift), 0.0);
        gl_FragColor = vec4(col, 1.0);
      }
    `;
    function ensureUnderwater() {
      if (underMat) return;
      underMat = new THREE.ShaderMaterial({
        uniforms: {
          uColor: {value: null},
          uDepth: {value: null},
          uResolution: {value: new THREE.Vector2(1.0, 1.0)},
          uCamNear: {value: 0.1},
          uCamFar: {value: 20000.0},
          uTime: {value: 0.0},
          // A murky green-teal, close to openmw.cfg Water_UnderwaterColor but
          // lifted so the scene still reads through it.
          uWaterColor: {value: new THREE.Color(0.04, 0.20, 0.20)},
          uFogDensity: {value: underState.fog},
          uCausticSharp: {value: underState.causticSharp},
          uHueShift: {value: underState.hue},
          uInvViewProj: {value: new THREE.Matrix4()},
          uWaterY: {value: 0.0}
        },
        vertexShader: UNDER_VERT,
        fragmentShader: UNDER_FRAG,
        depthTest: false,
        depthWrite: false
      });
      fsScene = new THREE.Scene();
      fsCamera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 1);
      fsScene.add(new THREE.Mesh(new THREE.PlaneGeometry(2, 2), underMat));
    }

    scenes.forEach(function (spec, index) {
      // The inner group carries the Z-up to Y-up rotation; the outer pivot
      // carries the centring. They cannot be the same object: three.js
      // composes T*R*S, so a position set from a centre measured before the
      // rotation leaves the mesh at R*v - centre rather than R*(v - centre).
      var group = new THREE.Group();
      group.userData.shapes = [];
      spec.meshes.forEach(function (m) {
        var g = new THREE.BufferGeometry();
        g.setAttribute("position", new THREE.BufferAttribute(m.positions, 3));
        // A particle cloud: draw the emitter's particle positions as drifting
        // points (mist, glowbugs) instead of a surface. No index, uv or normals.
        if (m.points) {
          var pmat = new THREE.PointsMaterial({
            size: 14, sizeAttenuation: true, transparent: true, opacity: 0.5,
            depthWrite: false, blending: THREE.AdditiveBlending, color: 0xdfe8f0
          });
          var ptex = (m.image && textured) ? makeSlotTexture(m.image, true) : null;
          if (ptex) { pmat.map = ptex; }
          var pNodeWorld = m.nodeWorld ? new THREE.Matrix4().fromArray(m.nodeWorld) : null;
          var pCount = (m.instances && m.instanceCount) ? m.instanceCount : 1;
          for (var pi = 0; pi < pCount; pi++) {
            var pts = new THREE.Points(g, pmat);
            var pm = new THREE.Matrix4();
            if (m.instances) pm.fromArray(m.instances, pi * 16);
            if (pNodeWorld) pm.multiply(pNodeWorld);
            pts.applyMatrix4(pm);
            pts.frustumCulled = false;
            pts.name = m.name || "particles";
            pts.userData.refIds = m.refIds || null;
            pts.userData.recordType = m.recordType || "Emitter";
            pts.userData.collision = !!m.collision;
            pts.userData.focus = !!m.focus;
            pts.userData.adjacent = !!m.adjacent;
            if (m.recordType) typesInScene[m.recordType] = true;
            if (m.collision) pts.visible = false;
            group.userData.shapes.push({object: pts, spec: m});
          }
          // Drift is on the shared geometry, so register it once (not per copy).
          pointsAnimated.push({attr: g.getAttribute("position"), base: m.positions.slice()});
          return;
        }
        g.setIndex(new THREE.BufferAttribute(m.indices, 1));
        if (m.uvs) g.setAttribute("uv", new THREE.BufferAttribute(m.uvs, 2));
        // Vertex colors are three floats each, already 0-1 in the file. They
        // are only attached when the count matches, which geometry.py has
        // already enforced -- a short set makes three.js index past the end of
        // the attribute and draw nothing at all.
        // itemSize is 3 for ordinary vertex colours, 4 for a terrain blend
        // layer whose fourth channel is its coverage alpha.
        if (m.colors) g.setAttribute("color", new THREE.BufferAttribute(m.colors, m.colorItems || 3));
        // The files carry normals, but not always, and a mesh with none
        // renders flat black. Computing them is cheap and always right.
        g.computeVertexNormals();
        // Water is its own thing: a reflective shader surface, not a lit mesh.
        // Handle it here and skip the whole texture/material pipeline below --
        // none of which (base map, alpha toggles, normal maps) applies to it.
        if (m.water) {
          var waterMat = makeWaterMaterial();
          var drawnW = new THREE.Mesh(g, waterShaderOn ? waterMat : getSimpleWaterMaterial());
          drawnW.userData.shaderMat = waterMat;  // restored when the shader is toggled on
          waterMaterials.push(waterMat);
          waterMeshes.push(drawnW);
          // The water plane's extent is the cell, so origin-centred frustum
          // culling would wrongly drop it.
          drawnW.frustumCulled = false;
          // Rendered after the terrain's transparent blend layers (whose order is
          // small) so it draws over ground that sits below it, while still failing
          // the depth test against ground that pokes above the surface.
          drawnW.renderOrder = 1000;
          drawnW.name = m.name || "water";
          drawnW.userData.refIds = m.refIds || null;
          drawnW.userData.recordType = m.recordType || "Water";
          if (m.recordType) typesInScene[m.recordType] = true;
          drawnW.userData.collision = false;
          drawnW.userData.focus = !!m.focus;
          drawnW.userData.adjacent = !!m.adjacent;
          drawnW.userData.water = true;
          if (m.adjacent) drawnW.visible = false;
          group.userData.shapes.push({object: drawnW, spec: m});
          group.add(drawnW);
          return;
        }
        var material = new THREE.MeshPhongMaterial({
          color: spec.color, side: THREE.DoubleSide
        });
        // What the file itself says about this shape's material. Held on the
        // mesh rather than applied blindly, because every one of these is
        // something a control below can override -- and a control that
        // overrides a *known* value is far more useful than one that
        // overrides a guess, which is what these were before the reader
        // carried them.
        var fromFile = {
          diffuse: m.diffuse ? new THREE.Color(m.diffuse[0], m.diffuse[1], m.diffuse[2]) : null,
          emissive: m.emissive
            ? new THREE.Color(m.emissive[0], m.emissive[1], m.emissive[2]) : null,
          opacity: typeof m.opacity === "number" ? m.opacity : 1.0,
          blend: !!m.alphaBlend,
          test: !!m.alphaTest,
          // A threshold of zero with testing on discards nothing, which is
          // indistinguishable from testing being off. Morrowind's own default
          // reference is what the file stores; fall back only when it is
          // absent entirely.
          threshold: typeof m.alphaThreshold === "number" ? m.alphaThreshold : 0.5
        };
        material.userData.fromFile = fromFile;
        // Opaque faces one-sided when asked (a cell): near walls cull away so
        // you see in. Alpha shapes (foliage, banners, glass) stay two-sided --
        // those really are drawn from both sides in Morrowind.
        if (singleSided && !fromFile.blend && !fromFile.test) {
          material.side = THREE.FrontSide;
        }
        if (m.colors) material.vertexColors = true;
        // Emissive is the material's own glow colour. When the shape carries a
        // glow *map* it is set to white in the glow block below, so the map carries
        // the colour and restricts the glow to the lit parts. Without a glow map a
        // bright emissive on a *textured* surface is almost always a window/glass
        // material whose (missing here) glow map was meant to mask it -- applying
        // it full blows the whole face white (the "white windows" bug). So a lone
        // emissive is honoured only on an untextured shape with no glow slot at
        // all, where it is a deliberate flat self-illumination (a glow effect).
        if (fromFile.emissive && !m.glow && !m.image) material.emissive = fromFile.emissive;
        if (fromFile.blend) { material.transparent = true; material.opacity = fromFile.opacity; }
        if (fromFile.test) material.alphaTest = fromFile.threshold;
        if (m.image && m.uvs && textured) {
          // makeSlotTexture returns the texture synchronously (an image-backed
          // one, or a compressed passthrough); its pixels arrive later and mark
          // it dirty. A null means a compressed format the GPU here cannot take,
          // so the shape simply draws untextured rather than wrong.
          var tex = makeSlotTexture(m.image, true);
          if (tex) {
            material.map = tex;
            // Tinting a texture with the side color would make the two
            // providers look different for a reason that is not in the file.
            material.color = new THREE.Color(0xffffff);
          }
        }
        // An OpenMW-style normal map, found beside the diffuse one by name.
        // Not color: it is loaded in linear space, because treating a field
        // of vectors as sRGB bends every one of them.
        var extras = m.extras || {};
        var normalSource = extras["_nh"] || extras["_n"] || null;
        var normalTex = null;
        if (normalSource && m.uvs && textured) {
          normalTex = makeSlotTexture(normalSource, false);
        }
        // The bump slot, read as tangent-space normals -- the MGE-XE/NifSkope
        // convention, not vanilla's (which ignores the slot outright) and not
        // necessarily OpenMW's. Loaded regardless of which convention is
        // in force; the "Bump as normal (MGE)" checkbox decides whether it
        // is ever attached to a material, and prefers an OpenMW-style
        // sibling when a mesh happens to carry both rather than fight over
        // which wins.
        var bumpTex = null;
        if (m.bump && m.uvs && textured) {
          bumpTex = makeSlotTexture(m.bump, false);
        }
        // Same OpenMW naming convention, for a specular map. Also linear:
        // it modulates highlight strength, not a color to be seen directly.
        var specSource = extras["_spec"] || extras["_diffusespec"] || null;
        var specTex = null;
        if (specSource && m.uvs && textured) {
          specTex = makeSlotTexture(specSource, false);
          // MeshPhongMaterial's default specular color is a dim 0x111111,
          // dim enough that a specular map barely shows against it. The
          // brighter value belongs *with* the map and is applied by the
          // control, not here.
          //
          // Setting it at construction was the earlier approach and made the
          // control read backwards: with the box unchecked the shape still
          // got the bright highlight, unmodulated across its whole surface,
          // which looks like specular is on. Ticking the box then attached
          // the map and *darkened* it wherever the map was dark -- so "on"
          // looked duller than "off". The logic was right and the appearance
          // was inverted, which is the harder kind to spot.
        }
        // The glow slot, unlike the two above, is not a filename guess -- the
        // shape names it directly, and it is what Morrowind's own renderer
        // (not just OpenMW) uses for self-illumination: lit windows, lava,
        // glowing eyes. sRGB, like the diffuse map: it is a color being
        // added to the surface, not a vector or a scalar mask.
        var glowTex = null;
        if (m.glow && m.uvs && textured) {
          glowTex = makeSlotTexture(m.glow, true);
          if (glowTex) {
            // Bind the glow map to the material *now*, not only when the toggle is
            // clicked. Without this, a self-illuminated object emits its whole
            // surface at the material's emissive colour (flat white) until the
            // toggle finally attaches the map that was meant to restrict the glow
            // to the lit parts. Emissive is set white so the map carries the glow
            // colour; the map arrives async and re-uploads onto this bound slot.
            material.emissiveMap = glowTex;
            material.emissive = new THREE.Color(0xffffff);
          }
        }
        // The dark and detail slots both multiply into the base color --
        // Morrowind applies detail first, then dark -- and gloss modulates
        // specular strength by a mask rather than supplying a specular
        // color. None of the three is a color meant to be seen on its own,
        // so none is sRGB, the same reasoning as the normal and specular
        // maps above. A decal is different again: a layer stamped on top,
        // meant to be seen, so it stays sRGB like the base texture.
        //
        // MeshPhongMaterial has exactly one slot shaped like "multiply the
        // surface by a texture" (aoMap) and exactly one shaped like
        // "modulate specular by a texture" (specularMap) -- and specularMap
        // is already spoken for by an OpenMW-style _spec sibling. Cramming
        // dark and detail into the one multiply slot would mean whichever
        // assigned second silently overwrote the first. Rather than pick a
        // loser, all four slots below are drawn through a shared
        // onBeforeCompile hook (attachExtraSlots, defined once outside this
        // loop) that injects them straight into Phong's own fragment shader
        // -- real per-mesh layers, not a shared property fighting over who
        // gets to hold it.
        var detailTex = null;
        if (m.detail && m.uvs && textured) {
          detailTex = makeSlotTexture(m.detail, false);
        }
        var darkTex = null;
        if (m.dark && m.uvs && textured) {
          darkTex = makeSlotTexture(m.dark, false);
        }
        var glossTex = null;
        if (m.gloss && m.uvs && textured) {
          glossTex = makeSlotTexture(m.gloss, false);
        }
        // Every decal the shape declares, in slot order. Kept as an array
        // rather than one texture because slot order *is* paint order: they
        // composite over one another, and the last declared is the one on top.
        var decalTexes = [];
        if (m.decals && m.decals.length && m.uvs && textured) {
          m.decals.forEach(function (slot) {
            var tex = makeSlotTexture(slot, true);
            if (tex) decalTexes.push(tex);
          });
        }
        var drawn;
        // The shape's node transform, folded into each instance matrix rather
        // than into the vertices (the vertices are the shape's own local space
        // now -- UNBAKE_MIGRATION.md). Identity for a baked mesh (standalone
        // viewer), where it changes nothing.
        var nodeWorldMat = m.nodeWorld ? new THREE.Matrix4().fromArray(m.nodeWorld) : null;
        if (m.instances && m.instanceCount) {
          // One InstancedMesh for the whole group: the model drawn once, placed
          // by a matrix per instance. isMesh stays true, so every control and
          // toggle below treats it exactly like a plain mesh.
          drawn = new THREE.InstancedMesh(g, material, m.instanceCount);
          var _im = new THREE.Matrix4(), _iw = new THREE.Matrix4();
          for (var _k = 0; _k < m.instanceCount; _k++) {
            _im.fromArray(m.instances, _k * 16);
            // instance = placement * nodeWorld: folds the shape's node transform
            // in here instead of into its vertices.
            if (nodeWorldMat) _iw.multiplyMatrices(_im, nodeWorldMat); else _iw.copy(_im);
            drawn.setMatrixAt(_k, _iw);
          }
          drawn.instanceMatrix.needsUpdate = true;
          // The base placements and the node transform, kept so a node animation
          // can rebuild each instance's matrix as placement * delta(t) * nodeWorld.
          drawn.instanceBase = m.instances;
          drawn.nodeWorldMat = nodeWorldMat;
          // Frustum culling uses the geometry's origin-centred bounds, which is
          // wrong once instances are scattered across a cell -- it would cull
          // objects that are plainly on screen. The cell is bounded, so drop it.
          drawn.frustumCulled = false;
        } else {
          drawn = new THREE.Mesh(g, material);
        }
        drawn.userData.map = material.map || null;
        // A scrolling/scaling texture animation from the file: the material's
        // diffuse map must wrap (so an offset past the edge repeats) and joins
        // the clock-driven list the animation loop advances.
        if (m.uvAnim && material.map) {
          material.map.wrapS = material.map.wrapT = THREE.RepeatWrapping;
          material.map.needsUpdate = true;
          uvAnimated.push({tex: material.map, anim: m.uvAnim});
        }
        // A node keyframe animation (sway/spin/slide) plays as a per-frame
        // matrix delta -- on the lone mesh's matrix, or on each instance's.
        if (m.transformAnim) registerXformAnim(drawn, m.transformAnim);
        // A visibility animation blinks the whole shape on and off.
        if (m.visAnim) visAnimated.push({obj: drawn, keys: m.visAnim});
        // A vertex-morph animation ripples the cloth: keep the base pose and
        // rewrite the shared position attribute each frame. Deltas must match the
        // vertex count; a mismatch (a dirty mesh) simply does not register.
        if (m.morphAnim && m.positions && drawn.geometry) {
          var posAttr = drawn.geometry.getAttribute("position");
          var ok = posAttr && m.morphAnim.targets.every(function (tt) {
            return tt.deltas.length === posAttr.array.length;
          });
          if (ok) {
            morphAnimated.push({
              posAttr: posAttr,
              base: m.positions.slice(),  // pristine copy of the un-morphed pose
              targets: m.morphAnim.targets
            });
          }
        }
        drawn.userData.normalMap = normalTex;
        drawn.userData.bumpMap = bumpTex;
        drawn.userData.specularMap = specTex;
        drawn.userData.glowMap = glowTex;
        drawn.userData.darkMap = darkTex;
        drawn.userData.detailMap = detailTex;
        drawn.userData.glossMap = glossTex;
        drawn.userData.decalMaps = decalTexes;
        drawn.name = m.name || "";  // the shape name, a fallback when clicked
        drawn.userData.refIds = m.refIds || null;  // object id per instance
        drawn.userData.texturePath = m.texture || "";  // winning diffuse, for the ori readout
        drawn.userData.recordType = m.recordType || "";  // Static, Light, ... for toggles
        if (m.recordType) typesInScene[m.recordType] = true;
        drawn.userData.tint = spec.color;
        drawn.userData.collision = !!m.collision;
        drawn.userData.focus = !!m.focus;
        drawn.userData.adjacent = !!m.adjacent;  // a neighbouring cell's geometry
        drawn.userData.skirtAlone = !!m.skirtAlone;  // lone-cell skirt: hide when neighbours show
        // Terrain blend layers: the base (layer 0) is opaque; every higher layer
        // is faded in over it by its per-vertex coverage alpha, so it must be
        // transparent, must not write depth (or it would occlude the layers
        // beneath it at the same height), and must render in paint order.
        if (m.blendLayer !== undefined && m.blendLayer >= 0) {
          drawn.renderOrder = m.blendLayer;
          if (m.blendLayer > 0) { material.transparent = true; material.depthWrite = false; }
        }
        // Collision hulls are never drawn in game; start them hidden so the view
        // shows what the player sees, and let the toggle reveal them on demand.
        // Adjacent-cell geometry, by contrast, starts *visible*: the user asked
        // for it in the picker, so show it, with a toggle to hide it again.
        if (m.collision) drawn.visible = false;
        attachExtraSlots(material, drawn, wantsSlot);
        if (normalTex) anyNormalMaps = true;
        if (bumpTex) anyBumpMaps = true;
        if (specTex) anySpecularMaps = true;
        if (glowTex) anyGlowMaps = true;
        if (darkTex) anyDarkMaps = true;
        if (detailTex) anyDetailMaps = true;
        if (glossTex) anyGlossMaps = true;
        if (decalTexes.length) anyDecalMaps = true;
        if (m.collision) anyCollision = true;
        if (m.adjacent) anyAdjacent = true;
        // Kept alongside the object so the shape list below can reach both:
        // the three.js mesh to hide, and the payload to describe. Matching
        // them up later by name would be guesswork -- shape names repeat
        // freely within one file, and several vanilla meshes have none.
        group.userData.shapes.push({object: drawn, spec: m});
        group.add(drawn);
      });
      group.rotation.x = -Math.PI / 2;
      group.visible = index === 0;
      pivot.add(group);
      groups.push(group);
    });

    pivot.updateMatrixWorld(true);
    // Framed over *every* provider, visible or not, so toggling never moves
    // the camera. A comparison where the view shifts as you switch is a
    // comparison of two different pictures.
    var box = new THREE.Box3();
    var _ibox = new THREE.Box3();
    var _imat = new THREE.Matrix4();
    function finiteBox(b) {
      return isFinite(b.min.x) && isFinite(b.min.y) && isFinite(b.min.z) &&
             isFinite(b.max.x) && isFinite(b.max.y) && isFinite(b.max.z);
    }
    // When any object is marked focus (an exterior's terrain, which is exactly
    // the cell), frame on those alone: a reference far outside the cell -- a
    // moved one, or a bad coordinate -- must not drag the camera off the cell.
    var anyFocus = false;
    groups.forEach(function (g) {
      g.traverse(function (o) { if (o.userData && o.userData.focus) anyFocus = true; });
    });
    function contributes(o) { return !anyFocus || (o.userData && o.userData.focus); }
    groups.forEach(function (g) {
      g.updateMatrixWorld(true);
      g.traverse(function (o) {
        if (!contributes(o)) return;
        if (o.isInstancedMesh) {
          // expandByObject ignores per-instance matrices (r128), so the bounds
          // would be one model at the origin. Union each instance's transformed
          // box instead, or the camera frames empty space. A single non-finite
          // transform (a broken reference) is skipped rather than poisoning the
          // whole box into NaN, which would blank the view.
          if (!o.geometry.boundingBox) o.geometry.computeBoundingBox();
          for (var k = 0; k < o.count; k++) {
            o.getMatrixAt(k, _imat);
            _ibox.copy(o.geometry.boundingBox).applyMatrix4(_imat).applyMatrix4(o.matrixWorld);
            if (finiteBox(_ibox)) box.union(_ibox);
          }
        } else if (o.isMesh) {
          box.expandByObject(o);
        }
      });
    });
    var centre = box.getCenter(new THREE.Vector3());
    var radius = Math.max(box.getSize(new THREE.Vector3()).length() / 2, 1e-3);
    if (!isFinite(radius) || box.isEmpty()) { centre.set(0, 0, 0); radius = 1; }
    pivot.position.copy(centre).negate();
    pivot.updateMatrixWorld(true);
    // Near and far track the scene size: a cell is thousands of units across, so
    // the fixed 0.1/200000 pair either clipped a large exterior beyond the far
    // plane or wasted all depth precision. Derived from the framed radius so both
    // an item and a whole cell sit comfortably inside the frustum.
    camera.near = Math.max(radius * 0.002, 0.05);
    camera.far = Math.max(radius * 400, 20000);
    camera.updateProjectionMatrix();

    // The sky sits behind an *exterior* (an interior has its own walls and a dark
    // void reads fine there). Sized to enclose the framed cell, well inside the
    // far plane. The initial time-of-day tints it and the water together.
    if (!singleSided) {
      skyMesh = makeSky(camera.far * 0.45);
      scene.add(skyMesh);
      starMesh = makeStars(camera.far * 0.44);
      if (starMesh) scene.add(starMesh);
    }
    applyTimeOfDay(timeOfDay);

    // Lighting is not decoration here. A normal map changes nothing at all
    // under flat ambient light -- the whole point of one is how it catches a
    // light that moves -- so these are the controls that make a normal map
    // comparison possible.
    var ambient = new THREE.AmbientLight(0xffffff, 0.55);
    scene.add(ambient);
    var key = new THREE.DirectionalLight(0xffffff, 1.6);
    key.position.set(1, 1.4, 1);
    scene.add(key);
    var fill = new THREE.DirectionalLight(0xffffff, 0.5);
    fill.position.set(-1, -0.6, -0.8);
    scene.add(fill);

    var lightState = {ambient: 0.55, key: 1.6, angle: 0.0, headlamp: false};

    function placeLights() {
      // Lighting changed, so the water's cached refraction of the lit scene is
      // stale (harmless when this runs from the animation loop, where the camera
      // has usually already marked it dirty anyway).
      viewDirty = true;
      ambient.intensity = lightState.ambient;
      key.intensity = lightState.key;
      fill.intensity = lightState.key * 0.31;
      if (lightState.headlamp) {
        // From the camera's own position, so a surface is always lit from
        // wherever you are looking. Useful for reading a normal map's detail,
        // and deliberately *not* the default: it means moving the camera
        // changes the lighting, so two providers can never be compared under
        // identical light while it is on.
        key.position.copy(camera.position);
        fill.position.copy(camera.position).negate();
        return;
      }
      var a = lightState.angle;
      key.position.set(Math.sin(a), 1.4, Math.cos(a));
      fill.position.set(-Math.sin(a), -0.6, -Math.cos(a));
    }

    var treeBox = document.getElementById("tree");

    function renderTree(nodes) {
      var ul = document.createElement("ul");
      nodes.forEach(function (node) {
        var li = document.createElement("li");
        li.setAttribute("data-block-index", node.index);
        var index = document.createElement("span");
        index.className = "ix"; index.textContent = node.index;
        var type = document.createElement("span");
        type.className = "ty"; type.textContent = node.type;
        li.appendChild(index); li.appendChild(type);
        if (node.name) {
          var nm = document.createElement("span");
          nm.className = "nm"; nm.textContent = " " + node.name;
          li.appendChild(nm);
        }
        if (node.note) {
          var no = document.createElement("span");
          no.className = "no"; no.textContent = "  " + node.note;
          li.appendChild(no);
        }
        if (node.children && node.children.length) li.appendChild(renderTree(node.children));
        ul.appendChild(li);
      });
      return ul;
    }

    // What the *file* says this shape is made of, as a sentence rather than
    // as controls. A mesh routinely has twenty shapes, and a checkbox per
    // material property per shape would be a hundred controls answering a
    // question nobody asks -- while "which shape wants a cutout" is answered
    // perfectly well by reading it.
    function summaryOf(m) {
      var parts = [];
      parts.push(m.triangleCount + " tri");
      if (m.collision) parts.push("collision");
      if (!m.uvs) parts.push("no UVs");
      if (m.colors) parts.push("vertex colors");
      if (m.alphaBlend) parts.push("blend @ " + (m.opacity !== undefined
        ? Math.round(m.opacity * 100) + "%" : "?"));
      // A cutout's reference matters -- 0 discards nothing, which is the same
      // as the flag being off -- so it is shown rather than just named.
      if (m.alphaTest) parts.push("cutout @ " + Math.round((m.alphaThreshold || 0) * 255));
      var maps = [];
      if (m.image) maps.push("base");
      if (m.glow) maps.push("glow");
      if (m.dark) maps.push("dark");
      if (m.detail) maps.push("detail");
      if (m.gloss) maps.push("gloss");
      if (m.decals && m.decals.length) {
        maps.push(m.decals.length > 1 ? "decal x" + m.decals.length : "decal");
      }
      if (m.bump) maps.push("bump");
      if (maps.length) parts.push(maps.join("+"));
      return parts.join(", ");
    }

    // Per-shape visibility. Isolating one shape is the control that earns its
    // place: when two providers' meshes differ it is almost always in one
    // sub-shape, and with everything drawn at once you can see *that*
    // something moved without seeing *what*.
    function renderShapes(group, spec) {
      var ul = document.createElement("ul");
      ul.className = "shapes";
      group.userData.shapes.forEach(function (entry, i) {
        var li = document.createElement("li");
        var box = document.createElement("input");
        box.type = "checkbox";
        box.checked = entry.object.visible;
        box.addEventListener("change", function () {
          entry.object.visible = box.checked;
          draw();
        });
        var name = document.createElement("span");
        name.className = "nm shapename";
        name.textContent = entry.spec.name || "(unnamed " + i + ")";
        name.title = "click to isolate; click again to restore";
        // Solo, and solo again to restore. A dedicated "show all" button
        // would be a second control for something the first one can say.
        name.addEventListener("click", function () {
          var soloed = group.userData.shapes.every(function (o) {
            return o === entry ? o.object.visible : !o.object.visible;
          });
          group.userData.shapes.forEach(function (o) {
            o.object.visible = soloed || o === entry;
          });
          refreshTree();
          draw();
        });
        var note = document.createElement("span");
        note.className = "no";
        note.textContent = "  " + summaryOf(entry.spec);
        li.appendChild(box); li.appendChild(name); li.appendChild(note);
        ul.appendChild(li);
      });
      return ul;
    }

    // The cell's audit numbers (references, overrides, missing meshes, ...) --
    // what the previous shape list, built for a single NIF, could never show.
    function renderCellInfo() {
      var head = document.createElement("h3");
      head.className = "cellhead";
      head.textContent = cellInfo.label || "Cell";
      treeBox.appendChild(head);
      var stats = cellInfo.stats || [];
      if (stats.length) {
        var tbl = document.createElement("table");
        tbl.className = "audit";
        stats.forEach(function (row) {
          var tr = document.createElement("tr");
          var td1 = document.createElement("td"); td1.textContent = row[0];
          var td2 = document.createElement("td"); td2.className = "n"; td2.textContent = row[1];
          tr.appendChild(td1); tr.appendChild(td2); tbl.appendChild(tr);
        });
        treeBox.appendChild(tbl);
      }
      var missing = cellInfo.missing || [];
      if (missing.length) {
        var mh = document.createElement("h4");
        mh.textContent = "missing meshes (" + missing.length + ")";
        mh.style.opacity = ".7";
        treeBox.appendChild(mh);
        var ul = document.createElement("ul");
        ul.className = "shapes";
        missing.slice(0, 40).forEach(function (name) {
          var li = document.createElement("li");
          li.className = "no";
          li.textContent = name;
          ul.appendChild(li);
        });
        treeBox.appendChild(ul);
      }
    }
    // The "ori" readout for the clicked object: its id and type, winning model
    // and texture, and which plugins define and place it (last in each list is
    // the one that wins). Shown above the cell audit; cleared on an empty click.
    function renderOri() {
      if (!selected) return;
      var info = objectInfo[(selected.id || "").toLowerCase()] || {};
      var box = document.createElement("div");
      box.className = "orisel";
      var head = document.createElement("h3");
      head.className = "cellhead";
      head.textContent = "Selected object";
      box.appendChild(head);
      var id = document.createElement("div");
      id.className = "oriid";
      id.textContent = selected.id + (selected.type ? "  [" + selected.type + "]" : "");
      box.appendChild(id);
      function kv(k, v) {
        if (!v) return;
        var row = document.createElement("div");
        row.className = "orikv";
        var key = document.createElement("span");
        key.className = "orik";
        key.textContent = k + ": ";
        row.appendChild(key);
        row.appendChild(document.createTextNode(v));
        box.appendChild(row);
      }
      kv("model", info.model || "");
      kv("texture", selected.texture || "");
      function plugins(title, list) {
        if (!list || !list.length) return;
        var h4 = document.createElement("h4");
        h4.textContent = title + " (" + list.length + ")";
        box.appendChild(h4);
        var ul = document.createElement("ul");
        ul.className = "shapes";
        list.forEach(function (name, i) {
          var li = document.createElement("li");
          li.textContent = name;
          // The last plugin in load order is the one whose version wins.
          if (i === list.length - 1) li.className = "oriwin";
          ul.appendChild(li);
        });
        box.appendChild(ul);
      }
      plugins("defined by", info.definedBy);
      plugins("placed here by", info.placedBy);
      treeBox.appendChild(box);
    }
    function refreshTree() {
      treeBox.textContent = "";
      if (cellMode) { renderOri(); renderCellInfo(); }
      var any = false;
      scenes.forEach(function (spec, i) {
        if (!groups[i].visible) return;
        var shapes = groups[i].userData.shapes || [];
        if (!shapes.length && (!spec.tree || !spec.tree.length)) return;
        any = true;
        if (shapes.length && cellMode) {
          // A cell has thousands of shapes; tuck the list in a collapsed accordion
          // and only build its rows when it is actually opened, so refreshing the
          // panel on every toggle stays cheap.
          var det = document.createElement("details");
          det.className = "acc";
          var sum = document.createElement("summary");
          sum.textContent = "shapes (" + shapes.length + ")";
          det.appendChild(sum);
          (function (group, spc, node) {
            var built = false;
            node.addEventListener("toggle", function () {
              if (node.open && !built) { node.appendChild(renderShapes(group, spc)); built = true; }
            });
          })(groups[i], spec, det);
          treeBox.appendChild(det);
        } else if (shapes.length) {
          var heading = document.createElement("h4");
          heading.textContent = spec.label;
          heading.style.color = spec.color;
          treeBox.appendChild(heading);
          var sub = document.createElement("h4");
          sub.textContent = "shapes (" + shapes.length + ")";
          sub.style.opacity = ".7";
          treeBox.appendChild(sub);
          treeBox.appendChild(renderShapes(groups[i], spec));
        }
        if (spec.tree && spec.tree.length && !cellMode) {
          var blocks = document.createElement("h4");
          blocks.textContent = "blocks";
          blocks.style.opacity = ".7";
          treeBox.appendChild(blocks);
          treeBox.appendChild(renderTree(spec.tree));
        }
      });
      if (!any && !cellMode) {
        var note = document.createElement("div");
        note.className = "no";
        note.textContent = "no structure to show";
        treeBox.appendChild(note);
      }
    }

    var statsBox = document.getElementById("stats");
    function refresh() {
      var shapes = 0, tris = 0, verts = 0, shown = 0;
      scenes.forEach(function (spec, i) {
        if (!groups[i].visible) return;
        shown++;
        spec.meshes.forEach(function (m) {
          shapes++; tris += m.triangleCount; verts += m.vertexCount;
        });
      });
      statsBox.textContent = shown
        ? shapes + " shape(s), " + tris + " triangles, " + verts + " vertices"
        : "nothing shown";
      Array.prototype.forEach.call(controls.querySelectorAll(".ctl"), function (el, i) {
        if (i < groups.length) el.className = "ctl" + (groups[i].visible ? "" : " off");
      });
      refreshTree();
      draw();
    }

    scenes.forEach(function (spec, index) {
      var id = "side" + index;
      var span = document.createElement("span");
      span.className = "ctl" + (index === 0 ? "" : " off");
      var box2 = document.createElement("input");
      box2.type = "checkbox"; box2.id = id; box2.checked = index === 0;
      var label = document.createElement("label");
      label.htmlFor = id;
      var swatch = document.createElement("span");
      swatch.className = "swatch"; swatch.style.background = spec.color;
      label.appendChild(swatch);
      label.appendChild(document.createTextNode(" " + spec.label));
      span.appendChild(box2); span.appendChild(label);
      controls.appendChild(span);
      box2.addEventListener("change", function () {
        groups[index].visible = box2.checked;
        refresh();
      });
    });

    var textureBox = document.createElement("input");
    textureBox.type = "checkbox"; textureBox.id = "textured"; textureBox.checked = true;
    var textureCtl = document.createElement("span");
    textureCtl.className = "ctl";
    var textureLabel = document.createElement("label");
    textureLabel.htmlFor = "textured";
    textureLabel.textContent = "Textures";
    textureCtl.appendChild(textureBox); textureCtl.appendChild(textureLabel);
    controls.appendChild(textureCtl);
    textureBox.addEventListener("change", function () {
      // Flat color is often the better comparison: two versions of a mesh
      // wearing the same texture differ in shape, and the texture hides it.
      scene.traverse(function (o) {
        if (!o.isMesh || !o.userData.map) return;
        o.material.map = textureBox.checked ? o.userData.map : null;
        o.material.color = new THREE.Color(textureBox.checked ? 0xffffff : o.userData.tint);
        o.material.needsUpdate = true;
      });
      textureCtl.className = "ctl" + (textureBox.checked ? "" : " off");
      viewDirty = true;  // the refracted scene's textures changed
      draw();
    });

    var alphaBox = document.createElement("input");
    alphaBox.type = "checkbox"; alphaBox.id = "alphacut";
    var alphaCtl = document.createElement("span");
    alphaCtl.className = "ctl off";
    var alphaLabel = document.createElement("label");
    alphaLabel.htmlFor = "alphacut";
    alphaLabel.textContent = "Alpha cutout";
    alphaCtl.appendChild(alphaBox); alphaCtl.appendChild(alphaLabel);
    controls.appendChild(alphaCtl);
    alphaBox.addEventListener("change", function () {
      // A cutout texture (grass, a fence, a leaf) has a quad behind it that
      // otherwise renders fully opaque wherever the mesh exists, regardless
      // of what the alpha channel says -- Phong's default ignores it
      // entirely. alphaTest discards those fragments outright rather than
      // blending them, which is what a cutout wants: blending needs the
      // triangles sorted back-to-front to look right, and two overlapping
      // cutout quads sorted wrong show through each other. A hard cutoff has
      // no ordering to get wrong.
      //
      // Checked forces a cutout on every shape; unchecked returns each to
      // whatever its own NiAlphaProperty asked for, at that shape's own
      // reference value. Before the reader carried those, "unchecked" meant
      // "off everywhere" -- which silently overrode files that had asked for
      // a cutout and got none.
      scene.traverse(function (o) {
        if (!o.isMesh || !o.userData.map) return;
        var own = o.material.userData.fromFile;
        o.material.alphaTest = alphaBox.checked
          ? 0.5
          : (own && own.test ? own.threshold : 0);
        o.material.needsUpdate = true;
      });
      alphaCtl.className = "ctl" + (alphaBox.checked ? "" : " off");
      draw();
    });

    var blendBox = document.createElement("input");
    blendBox.type = "checkbox"; blendBox.id = "alphablend";
    var blendCtl = document.createElement("span");
    blendCtl.className = "ctl off";
    var blendLabel = document.createElement("label");
    blendLabel.htmlFor = "alphablend";
    blendLabel.textContent = "Alpha blend";
    blendCtl.appendChild(blendBox); blendCtl.appendChild(blendLabel);
    controls.appendChild(blendCtl);
    blendBox.addEventListener("change", function () {
      // Alpha cutout is a mask, either in or out -- right for grass and
      // fences, wrong for genuine translucency like glass, water or a ghost,
      // where alpha sits meaningfully between 0 and 1 and a 0.5 cutoff would
      // just round it to fully-opaque-or-fully-gone. This blends it properly
      // instead. The trade this time is the one alphaTest was chosen to
      // avoid above: three.js sorts whole objects back-to-front by distance
      // to the camera, but never the triangles within a single one, so a
      // mesh with self-overlapping transparent geometry -- a complex glass
      // shape folded back on itself -- can still show the wrong surface on
      // top. Independent of the cutout checkbox; a mesh can want both at
      // once (a cutout leaf with softened edges), so neither toggle turns
      // the other off.
      //
      // As with the cutout: checked forces blending everywhere, unchecked
      // returns each shape to what its own file asked for -- including the
      // material's own alpha value, which is *how* transparent as distinct
      // from *whether* it blends at all.
      scene.traverse(function (o) {
        if (!o.isMesh || !o.userData.map) return;
        var own = o.material.userData.fromFile;
        var blend = blendBox.checked || !!(own && own.blend);
        o.material.transparent = blend;
        o.material.opacity = blend && own ? own.opacity : 1.0;
        o.material.needsUpdate = true;
      });
      blendCtl.className = "ctl" + (blendBox.checked ? "" : " off");
      draw();
    });

    function addSlider(id, label, min, max, value, step, onInput) {
      var ctl = document.createElement("span");
      ctl.className = "ctl";
      var text = document.createElement("label");
      text.htmlFor = id; text.textContent = label;
      var range = document.createElement("input");
      range.type = "range"; range.id = id;
      range.min = min; range.max = max; range.step = step; range.value = value;
      range.addEventListener("input", function () {
        onInput(parseFloat(range.value));
        placeLights();
        draw();
      });
      ctl.appendChild(text); ctl.appendChild(range);
      controls.appendChild(ctl);
      return range;
    }

    // In cell mode these live in the right-hand fine-tune panel instead of the
    // toolbar (built further below); here they stay in the toolbar for the
    // single-mesh viewer. angleRange is shared with the Follow-camera toggle.
    var angleRange = null;
    if (!cellMode) {
      addSlider("lightkey", "Light", 0, 4, lightState.key, 0.05,
        function (v) { lightState.key = v; });
      addSlider("lightamb", "Ambient", 0, 2, lightState.ambient, 0.05,
        function (v) { lightState.ambient = v; });
      angleRange = addSlider("lightang", "Angle", 0, 6.2832, lightState.angle, 0.02,
        function (v) { lightState.angle = v; });
    }

    var lampBox = document.createElement("input");
    lampBox.type = "checkbox"; lampBox.id = "headlamp";
    var lampCtl = document.createElement("span");
    lampCtl.className = "ctl off";
    var lampLabel = document.createElement("label");
    lampLabel.htmlFor = "headlamp";
    lampLabel.textContent = "Follow camera";
    lampCtl.appendChild(lampBox); lampCtl.appendChild(lampLabel);
    controls.appendChild(lampCtl);
    lampBox.addEventListener("change", function () {
      lightState.headlamp = lampBox.checked;
      // The fixed-angle slider means nothing while the light tracks the
      // camera, so it is disabled rather than left to look operative (it lives
      // in the right panel in cell mode, or not at all if not built).
      if (angleRange) angleRange.disabled = lampBox.checked;
      lampCtl.className = "ctl" + (lampBox.checked ? "" : " off");
      placeLights();
      draw();
    });

    // Only offered when the collection actually ships one. A permanently
    // dead control implies the feature is broken rather than unused.
    if (anyNormalMaps && !cellMode) {
      var normalBox = document.createElement("input");
      normalBox.type = "checkbox"; normalBox.id = "normals";
      var normalCtl = document.createElement("span");
      normalCtl.className = "ctl off";
      var normalLabel = document.createElement("label");
      normalLabel.htmlFor = "normals";
      normalLabel.textContent = "Normal maps";
      normalCtl.appendChild(normalBox); normalCtl.appendChild(normalLabel);
      controls.appendChild(normalCtl);
      normalBox.addEventListener("change", function () {
        scene.traverse(function (o) {
          if (!o.isMesh || !o.userData.normalMap) return;
          o.material.normalMap = normalBox.checked ? o.userData.normalMap : null;
          o.material.needsUpdate = true;
        });
        normalCtl.className = "ctl" + (normalBox.checked ? "" : " off");
        draw();
      });
    }

    if (anySpecularMaps && !cellMode) {
      var specBox = document.createElement("input");
      specBox.type = "checkbox"; specBox.id = "specular";
      var specCtl = document.createElement("span");
      specCtl.className = "ctl off";
      var specLabel = document.createElement("label");
      specLabel.htmlFor = "specular";
      specLabel.textContent = "Specular maps";
      specCtl.appendChild(specBox); specCtl.appendChild(specLabel);
      controls.appendChild(specCtl);
      specBox.addEventListener("change", function () {
        scene.traverse(function (o) {
          if (!o.isMesh || !o.userData.specularMap) return;
          o.material.specularMap = specBox.checked ? o.userData.specularMap : null;
          // The highlight color moves with the map. A specular map modulates
          // the specular color, so a bright color with no map is a highlight
          // over the whole surface -- the state that made this control look
          // inverted. 0x111111 is three.js's own default, i.e. what the
          // material would have had if no specular map had ever been found.
          o.material.specular = new THREE.Color(specBox.checked ? 0x808080 : 0x111111);
          o.material.needsUpdate = true;
        });
        specCtl.className = "ctl" + (specBox.checked ? "" : " off");
        draw();
      });
    }

    if (anyGlowMaps && !cellMode) {
      var glowBox = document.createElement("input");
      glowBox.type = "checkbox"; glowBox.id = "glow";
      glowBox.checked = true;  // glow is bound at creation, so start on and in sync
      var glowCtl = document.createElement("span");
      glowCtl.className = "ctl";
      var glowLabel = document.createElement("label");
      glowLabel.htmlFor = "glow";
      glowLabel.textContent = "Glow maps";
      glowCtl.appendChild(glowBox); glowCtl.appendChild(glowLabel);
      controls.appendChild(glowCtl);
      glowBox.addEventListener("change", function () {
        // On by default: self-illumination is base-game rendering (lamps, lava,
        // glowing eyes), and the map is already bound at creation. This toggle
        // lets you switch it off; unlike the other optional maps, glow starts on.
        scene.traverse(function (o) {
          if (!o.isMesh || !o.userData.glowMap) return;
          o.material.emissiveMap = glowBox.checked ? o.userData.glowMap : null;
          o.material.emissive = glowBox.checked ? new THREE.Color(0xffffff) : new THREE.Color(0x000000);
          o.material.needsUpdate = true;
        });
        glowCtl.className = "ctl" + (glowBox.checked ? "" : " off");
        draw();
      });
    }

    if (anyDarkMaps && !cellMode) {
      var darkBox = document.createElement("input");
      darkBox.type = "checkbox"; darkBox.id = "dark";
      var darkCtl = document.createElement("span");
      darkCtl.className = "ctl off";
      var darkLabel = document.createElement("label");
      darkLabel.htmlFor = "dark";
      darkLabel.textContent = "Dark maps";
      darkCtl.appendChild(darkBox); darkCtl.appendChild(darkLabel);
      controls.appendChild(darkCtl);
      darkBox.addEventListener("change", function () {
        // Nothing here sets a material property directly -- attachExtraSlots
        // reads darkBox.checked itself the next time the shader compiles,
        // which needsUpdate is what triggers.
        scene.traverse(function (o) {
          if (o.isMesh && o.userData.darkMap) o.material.needsUpdate = true;
        });
        darkCtl.className = "ctl" + (darkBox.checked ? "" : " off");
        draw();
      });
    }

    if (anyDetailMaps && !cellMode) {
      var detailBox = document.createElement("input");
      detailBox.type = "checkbox"; detailBox.id = "detail";
      var detailCtl = document.createElement("span");
      detailCtl.className = "ctl off";
      var detailLabel = document.createElement("label");
      detailLabel.htmlFor = "detail";
      detailLabel.textContent = "Detail maps";
      detailCtl.appendChild(detailBox); detailCtl.appendChild(detailLabel);
      controls.appendChild(detailCtl);
      detailBox.addEventListener("change", function () {
        scene.traverse(function (o) {
          if (o.isMesh && o.userData.detailMap) o.material.needsUpdate = true;
        });
        detailCtl.className = "ctl" + (detailBox.checked ? "" : " off");
        draw();
      });
    }

    if (anyGlossMaps && !cellMode) {
      var glossBox = document.createElement("input");
      glossBox.type = "checkbox"; glossBox.id = "gloss";
      var glossCtl = document.createElement("span");
      glossCtl.className = "ctl off";
      var glossLabel = document.createElement("label");
      glossLabel.htmlFor = "gloss";
      glossLabel.textContent = "Gloss maps";
      glossCtl.appendChild(glossBox); glossCtl.appendChild(glossLabel);
      controls.appendChild(glossCtl);
      glossBox.addEventListener("change", function () {
        scene.traverse(function (o) {
          if (o.isMesh && o.userData.glossMap) o.material.needsUpdate = true;
        });
        glossCtl.className = "ctl" + (glossBox.checked ? "" : " off");
        draw();
      });
    }

    if (anyDecalMaps && !cellMode) {
      var decalBox = document.createElement("input");
      decalBox.type = "checkbox"; decalBox.id = "decal";
      var decalCtl = document.createElement("span");
      decalCtl.className = "ctl off";
      var decalLabel = document.createElement("label");
      decalLabel.htmlFor = "decal";
      decalLabel.textContent = "Decal maps";
      decalCtl.appendChild(decalBox); decalCtl.appendChild(decalLabel);
      controls.appendChild(decalCtl);
      decalBox.addEventListener("change", function () {
        scene.traverse(function (o) {
          if (o.isMesh && o.userData.decalMaps && o.userData.decalMaps.length) {
            o.material.needsUpdate = true;
          }
        });
        decalCtl.className = "ctl" + (decalBox.checked ? "" : " off");
        draw();
      });
    }

    if (anyBumpMaps && !cellMode) {
      var bumpBox = document.createElement("input");
      bumpBox.type = "checkbox"; bumpBox.id = "bump";
      var bumpCtl = document.createElement("span");
      bumpCtl.className = "ctl off";
      var bumpLabel = document.createElement("label");
      bumpLabel.htmlFor = "bump";
      bumpLabel.textContent = "Bump as normal (MGE)";
      bumpCtl.appendChild(bumpBox); bumpCtl.appendChild(bumpLabel);
      controls.appendChild(bumpCtl);
      bumpBox.addEventListener("change", function () {
        // Unlike the four above, this is a real MeshPhongMaterial property
        // (normalMap), not a shader injection -- three.js already supports
        // it correctly, including the tangent-space lighting math, so there
        // is nothing to patch. An OpenMW-style _n/_nh sibling still wins
        // when a mesh happens to carry both, rather than the two fighting
        // over which is current: MGE-converted content and OpenMW-authored
        // normal maps are different eras of the same idea, not a pair meant
        // to be layered.
        scene.traverse(function (o) {
          if (!o.isMesh || !o.userData.bumpMap) return;
          if (o.userData.normalMap) return;
          o.material.normalMap = bumpBox.checked ? o.userData.bumpMap : null;
          o.material.needsUpdate = true;
        });
        bumpCtl.className = "ctl" + (bumpBox.checked ? "" : " off");
        draw();
      });
    }

    // Only offered when the collection actually has collision geometry, for
    // the same reason as the normal-map control above.
    if (anyCollision) {
      var collisionBox = document.createElement("input");
      collisionBox.type = "checkbox"; collisionBox.id = "collision"; collisionBox.checked = false;
      var collisionCtl = document.createElement("span");
      collisionCtl.className = "ctl off";
      var collisionLabel = document.createElement("label");
      collisionLabel.htmlFor = "collision";
      collisionLabel.textContent = "Collision shapes";
      collisionCtl.appendChild(collisionBox); collisionCtl.appendChild(collisionLabel);
      controls.appendChild(collisionCtl);
      collisionBox.addEventListener("change", function () {
        // Collision geometry is never drawn in game, so hiding it here only
        // turns off its render -- the mesh itself stays in the scene graph,
        // so hiding it shrinks neither the framing nor the stats, only what
        // is drawn.
        collisionOn = collisionBox.checked;
        applyVisibility();
        collisionCtl.className = "ctl" + (collisionBox.checked ? "" : " off");
        draw();
      });
    }

    // The neighbouring cells, off by default: the opening view is the picked
    // cell alone, and this brings its surroundings in so a merge seam or a
    // floater can be read against the ground next door. Only offered when the
    // preview was built with adjacent cells, for the same reason as above.
    if (anyAdjacent) {
      var adjacentBox = document.createElement("input");
      adjacentBox.type = "checkbox"; adjacentBox.id = "adjacent"; adjacentBox.checked = true;
      var adjacentCtl = document.createElement("span");
      adjacentCtl.className = "ctl";
      var adjacentLabel = document.createElement("label");
      adjacentLabel.htmlFor = "adjacent";
      adjacentLabel.textContent = "Adjacent cells";
      adjacentCtl.appendChild(adjacentBox); adjacentCtl.appendChild(adjacentLabel);
      controls.appendChild(adjacentCtl);
      adjacentBox.addEventListener("change", function () {
        adjacentOn = adjacentBox.checked;
        applyVisibility();
        adjacentCtl.className = "ctl" + (adjacentBox.checked ? "" : " off");
        draw();
      });
    }

    // One checkbox per record type present (Static, Light, Activator, Terrain,
    // Water, ...), so a whole category can be turned off -- the light halos and
    // marker boxes that clutter a cell without being architecture. Clicking the
    // *name* solos that category (hides the rest); clicking it again restores all.
    var typeControls = [];
    function soloType(name) {
      var onlyThis = typeControls.every(function (t) {
        return t.name === name ? t.box.checked : !t.box.checked;
      });
      typeControls.forEach(function (t) {
        var on = onlyThis || t.name === name;
        t.box.checked = on;
        typeStates[t.name] = on;
        t.ctl.className = "ctl" + (on ? "" : " off");
      });
      applyVisibility();
      draw();
    }
    Object.keys(typesInScene).sort().forEach(function (typeName) {
      var typeBox = document.createElement("input");
      typeBox.type = "checkbox"; typeBox.checked = true;
      var typeCtl = document.createElement("span");
      typeCtl.className = "ctl";
      var typeLabel = document.createElement("label");
      typeLabel.textContent = typeName;
      typeLabel.title = "click to show only this category; click again to show all";
      typeLabel.addEventListener("click", function () { soloType(typeName); });
      typeCtl.appendChild(typeBox); typeCtl.appendChild(typeLabel);
      controls.appendChild(typeCtl);
      typeControls.push({name: typeName, box: typeBox, ctl: typeCtl});
      typeBox.addEventListener("change", function () {
        typeStates[typeName] = typeBox.checked;
        applyVisibility();
        typeCtl.className = "ctl" + (typeBox.checked ? "" : " off");
        draw();
      });
    });

    var spacer = document.createElement("span");
    spacer.className = "spacer";
    controls.appendChild(spacer);

    // Collapse the side panel for a (near-)fullscreen viewport -- the point of a
    // cell view is the scene, not the list beside it.
    var panelBtn = document.createElement("button");
    panelBtn.type = "button";
    panelBtn.id = "panelToggle";  // so a mesh click can reveal the panel for the ori readout
    panelBtn.textContent = "Hide panel";
    panelBtn.addEventListener("click", function () {
      var hidden = document.body.classList.toggle("nopanel");
      panelBtn.textContent = hidden ? "Show panel" : "Hide panel";
      resize(); draw();
    });
    controls.appendChild(panelBtn);
    // In cell mode the panel is a distraction by default; start it collapsed.
    if (cellMode) { document.body.classList.add("nopanel"); panelBtn.textContent = "Show panel"; }

    [["Show all", function () { groups.forEach(function (g) { g.visible = true; }); }],
     ["Reset view", function () {
       yaw = 0.6; pitch = 0.5; distance = radius * 3; target.set(0, 0, 0);
     }]
    ].forEach(function (pair) {
      var button = document.createElement("button");
      button.type = "button";
      button.textContent = pair[0];
      button.addEventListener("click", function () {
        pair[1]();
        Array.prototype.forEach.call(controls.querySelectorAll("input"), function (cb, i) {
          if (i < groups.length) cb.checked = groups[i].visible;
        });
        place(); refresh();
      });
      controls.appendChild(button);
    });

    // ---- Right-hand fine-tune panel (cell mode) -------------------------------
    // The accordion of sliders the user asked for -- lighting/time, water, and
    // sky/weather -- lives in its own collapsible right panel rather than the
    // toolbar, so the toolbar stays a row of toggles and the fiddly tuning has
    // room to breathe.
    if (cellMode) {
      var rpanel = document.getElementById("rpanel");
      var rtitle = document.createElement("h3");
      rtitle.textContent = "Fine-tune";
      rpanel.appendChild(rtitle);
      function rpGroup(groupTitle, open) {
        var det = document.createElement("details");
        if (open) det.open = true;
        var sum = document.createElement("summary");
        sum.textContent = groupTitle;
        det.appendChild(sum);
        var bodyDiv = document.createElement("div");
        bodyDiv.className = "accbody";
        det.appendChild(bodyDiv);
        rpanel.appendChild(det);
        return bodyDiv;
      }
      function rpSlider(bodyDiv, label, min, max, value, step, onInput) {
        var row = document.createElement("div");
        row.className = "row";
        var lab = document.createElement("label");
        lab.textContent = label;
        var rng = document.createElement("input");
        rng.type = "range";
        rng.min = min; rng.max = max; rng.step = step; rng.value = value;
        rng.addEventListener("input", function () { onInput(parseFloat(rng.value)); draw(); });
        row.appendChild(lab); row.appendChild(rng);
        bodyDiv.appendChild(row);
        return rng;
      }
      function rpCheckbox(bodyDiv, label, checked, onChange) {
        var row = document.createElement("div");
        row.className = "row";
        var lab = document.createElement("label");
        lab.textContent = label;
        var box = document.createElement("input");
        box.type = "checkbox";
        box.checked = checked;
        box.addEventListener("change", function () { onChange(box.checked); draw(); });
        row.appendChild(lab); row.appendChild(box);
        bodyDiv.appendChild(row);
        return box;
      }
      function setWaterUniform(name, val) {
        for (var wi = 0; wi < waterMaterials.length; wi++) {
          waterMaterials[wi].uniforms[name].value = val;
        }
        viewDirty = true;
      }
      function setUnderUniform(name, val) {
        if (name === "uFogDensity") underState.fog = val;
        if (name === "uCausticSharp") underState.causticSharp = val;
        if (name === "uHueShift") underState.hue = val;
        if (underMat) underMat.uniforms[name].value = val;
        viewDirty = true;
      }
      // The caustic net is drawn by both the surface and the underwater pass, so
      // one control sets its line width on both.
      function setCausticSharp(v) {
        setWaterUniform("uCausticSharp", v);
        setUnderUniform("uCausticSharp", v);
      }

      var gLight = rpGroup("Lighting & time", true);
      rpSlider(gLight, "Light", 0, 4, lightState.key, 0.05,
        function (v) { lightState.key = v; placeLights(); });
      rpSlider(gLight, "Ambient", 0, 2, lightState.ambient, 0.05,
        function (v) { lightState.ambient = v; placeLights(); });
      angleRange = rpSlider(gLight, "Light angle", 0, 6.2832, lightState.angle, 0.02,
        function (v) { lightState.angle = v; placeLights(); });
      if (!singleSided) {
        rpSlider(gLight, "Time of day", 0, 24, timeOfDay, 0.1,
          function (v) { applyTimeOfDay(v); });
      }

      // Play/pause for everything clock-driven (water, scrolling banners, sway,
      // blink, cloth ripple). Only offered when the scene actually has some.
      if (hasAnimation()) {
        var gAnim = rpGroup("Animation", true);
        rpCheckbox(gAnim, "Play", true, function (on) {
          animateOn = on;
          startAnimLoop();  // ensure the loop is running (no-op if already)
          if (!on) draw();  // settle on the current frame while paused
        });
      }

      if (waterMaterials.length) {
        var gWater = rpGroup("Water", true);
        // Turns the fancy surface + underwater shaders off, falling the water back
        // to a plain blue tint (and skipping the heavy refraction/underwater passes
        // -- a lighter option on weak GPUs). Hiding water entirely is a separate
        // toggle (the "Water" record-type checkbox up top).
        rpCheckbox(gWater, "Water shader", true, function (on) {
          waterShaderOn = on;
          applyWaterShader();
        });
        rpSlider(gWater, "Wave height", 0, 2.5, 1.0, 0.05,
          function (v) { setWaterUniform("uWaveScale", v); });
        rpSlider(gWater, "Chop / detail", 0, 16, 8.0, 0.5,
          function (v) { setWaterUniform("uChop", v); });
        rpSlider(gWater, "Clarity", 400, 4000, 1800, 50,
          function (v) { setWaterUniform("uClarity", v); });
        // Keeps the blue hue but thins its opacity, so the bottom shows through
        // more the higher it goes (0 = the deep default, 1 = clearest).
        rpSlider(gWater, "See-through", 0, 1, 0.0, 0.02,
          function (v) { setWaterUniform("uSeeThrough", v); });
        rpSlider(gWater, "Caustics", 0, 2.5, 1.0, 0.05,
          function (v) { setWaterUniform("uCausticStr", v); });
        rpSlider(gWater, "Caustic width", 0.5, 6, 1.5, 0.1,
          function (v) { setCausticSharp(v); });
        rpSlider(gWater, "Sparkle", 0, 6, 2.5, 0.1,
          function (v) { setWaterUniform("uSparkle", v); });
        rpSlider(gWater, "Shore tint", 0, 0.6, 0.14, 0.01,
          function (v) { setWaterUniform("uTintStr", v); });
        rpSlider(gWater, "Shore tint reach", 100, 2000, 500, 25,
          function (v) { setWaterUniform("uTintDepth", v); });
        // Rotate the surface water's hue (radians), 0 = unchanged.
        rpSlider(gWater, "Hue", -3.14, 3.14, 0.0, 0.02,
          function (v) { setWaterUniform("uHueShift", v); });

        // Underwater view: how far you can see before the murk closes in. Clarity
        // 0 is thick fog, 1 is nearly clear; it maps to the fog density the
        // full-screen underwater pass uses (0.75 reproduces the default look).
        var gUnder = rpGroup("Underwater", false);
        rpSlider(gUnder, "Clarity", 0, 1, 0.75, 0.02,
          function (v) { setUnderUniform("uFogDensity", 0.0038 - v * 0.0036); });
        rpSlider(gUnder, "Hue", -3.14, 3.14, 0.0, 0.02,
          function (v) { setUnderUniform("uHueShift", v); });
      }

      if (!singleSided) {
        var gSky = rpGroup("Sky & weather", false);
        var wrow = document.createElement("div");
        wrow.className = "row";
        var wlab = document.createElement("label");
        wlab.textContent = "Weather";
        var wsel = document.createElement("select");
        [["clear", "Clear"], ["cloudy", "Cloudy"], ["foggy", "Foggy"],
         ["overcast", "Overcast"], ["storm", "Storm"], ["ashstorm", "Ashstorm"],
         ["blight", "Blight"], ["snow", "Snow"]].forEach(function (o) {
          var opt = document.createElement("option");
          opt.value = o[0]; opt.textContent = o[1];
          wsel.appendChild(opt);
        });
        wsel.addEventListener("change", function () { applyWeather(wsel.value); draw(); });
        wrow.appendChild(wlab); wrow.appendChild(wsel);
        gSky.appendChild(wrow);
      }

      // Toolbar button to show/hide the panel.
      var rpBtn = document.createElement("button");
      rpBtn.type = "button";
      rpBtn.textContent = "Water & Sky";
      rpBtn.addEventListener("click", function () {
        document.body.classList.toggle("rpanel");
        resize(); draw();
      });
      controls.appendChild(rpBtn);
    }

    // Weather swaps in that weather's own Morrowind sky texture (dome and water
    // reflection both), and applies a gentle colour cast on top for the darker,
    // moodier types -- the texture carries most of the look, the cast just deepens
    // it. Clear restores the plain sky.
    function applyWeather(name) {
      var table = {
        clear: [1.0, 1.0, 1.0], cloudy: [0.95, 0.96, 0.98], foggy: [0.90, 0.92, 0.94],
        overcast: [0.78, 0.80, 0.84], storm: [0.62, 0.65, 0.72], ashstorm: [0.78, 0.68, 0.56],
        blight: [0.85, 0.66, 0.60], snow: [0.95, 0.96, 1.0]
      };
      var m = table[name] || [1.0, 1.0, 1.0];
      weatherMult.setRGB(m[0], m[1], m[2]);
      loadSky(name);
      applyTimeOfDay(timeOfDay);
    }

    var yaw = 0.6, pitch = 0.5, distance = radius * 3;
    // What the camera orbits and looks at. Panning moves this rather than the
    // camera directly, so zoom and orbit keep working exactly as before --
    // they are still defined relative to a point, just one that is no longer
    // pinned to the mesh's centre.
    var target = new THREE.Vector3(0, 0, 0);
    function place() {
      camera.position.set(
        target.x + distance * Math.cos(pitch) * Math.sin(yaw),
        target.y + distance * Math.sin(pitch),
        target.z + distance * Math.cos(pitch) * Math.cos(yaw));
      camera.lookAt(target);
      // The camera moved, so the cached refraction of the scene beneath the water
      // is stale and must be re-rendered on the next frame.
      viewDirty = true;
      // In headlamp mode the light rides the camera, so it has to move here
      // rather than only when a slider changes.
      placeLights();
    }
    // Keep the refraction target the size of the drawing buffer, rebuilding it
    // when the canvas resizes. A depth texture rides alongside the colour so the
    // water shader can read both what is beneath it and how far down it is.
    function ensureRefractRT() {
      var sz = renderer.getDrawingBufferSize(new THREE.Vector2());
      var w = Math.max(1, Math.floor(sz.x * REFRACT_SCALE));
      var h = Math.max(1, Math.floor(sz.y * REFRACT_SCALE));
      if (refractRT && refractRT.width === w && refractRT.height === h) return;
      if (refractRT) { refractRT.depthTexture.dispose(); refractRT.dispose(); }
      refractRT = new THREE.WebGLRenderTarget(w, h);
      refractRT.texture.minFilter = THREE.LinearFilter;
      refractRT.texture.magFilter = THREE.LinearFilter;
      refractRT.depthTexture = new THREE.DepthTexture(w, h);
      viewDirty = true;  // a new target must be filled before it is sampled
    }
    function ensureSceneRT() {
      var sz = renderer.getDrawingBufferSize(new THREE.Vector2());
      var w = Math.max(1, Math.floor(sz.x)), h = Math.max(1, Math.floor(sz.y));
      if (sceneRT && sceneRT.width === w && sceneRT.height === h) return;
      if (sceneRT) { sceneRT.depthTexture.dispose(); sceneRT.dispose(); }
      sceneRT = new THREE.WebGLRenderTarget(w, h);
      sceneRT.texture.minFilter = THREE.LinearFilter;
      sceneRT.texture.magFilter = THREE.LinearFilter;
      sceneRT.depthTexture = new THREE.DepthTexture(w, h);
    }
    function draw() {
      // No water, or the fancy shader is off: a single plain render. The water
      // planes (now the simple tint) draw with the rest, and the refraction and
      // underwater passes are skipped entirely.
      if (!waterMaterials.length || !waterShaderOn) { renderer.render(scene, camera); return; }
      ensureRefractRT();
      var sz = renderer.getDrawingBufferSize(new THREE.Vector2());
      // Refresh the refraction target only when the camera has moved: the scene
      // beneath the water is static, so between camera moves the cached target is
      // reused and the ripple frames cost one scene render, not two.
      if (viewDirty) {
        var wasVisible = [];
        for (var i = 0; i < waterMeshes.length; i++) {
          wasVisible.push(waterMeshes[i].visible);
          waterMeshes[i].visible = false;
        }
        renderer.setRenderTarget(refractRT);
        renderer.render(scene, camera);
        renderer.setRenderTarget(null);
        for (var j = 0; j < waterMeshes.length; j++) waterMeshes[j].visible = wasVisible[j];
        _invViewProj.copy(camera.projectionMatrix).multiply(camera.matrixWorldInverse).invert();
        for (var k = 0; k < waterMaterials.length; k++) {
          var uw = waterMaterials[k].uniforms;
          uw.uSceneColor.value = refractRT.texture;
          uw.uSceneDepth.value = refractRT.depthTexture;
          uw.uResolution.value.set(sz.x, sz.y);
          uw.uCamNear.value = camera.near;
          uw.uCamFar.value = camera.far;
          uw.uInvViewProj.value.copy(_invViewProj);
        }
        viewDirty = false;
      }
      // Is the camera below the water surface? (Its world height -- the plane is
      // horizontal, so any point's Y is the surface level.)
      waterMeshes[0].getWorldPosition(_tmpV);
      var underwater = camera.position.y < _tmpV.y;
      if (underwater) {
        // Render the whole frame (water included) to a target, then run it
        // through the underwater shader: fog, seabed caustics and wobble.
        ensureUnderwater();
        ensureSceneRT();
        renderer.setRenderTarget(sceneRT);
        renderer.render(scene, camera);
        renderer.setRenderTarget(null);
        var u = underMat.uniforms;
        u.uColor.value = sceneRT.texture;
        u.uDepth.value = sceneRT.depthTexture;
        u.uResolution.value.set(sz.x, sz.y);
        u.uCamNear.value = camera.near;
        u.uCamFar.value = camera.far;
        u.uTime.value = waterMaterials[0].uniforms.uTime.value;
        u.uInvViewProj.value.copy(_invViewProj);
        u.uWaterY.value = _tmpV.y;
        renderer.render(fsScene, fsCamera);
      } else {
        renderer.render(scene, camera);
      }
    }
    function resize() {
      var w = stage.clientWidth, h = stage.clientHeight;
      renderer.setSize(w, h, false);
      camera.aspect = w / Math.max(h, 1);
      camera.updateProjectionMatrix();
      viewDirty = true;  // the target size and projection changed
    }
    var dragging = false, dragMode = "orbit", lastX = 0, lastY = 0;
    var downX = 0, downY = 0, moved = false;
    // Raycast a click (a press that did not turn into a drag) against the scene
    // and name what it hits, so you can identify a mesh in the 3D view rather
    // than hunt for it in the shape list. Collision/hidden meshes are invisible
    // and so are skipped, matching what the toggle shows.
    var picker = new THREE.Raycaster();
    var pickedEl = document.getElementById("picked");
    // Bring the left panel back if it is collapsed, so a click's readout is seen.
    function revealPanel() {
      if (!document.body.classList.contains("nopanel")) return;
      document.body.classList.remove("nopanel");
      var toggle = document.getElementById("panelToggle");
      if (toggle) toggle.textContent = "Hide panel";
      resize(); draw();
    }
    function pickAt(clientX, clientY) {
      var rect = renderer.domElement.getBoundingClientRect();
      var ndc = new THREE.Vector2(
        ((clientX - rect.left) / rect.width) * 2 - 1,
        -((clientY - rect.top) / rect.height) * 2 + 1
      );
      picker.setFromCamera(ndc, camera);
      var hits = picker.intersectObjects(pivot.children, true);
      for (var i = 0; i < hits.length; i++) {
        var o = hits[i].object;
        if (o.visible && o.isMesh) {
          // Prefer the object id of the exact instance clicked (the CS Object
          // ID); fall back to the shape name when there is no id list.
          var ids = o.userData.refIds, inst = hits[i].instanceId;
          var label = (ids && inst != null && ids[inst]) ? ids[inst]
                    : (o.name || "(unnamed shape)");
          pickedEl.textContent = label;
          pickedEl.style.display = "block";
          // In cell mode, the left panel shows an "ori" readout for the clicked
          // object: its provenance (from objectInfo) and its winning texture.
          if (cellMode) {
            selected = {id: label, texture: o.userData.texturePath || "",
                        type: o.userData.recordType || ""};
            revealPanel();  // the readout is no use behind a collapsed panel
            refreshTree();
          }
          return;
        }
      }
      pickedEl.style.display = "none";  // clicked empty space: clear the label
      if (cellMode && selected) { selected = null; refreshTree(); }
    }
    // A right-drag must not also pop up the browser's context menu.
    renderer.domElement.addEventListener("contextmenu", function (e) { e.preventDefault(); });
    renderer.domElement.addEventListener("mousedown", function (e) {
      dragging = true; lastX = downX = e.clientX; lastY = downY = e.clientY; moved = false;
      dragMode = (e.button === 2 || e.shiftKey) ? "pan" : "orbit";
    });
    window.addEventListener("mouseup", function () {
      if (dragging && !moved) pickAt(downX, downY);
      dragging = false;
    });
    window.addEventListener("mousemove", function (e) {
      if (!dragging) return;
      if (Math.abs(e.clientX - downX) + Math.abs(e.clientY - downY) > 4) moved = true;
      var dx = e.clientX - lastX, dy = e.clientY - lastY;
      if (dragMode === "pan") {
        // Move the point the camera looks at across the camera's own
        // right/up axes rather than the world's, so panning "up" always
        // means up on screen whatever angle the mesh is being viewed from.
        // Scaled by distance so a drag covers the same apparent screen
        // distance whether zoomed in on a buckle or zoomed out on a whole
        // building -- a fixed world-space step would crawl at one zoom and
        // fly past the mesh at another.
        var panScale = distance * 0.0015;
        var rightAxis = new THREE.Vector3().setFromMatrixColumn(camera.matrix, 0);
        var upAxis = new THREE.Vector3().setFromMatrixColumn(camera.matrix, 1);
        target.addScaledVector(rightAxis, -dx * panScale);
        target.addScaledVector(upAxis, dy * panScale);
      } else {
        yaw -= dx * 0.01;
        pitch += dy * 0.01;
        // Stop short of the poles: at exactly +/-90 degrees the look-at up
        // vector is parallel to the view and the image flips.
        pitch = Math.max(-1.55, Math.min(1.55, pitch));
      }
      lastX = e.clientX; lastY = e.clientY;
      place(); draw();
    });
    renderer.domElement.addEventListener("wheel", function (e) {
      e.preventDefault();
      distance *= (e.deltaY > 0) ? 1.1 : 0.9;
      distance = Math.max(radius * 0.05, Math.min(radius * 60, distance));
      place(); draw();
    }, {passive: false});
    // WASD flies the point the camera looks at across the ground (Q/E drop and
    // lift it), so you can move through a cell rather than only orbit one spot.
    // Ignored while a form control has focus, so typing in the panel is unaffected.
    window.addEventListener("keydown", function (e) {
      if (e.target && /^(INPUT|SELECT|TEXTAREA)$/.test(e.target.tagName)) return;
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      var k = (e.key || "").toLowerCase();
      var fwd = new THREE.Vector3();
      camera.getWorldDirection(fwd); fwd.y = 0;
      if (fwd.lengthSq() < 1e-6) fwd.set(0, 0, -1);
      fwd.normalize();
      var rightAxis = new THREE.Vector3().crossVectors(fwd, new THREE.Vector3(0, 1, 0)).normalize();
      var step = distance * 0.06;
      var moved = true;
      if (k === "w") target.addScaledVector(fwd, step);
      else if (k === "s") target.addScaledVector(fwd, -step);
      else if (k === "a") target.addScaledVector(rightAxis, -step);
      else if (k === "d") target.addScaledVector(rightAxis, step);
      else if (k === "q") target.y -= step;
      else if (k === "e") target.y += step;
      else moved = false;
      if (moved) { e.preventDefault(); place(); draw(); }
    });
    window.addEventListener("resize", function () { resize(); draw(); });
    applyVisibility();
    resize(); place(); refresh();

    // Only a scene with water animates; everything else stays on-demand, drawn
    // when the camera or a toggle changes. The water shader moves its swell and
    // ripples by the ``uTime`` uniform, so advancing that and redrawing keeps the
    // surface (and its reflection) alive while the view sits still.
    //
    // Throttled to 25 fps -- Morrowind's own water tick (openmw.cfg
    // PixelWater_SurfaceFPS = 25). requestAnimationFrame still schedules at the
    // display rate, but the scene is only re-drawn every 40 ms, which is plenty
    // for water and leaves the GPU free for texture streaming and camera work
    // (dragging draws directly, so it stays smooth).
    // Whether the scene has anything clock-driven to play at all.
    function hasAnimation() {
      return !!(waterMaterials.length || uvAnimated.length || xformAnimated.length
                || visAnimated.length || morphAnimated.length || pointsAnimated.length);
    }
    // Start the animation loop once. Hoisted (a function declaration), so the
    // "Play" checkbox built earlier can call it. Throttled to 25 fps -- Morrowind's
    // own water tick. The rAF stays scheduled even while paused, so toggling Play
    // resumes it without a restart.
    function startAnimLoop() {
      if (animLoopStarted || !hasAnimation()) return;
      animLoopStarted = true;
      var nowMs = function () {
        return (typeof performance !== "undefined" ? performance.now() : Date.now());
      };
      var animStart = nowMs();
      var animFrameMs = 1000 / 25;
      var lastAnimDraw = -1e9;
      var tickAnim = function () {
        if (animateOn) {
          var now = nowMs();
          if (now - lastAnimDraw >= animFrameMs) {
            lastAnimDraw = now;
            var seconds = (now - animStart) / 1000;
            for (var wi = 0; wi < waterMaterials.length; wi++) {
              waterMaterials[wi].uniforms.uTime.value = seconds;
            }
            advanceUvAnimations(seconds);
            advanceXformAnimations(seconds);
            advanceVisAnimations(seconds);
            advanceMorphAnimations(seconds);
            advancePointsAnimations(seconds);
            draw();
          }
        }
        requestAnimationFrame(tickAnim);
      };
      requestAnimationFrame(tickAnim);
    }
    startAnimLoop();
  }
})();
</script>
__EDIT_JS__
</body></html>
"""


#: The editor overlay, injected only for an editable view. Kept out of
#: :data:`_PAGE` and run as its own script so the large, read-only viewer is not
#: touched by it: it works off the DOM the viewer already built (the block tree's
#: ``li[data-block-index]`` rows) and a single ``EDIT`` payload, so a
#: non-editable page carries none of this and behaves exactly as before. It
#: collects field edits, POSTs them to the server's apply handler, and saves the
#: returned file. A **raw** string for the same reason :data:`_PAGE` is.
_EDITOR_JS: Final[str] = r"""<script>
(function () {
  var EDIT = __EDIT_DATA__;
  if (!EDIT || !EDIT.url) return;
  var insp = document.getElementById("inspector");
  var treeBox = document.getElementById("tree");
  var controls = document.getElementById("controls");
  if (!insp || !treeBox || !controls) return;
  insp.style.display = "flex";
  var pending = {};

  var save = document.createElement("button");
  save.className = "save-btn"; save.textContent = "Save edited .nif";
  var status = document.createElement("span"); status.className = "save-status";
  controls.appendChild(save); controls.appendChild(status);
  save.addEventListener("click", doSave);

  treeBox.addEventListener("click", function (e) {
    var li = e.target;
    while (li && li !== treeBox && !li.hasAttribute("data-block-index")) li = li.parentNode;
    if (!li || li === treeBox) return;
    select(parseInt(li.getAttribute("data-block-index"), 10), li);
  });

  function esc(s) {
    return String(s).replace(/[&<>"]/g, function (c) {
      return {"&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;"}[c];
    });
  }
  function refreshStatus() {
    var n = Object.keys(pending).length;
    status.textContent = n ? (n + " edit" + (n > 1 ? "s" : "") + " pending") : "";
  }
  function select(idx, li) {
    Array.prototype.forEach.call(treeBox.querySelectorAll("li.sel"), function (x) {
      x.classList.remove("sel");
    });
    if (li) li.classList.add("sel");
    render(idx);
  }
  function widget(f) {
    if (!f.editable) return '<span class="roval">' + esc(f.value === null ? "—" : f.value) + "</span>";
    if (f.kind === "bool32") {
      return '<input type="checkbox" data-field="' + esc(f.name) + '" data-kind="bool32"' +
        (f.value === true ? " checked" : "") + ">";
    }
    return '<input class="f" type="text" data-field="' + esc(f.name) + '" data-kind="' +
      esc(f.kind) + '" value="' + esc(f.value === null ? "" : f.value) + '">';
  }
  function rows(fields) {
    return fields.map(function (f) {
      return '<div class="frow ' + (f.editable ? "" : "ro") + '"><div class="lab"><span class="fname">' +
        esc(f.name) + '</span><span class="kind">' + esc(f.kind) + "</span></div>" + widget(f) + "</div>";
    }).join("");
  }
  function render(idx) {
    var b = EDIT.blocks[idx];
    if (!b) { insp.innerHTML = '<div class="insp-empty">This block has no editable fields.</div>'; return; }
    var ed = b.fields.filter(function (f) { return f.editable; });
    var ro = b.fields.filter(function (f) { return !f.editable; });
    insp.innerHTML =
      '<div class="insp-head"><div class="nm">' + esc(b.type) + '</div><div class="sub">block ' +
      idx + " &middot; " + b.fields.length + " fields</div></div>" +
      '<div class="sect"><div class="sh">PROPERTIES &middot; EDITABLE (' + ed.length + ')</div><div class="body">' +
      (rows(ed) || '<div class="insp-empty">No directly-editable fields.</div>') + "</div></div>" +
      '<div class="sect"><div class="sh">STRUCTURE &middot; READ-ONLY (' + ro.length + ')</div><div class="body">' +
      rows(ro) + "</div></div>";
    Array.prototype.forEach.call(insp.querySelectorAll("[data-field]"), function (el) {
      el.addEventListener("change", function () { onEdit(idx, el); });
    });
  }
  function coerce(el) {
    var k = el.getAttribute("data-kind");
    if (k === "bool32") return el.checked;
    if (k === "f32") return parseFloat(el.value);
    if (k === "u8" || k === "u16" || k === "u32" || k === "i32" || k === "link") return parseInt(el.value, 10);
    return el.value;
  }
  function onEdit(idx, el) {
    var name = el.getAttribute("data-field");
    pending[idx + ":" + name] = {op: "set_field", block: idx, name: name, value: coerce(el)};
    var frow = el.parentNode;
    while (frow && frow.className && frow.className.indexOf("frow") < 0) frow = frow.parentNode;
    if (frow && frow.classList) frow.classList.add("dirty");
    refreshStatus();
  }
  function doSave() {
    var edits = Object.keys(pending).map(function (k) { return pending[k]; });
    if (!edits.length) { status.textContent = "no edits to save"; return; }
    status.textContent = "saving…";
    fetch(EDIT.url, {
      method: "POST", headers: {"Content-Type": "application/json"},
      body: JSON.stringify({edits: edits})
    }).then(function (r) {
      if (!r.ok) return r.text().then(function (t) { throw new Error(t || ("HTTP " + r.status)); });
      return r.blob();
    }).then(function (blob) {
      var a = document.createElement("a");
      a.href = URL.createObjectURL(blob);
      a.download = EDIT.filename || "edited.nif";
      document.body.appendChild(a); a.click(); a.remove();
      setTimeout(function () { URL.revokeObjectURL(a.href); }, 1000);
      pending = {};
      Array.prototype.forEach.call(insp.querySelectorAll(".frow.dirty"), function (x) {
        x.classList.remove("dirty");
      });
      status.textContent = "saved ✓";
    }).catch(function (err) { status.textContent = "error: " + err.message; });
  }
})();
</script>"""
