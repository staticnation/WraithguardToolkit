"""What a texture is *for*, which decides how it should be shown and compared.

A texture is not just an image. ``tx_rock_01.dds`` is colour a human can judge;
``tx_rock_01_n.dds`` is a field of surface normals that only means anything to
a shader. Showing the second the way you show the first produces the lurid
blue-green sheet everyone recognises, and -- more importantly -- comparing them
as though they were the same kind of thing is a category error.

**Three conventions say it, in three different places, and all three are real.**

*Vanilla* puts the role in the mesh. A ``NiTexturingProperty`` has ordered
slots -- base, dark, detail, gloss, glow, bump, then up to four decals --
assigned to the lowest free slot, and whichever slot a ``NiSourceTexture``
hangs off is what the engine treats it as. The file name is irrelevant; the
same image can be a base map on one shape and a glow map on another.

*OpenMW* additionally infers roles from **file name suffixes**, configured under
``[Shaders]`` in ``settings.cfg`` and **off by default**: ``_n`` for a normal
map, ``_nh`` for normal-plus-height, ``_spec`` for specular, ``_diffusespec``
for the terrain layer that packs specular into diffuse alpha. That exists
because the Morrowind NIF has no slot OpenMW can trust for a normal map, so a
texture pack supplies one by name for meshes that declare nothing.

*OSG native meshes* (``.osgt`` and ``.osgb``) name the role directly on the
``osg::Texture2D`` -- ``normalMap``, ``emissiveMap``, ``envMap`` and so on --
which is the only one of the three that is unambiguous.

None alone is complete, which is why :func:`classify` consults what it has.

**The bump slot is a trap, and this module does not pretend otherwise.**
Vanilla Morrowind *does not render bump or normal maps at all*, even though the
NIF has a slot for them. MGE-XE and MCP add the capability by repurposing the
**environment map** slot, and NifSkope follows that convention -- so a NIF
carrying MGE-convention data loaded into OpenMW without conversion renders
metallic and wrong. A texture in the bump slot is therefore recorded as
:attr:`TextureRole.BUMP`, not as a normal map: what it means depends on which
engine and which toolchain produced the file, and this module will not guess.

**Why this matters to a conflict report.** Two mods overwriting the same
``_n.dds`` are both shipping normal maps, so comparing them is exactly right
and the difference is meaningful. One mod shipping ``tx_rock.dds`` while
another ships ``tx_rock_n.dds`` is not a conflict at all -- they are
complementary channels of one material. Getting that wrong turns a
collaboration into a reported clash.
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
    """Base colour and transparency: the one a human can look at and judge."""

    DARK = "dark"
    """Multiplied into the base colour *after* the detail map."""

    DETAIL = "detail"
    """Multiplied into the base colour before the dark map, zoomed 2x."""

    GLOSS = "gloss"
    """Vanilla specular mask; brighter means shinier. A single channel."""

    GLOW = "glow"
    """Emissive: added to pixel brightness after lighting. OSG ``emissiveMap``."""

    NORMAL = "normal"
    """Tangent-space normals in RGB. OpenMW ``_n``, OSG ``normalMap``.

    Alpha may carry height for parallax even without the ``_nh`` name, so a
    normal map is not necessarily three-channel.
    """

    NORMAL_HEIGHT = "normal-height"
    """Normals with height in alpha, for parallax. ``_nh``, ``normalHeightMap``.

    A two-channel compressed format cannot be one of these: BC5 has no alpha,
    so it has nowhere to put the height field.
    """

    SPECULAR = "specular"
    """Specular **colour** in RGB with shininess in alpha. ``_spec``.

    Not a greyscale mask -- that is :attr:`GLOSS`. The distinction matters for
    display: this one is meant to be seen as colour.
    """

    DIFFUSE_SPECULAR = "diffuse-specular"
    """A terrain layer with specular intensity packed into alpha. ``_diffusespec``."""

    BUMP = "bump"
    """The vanilla bump slot, whose meaning depends on the toolchain.

    Vanilla ignores it entirely. Under the MGE-XE convention the environment
    map slot carries normals instead, and NifSkope follows suit. Recorded as
    its own role rather than guessed into :attr:`NORMAL`.
    """

    ENVIRONMENT = "environment"
    """A spherical environment map -- or, under MGE, normals wearing its hat."""

    DECAL = "decal"
    """One of up to four overlay layers stamped onto the surface."""

    UNKNOWN = "unknown"
    """Nothing said what this is. Treat it as colour, and say that it guessed."""

    @property
    def is_normal_map(self) -> bool:
        """Whether this holds vectors rather than colour.

        A normal map must not be shown as a photograph, and a difference
        between two of them is a difference in modelled geometry rather than
        in appearance. :attr:`BUMP` is excluded deliberately: it *may* be a
        normal map, and acting on a maybe is what this module exists to avoid.
        """
        return self in (TextureRole.NORMAL, TextureRole.NORMAL_HEIGHT)

    @property
    def is_mask(self) -> bool:
        """Whether this is a single-meaning greyscale mask rather than colour.

        Only :attr:`GLOSS` qualifies. A specular map carries RGB colour plus
        an alpha shininess term, so it is not a mask however much the name
        suggests one.
        """
        return self is TextureRole.GLOSS

    @property
    def is_colour(self) -> bool:
        """Whether the image is meant to be looked at as colour.

        Used to decide whether a side-by-side view is telling the user
        something or just showing them noise.
        """
        return not self.is_normal_map and not self.is_mask

    @property
    def carries_height(self) -> bool:
        """Whether alpha may hold a parallax height field.

        Worth knowing before reporting an alpha-channel difference: in these,
        a change in alpha is a change in modelled depth, not transparency.
        """
        return self in (TextureRole.NORMAL, TextureRole.NORMAL_HEIGHT)


#: The vanilla texture slots, in the order ``NiTexturingProperty`` declares
#: them, mapped onto what the engine does with each. Decal slots repeat past
#: the end -- a mesh may declare up to four -- so they are matched by prefix.
_SLOT_ROLES: Final[dict[str, TextureRole]] = {
    "base": TextureRole.DIFFUSE,
    "dark": TextureRole.DARK,
    "detail": TextureRole.DETAIL,
    "gloss": TextureRole.GLOSS,
    "glow": TextureRole.GLOW,
    "bump": TextureRole.BUMP,
    "env": TextureRole.ENVIRONMENT,
    "environment": TextureRole.ENVIRONMENT,
}

#: The names OpenMW recognises on an ``osg::Texture2D`` in a native mesh.
#: Unlike the other two conventions this one is explicit, so it is trusted
#: ahead of both.
_OSG_ROLES: Final[dict[str, TextureRole]] = {
    "base texture": TextureRole.DIFFUSE,
    "basetexture": TextureRole.DIFFUSE,
    "normalmap": TextureRole.NORMAL,
    "normalheightmap": TextureRole.NORMAL_HEIGHT,
    "emissivemap": TextureRole.GLOW,
    "darkmap": TextureRole.DARK,
    "detailmap": TextureRole.DETAIL,
    "envmap": TextureRole.ENVIRONMENT,
    "specularmap": TextureRole.SPECULAR,
}

#: OpenMW's file-name suffixes, **longest first**. The order is load-bearing:
#: ``_diffusespec`` also ends in ``spec``, and matching the shorter one first
#: would file every terrain layer as a plain specular map.
#:
#: These are the stock patterns. They are configurable per install, which is
#: why :func:`role_from_name` takes an override.
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
        The role, or :attr:`TextureRole.UNKNOWN` for an unrecognised name.
    """
    cleaned = slot.strip().lower()
    if cleaned.startswith("decal"):
        return TextureRole.DECAL
    return _SLOT_ROLES.get(cleaned, TextureRole.UNKNOWN)


