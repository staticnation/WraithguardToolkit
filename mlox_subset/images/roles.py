"""What a texture is *for*, which decides how it should be shown and compared.

A texture is not just an image. ``tx_rock_01.dds`` is colour a human can judge;
``tx_rock_01_n.dds`` is a field of surface normals that only means anything to
a shader. Showing the second one the way you show the first produces the lurid
blue-green sheet everyone recognises, and -- more importantly -- comparing them
as though they were the same kind of thing is a category error.

**Morrowind and OpenMW say it in different places, and both are authoritative.**

*Vanilla* puts the role in the mesh. A ``NiTexturingProperty`` has named slots
-- base, dark, detail, gloss, glow, bump -- and whichever slot a
``NiSourceTexture`` hangs off is what the engine treats it as. The file name is
irrelevant; the same image can be a base map on one shape and a glow map on
another.

*OpenMW* additionally infers roles from **file name suffixes**, configured under
``[Shaders]`` in ``settings.cfg``, so that a mod can supply normal and specular
maps for meshes that never declared any: ``_n`` for a normal map, ``_nh`` for
normal-plus-height in the alpha channel, ``_spec`` for specular, and
``_diffusespec`` for the terrain layer that packs specular into diffuse alpha.

So the mesh is consulted when there is a mesh, and the name when there is not.
Neither alone is complete: a vanilla glow map has no suffix, and an OpenMW
normal map added by a texture pack appears in no mesh at all.

**Why this matters to a conflict report.** Two mods overwriting the same
``_n.dds`` are both shipping normal maps, so comparing them against each other
is exactly right and the difference is meaningful. Two mods where one ships
``tx_rock.dds`` and another ships ``tx_rock_n.dds`` are not in conflict at all
-- they are contributing different channels of the same material. Getting that
distinction wrong turns a complementary pair into a reported clash.
"""

from __future__ import annotations

from enum import Enum
from typing import Final


class TextureRole(Enum):
    """What part a texture plays in rendering a surface.

    The values are stable, lower-case identifiers so they can go into a CSV
    column or a report without a separate display mapping.
    """

    DIFFUSE = "diffuse"
    """Base colour: the one a human can look at and judge."""

    DARK = "dark"
    """A vanilla multiply layer, darkening the base."""

    DETAIL = "detail"
    """A vanilla high-frequency layer tiled over the base."""

    GLOSS = "gloss"
    """Vanilla specular mask; brighter means shinier."""

    GLOW = "glow"
    """Vanilla emissive map: the parts that light themselves."""

    NORMAL = "normal"
    """Surface normals. The vanilla bump slot, or an OpenMW ``_n``."""

    NORMAL_HEIGHT = "normal-height"
    """Normals with a height field in alpha, for parallax. OpenMW ``_nh``."""

    SPECULAR = "specular"
    """OpenMW ``_spec``: specular colour in RGB, shininess in alpha."""

    DIFFUSE_SPECULAR = "diffuse-specular"
    """OpenMW ``_diffusespec``: a terrain layer with specular in alpha."""

    DECAL = "decal"
    """A vanilla overlay layer stamped onto the surface."""

    UNKNOWN = "unknown"
    """Nothing said what this is, so treat it as colour and say so."""

    @property
    def is_normal_map(self) -> bool:
        """Whether this holds vectors rather than colour.

        A normal map must not be shown as a photograph, and a difference
        between two of them is a difference in geometry, not in appearance.
        """
        return self in (TextureRole.NORMAL, TextureRole.NORMAL_HEIGHT)

    @property
    def is_mask(self) -> bool:
        """Whether this is a single-meaning greyscale mask rather than colour."""
        return self in (TextureRole.GLOSS, TextureRole.SPECULAR)

    @property
    def is_colour(self) -> bool:
        """Whether the image is meant to be looked at as colour.

        Used to decide whether a side-by-side view is telling the user
        something or just showing them noise.
        """
        return not self.is_normal_map and not self.is_mask


#: The vanilla texture slots, in the order ``NiTexturingProperty`` declares
#: them, mapped onto what the engine does with each. Decal slots repeat past
#: the end of this list -- a mesh may declare several -- so they are matched by
#: prefix rather than listed.
_SLOT_ROLES: Final[dict[str, TextureRole]] = {
    "base": TextureRole.DIFFUSE,
    "dark": TextureRole.DARK,
    "detail": TextureRole.DETAIL,
    "gloss": TextureRole.GLOSS,
    "glow": TextureRole.GLOW,
    "bump": TextureRole.NORMAL,
}

#: OpenMW's file-name suffixes, **longest first**. The order is load-bearing:
#: ``_diffusespec`` also ends in ``spec``, and matching the shorter one first
#: would file every terrain layer as a plain specular map.
_NAME_SUFFIXES: Final[tuple[tuple[str, TextureRole], ...]] = (
    ("_diffusespec", TextureRole.DIFFUSE_SPECULAR),
    ("_spec", TextureRole.SPECULAR),
    ("_nh", TextureRole.NORMAL_HEIGHT),
    ("_n", TextureRole.NORMAL),
)


def role_from_slot(slot: str) -> TextureRole:
    """Say what a vanilla texture slot means.

    Args:
        slot: A slot name as :mod:`mlox_subset.nif.reader` records it, such as
            ``"base"``, ``"glow"`` or ``"decal_0"``.

    Returns:
        The role, or :attr:`TextureRole.UNKNOWN` for a slot name not recognised.
    """
    cleaned = slot.strip().lower()
    if cleaned.startswith("decal"):
        return TextureRole.DECAL
    return _SLOT_ROLES.get(cleaned, TextureRole.UNKNOWN)


def role_from_name(reference: str) -> TextureRole:
    """Infer a role from a file name, the way OpenMW's shaders do.

    Args:
        reference: A texture path or file name, in any case and with either
            separator.

    Returns:
        The role the suffix implies, or :attr:`TextureRole.UNKNOWN` when the
        name carries no suffix -- which is the common case, and means "no
        opinion" rather than "colour".
    """
    stem = reference.strip().replace("\\", "/").rsplit("/", 1)[-1].lower()
    if "." in stem:
        stem = stem.rsplit(".", 1)[0]
    for suffix, role in _NAME_SUFFIXES:
        if stem.endswith(suffix):
            return role
    return TextureRole.UNKNOWN


def classify(reference: str, slot: str = "") -> TextureRole:
    """Decide a texture's role from everything available.

    The mesh wins when it has an opinion, because the slot is what the engine
    actually honours for a vanilla mesh; the file name is consulted second,
    because a texture pack can add a normal map for a mesh that declares none.

    One deliberate exception: a suffix that names a normal map overrides a
    ``base`` slot. That combination means a pack has dropped an OpenMW-style
    map into a mesh written before the convention existed, and treating it as
    diffuse would show a normal map as though it were colour.

    Args:
        reference: The texture path as the mesh or the file system gives it.
        slot: The vanilla slot it hangs off, when it came from a mesh.

    Returns:
        The role.
    """
    from_slot = role_from_slot(slot) if slot else TextureRole.UNKNOWN
    from_name = role_from_name(reference)
    if from_slot is TextureRole.DIFFUSE and from_name.is_normal_map:
        return from_name
    if from_slot is not TextureRole.UNKNOWN:
        return from_slot
    return from_name


def comparable(first: TextureRole, second: TextureRole) -> bool:
    """Whether two textures are the same kind of thing.

    Two normal maps compare meaningfully against each other, and so do two
    diffuse maps. A normal map against a diffuse map does not: they are
    different channels of one material, and reporting their difference would
    be reporting that red is not blue.

    Args:
        first: One texture's role.
        second: The other's.

    Returns:
        Whether a difference between them means anything.
    """
    if TextureRole.UNKNOWN in (first, second):
        # No opinion is not a mismatch. Most vanilla textures carry no suffix
        # and are never seen through a mesh, so refusing to compare them would
        # disable the feature for the base game.
        return True
    return first is second