def role_from_osg(name: str) -> TextureRole:
    """Say what an OSG texture unit name means.

    Args:
        name: The ``osg::Texture2D`` name from a ``.osgt`` or ``.osgb`` mesh.

    Returns:
        The role, or :attr:`TextureRole.UNKNOWN`.
    """
    cleaned = name.strip().lower().replace("_", "").replace(" ", "")
    return _OSG_ROLES.get(cleaned, _OSG_ROLES.get(name.strip().lower(), TextureRole.UNKNOWN))


def role_from_name(
    reference: str, patterns: tuple[tuple[str, TextureRole], ...] | None = None
) -> TextureRole:
    """Infer a role from a file name, the way OpenMW's shaders do.

    Args:
        reference: A texture path or file name, in any case, either separator.
        patterns: Suffix rules to use instead of the stock ones, longest
            first. OpenMW reads these from ``settings.cfg``, so an install can
            legitimately use different ones.

    Returns:
        The role the suffix implies, or :attr:`TextureRole.UNKNOWN` when the
        name carries none -- which is the common case, and means "no opinion"
        rather than "this is colour".
    """
    stem = reference.strip().replace("\\", "/").rsplit("/", 1)[-1].lower()
    if "." in stem:
        stem = stem.rsplit(".", 1)[0]
    for suffix, role in patterns or _NAME_SUFFIXES:
        if stem.endswith(suffix.lower()):
            return role
    return TextureRole.UNKNOWN


def classify(reference: str, slot: str = "", osg_name: str = "") -> TextureRole:
    """Decide a texture's role from everything available.

    The order is by how much each source actually knows:

    1. **An OSG unit name**, which states the role outright.
    2. **A vanilla slot**, which is what the engine honours for a NIF -- except
       that a slot of ``base`` loses to a normal-map suffix, because that
       combination means a pack has dropped an OpenMW-style map into a mesh
       written before the convention existed, and calling it diffuse would
       show a normal map as though it were a photograph.
    3. **The file name**, which is a convention rather than a declaration but
       is the only thing a loose texture has.

    Args:
        reference: The texture path as the mesh or the file system gives it.
        slot: The vanilla slot it hangs off, when it came from a NIF.
        osg_name: The texture unit name, when it came from an OSG mesh.

    Returns:
        The role.
    """
    if osg_name:
        from_osg = role_from_osg(osg_name)
        if from_osg is not TextureRole.UNKNOWN:
            return from_osg
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
    amount to reporting that red is not blue.

    :attr:`TextureRole.BUMP` and :attr:`TextureRole.NORMAL` are treated as
    comparable, because under the MGE convention they frequently are the same
    thing under two names -- and a false "not comparable" silently drops a
    real conflict, which is the worse failure of the two.

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
    normal_family = {TextureRole.NORMAL, TextureRole.NORMAL_HEIGHT, TextureRole.BUMP}
    if first in normal_family and second in normal_family:
        return True
    return first is second
