"""Regenerate ``mlox_subset/tes3fields/schema.py`` from the UESP format tables.

The input is a CSV export of UESP's *Morrowind Mod:Mod File Format* pages -- the
community's reference for the TES3 record layout, which documents every record
type, each of its subrecords, and the byte layout of the struct subrecords.

**This is documentation, not code.** UESP's text is CC-BY-SA; what we take from
it are format facts (a ``NPDT`` is 12 or 52 bytes; the first two are a uint16
Level) which are descriptions of Bethesda's file format rather than anyone's
creative expression, and the generated module credits the source. No source code
from any implementation is read or copied -- that policy is the whole reason
this file exists rather than a port of somebody's header.

What the schema buys: the field-diff window currently shows tes3conv's key names
and raw values. With this it can say what a field *is*, how wide it should be,
whether it is required, and -- for struct subrecords -- split a blob into named
members, so a diff reads "Gold: 100 -> 250" instead of two base64 strings.

Usage:
    python tools/gen_tes3_schema.py tes3filetype.csv
"""

from __future__ import annotations

import csv
import re
import sys
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "mlox_subset/tes3fields/schema.py"

#: The row that introduces each record type's subrecord table.
_TABLE_HEADER = ("C", "Field", "Type/Size", "Info")

#: ``Morrowind Mod:Mod File Format/LAND`` -- the page each table came from. The
#: trailing segment is the record type, except for the handful of shared
#: subpages (``AI Package Fields``) which are not record types at all.
_SECTION = re.compile(r"^Morrowind Mod:Mod File Format/(?P<name>.+)$")

#: ``uint8[3]``, ``uint16[16][16]``, ``char[32]`` -- a type with array extents.
_TYPED = re.compile(r"^(?P<base>[A-Za-z_][\w]*)(?P<dims>(?:\[\d+\])*)$")

#: One line of a struct layout: ``uint16 - Level``. ``=`` is accepted as well
#: as ``-`` because the tables contain at least one typo of that shape
#: (``CLAS``'s ``uint32 = Flags``), and dropping it silently cost four bytes out
#: of a sixty-byte struct. Bit-value lines (``0x01 = Interior``) cannot be
#: confused with it: their left side is not a type name, which is checked.
_MEMBER = re.compile(r"^(?P<type>[A-Za-z_][\w]*(?:\[\d+\])*)\s*[-=]\s*(?P<name>.+)$")

#: A subrecord tag: short, upper case, digits and underscores allowed.
_TAG = re.compile(r"[A-Z0-9_]{2,4}")

#: ``(12 or 52 bytes)``, ``(8 bytes)``, ``(12,675 bytes)`` -- the declared size
#: under a ``struct`` type.
_SIZE = re.compile(r"\(([^)]*bytes?[^)]*)\)")

#: Byte widths for the scalar types the format uses. Sizes the tables state
#: themselves; ``zstring``/``string`` are variable and deliberately absent.
_WIDTHS: dict[str, int] = {
    "int8": 1,
    "uint8": 1,
    "char": 1,
    "int16": 2,
    "uint16": 2,
    "int32": 4,
    "uint32": 4,
    "float32": 4,
    #: The tables say ``float`` in a few places where they mean ``float32``.
    #: Without this the member is dropped and the struct comes up short.
    "float": 4,
    "rgb": 4,
}


def _clean(text: str) -> str:
    """Normalise whitespace, including the non-breaking spaces the wiki uses.

    Args:
        text: Raw cell text.

    Returns:
        The text with NBSPs turned into spaces and edges stripped.
    """
    return (
        text.replace("\xa0", " ")
        .replace("\u2013", "-")  # en dash
        .replace("\u00d7", "x")  # multiplication sign, as in "65x65"
        .strip()
    )


def parse_type(text: str) -> tuple[str, tuple[int, ...]]:
    """Split a type string into its base type and array extents.

    Args:
        text: e.g. ``"uint16[16][16]"``.

    Returns:
        ``("uint16", (16, 16))``. An unparseable type comes back with no
        extents rather than raising -- the table is prose written by people.
    """
    match = _TYPED.match(text.strip())
    if match is None:
        return text.strip(), ()
    dims = tuple(int(n) for n in re.findall(r"\[(\d+)\]", match.group("dims")))
    return match.group("base"), dims


def member_size(base: str, dims: tuple[int, ...]) -> int:
    """Compute a struct member's width in bytes.

    Args:
        base: The scalar type name.
        dims: Array extents.

    Returns:
        The width, or ``0`` when the type is variable-length or unknown -- which
        the consumer must treat as "stop decoding here" rather than as zero.
    """
    width = _WIDTHS.get(base.lower())
    if width is None:
        return 0
    for dim in dims:
        width *= dim
    return width


def parse_layout(info: str) -> tuple[list[tuple[str, tuple[int, ...], str]], list[str]]:
    """Extract a struct's members from a field's Info cell.

    The cell is a description line followed by ``type - name`` lines. A few
    fields document two layouts under headings (``NPDT`` is 12 *or* 52 bytes
    depending on a flag); those are reported as variant headings rather than
    concatenated into one nonsense struct, because a decoder that runs the two
    together would silently mis-read every NPC in the game.

    Args:
        info: The Info cell's full text.

    Returns:
        The members as ``(base_type, dims, name)``, and any variant headings
        found. A field with variants yields **no** members: which layout applies
        is a runtime question this table cannot answer.
    """
    members: list[tuple[str, tuple[int, ...], str]] = []
    variants: list[str] = []
    lines = info.splitlines()
    # The first line is normally a description ("Alchemy data"), but not always:
    # CELL's DATA opens straight onto ``uint32 - Flags``. Skipping it blindly
    # cost that field its first member, so it is skipped only when it is not
    # itself a member line.
    if lines and not _MEMBER.match(_clean(lines[0])):
        lines = lines[1:]
    for raw in lines:
        line = _clean(raw)
        if not line:
            continue
        if re.match(r"^\d[\d,]*-byte version", line, re.IGNORECASE):
            variants.append(line)
            continue
        match = _MEMBER.match(line)
        if match is None:
            continue
        base, dims = parse_type(match.group("type"))
        if base.lower() not in _WIDTHS and not dims:
            continue  # prose that happened to contain a dash
        members.append((base, dims, _clean(match.group("name"))))
    if variants:
        return [], variants
    return members, variants


def parse_csv(path: Path) -> dict[str, dict[str, object]]:
    """Read the export into ``{section: {description, fields}}``.

    Args:
        path: The CSV export.

    Returns:
        One entry per documented section, in file order.

    Raises:
        ValueError: If no sections were found at all, which means the export is
            not what this script expects and a silent empty schema would be far
            worse than a stop.
    """
    with path.open(encoding="utf-8", errors="replace", newline="") as handle:
        rows = [[_clean(cell) for cell in row] for row in csv.reader(handle)]

    sections: dict[str, dict[str, object]] = {}
    current: str | None = None
    in_table = False
    for index, row in enumerate(rows):
        first = row[0] if row else ""
        section = _SECTION.match(first)
        if section:
            current = section.group("name")
            in_table = False
            description = ""
            for follower in rows[index + 1 : index + 4]:
                text = follower[0] if follower else ""
                if text and not text.startswith("The UESPWiki"):
                    description = text
                    break
            sections.setdefault(current, {"description": description, "fields": []})
            continue
        if tuple(row[:4]) == _TABLE_HEADER:
            in_table = bool(current)
            continue
        if not in_table or current is None or len(row) < 4:
            continue
        cardinality, name, type_text, info = row[0], row[1], row[2], row[3]
        if not name:
            # A full-width note between the header and the fields ("AI Packages
            # - the following fields can appear in any order"). Treating this as
            # the end of the table cost the AI package fields and every INFO
            # field their entire entry, silently -- so a note is skipped and the
            # table continues. Only a new section ends a table.
            continue
        if not _TAG.fullmatch(name):
            # Several pages carry a second table after the subrecord one -- the
            # magic-effect list on MGEF, the GMST value list -- and its rows are
            # four columns wide too. A subrecord tag is short and upper case
            # (NAME, AI_A, NAM5); "Jump" is not, and letting it through put
            # rows like Subrecord(name="Jump", cardinality="9") in the schema.
            continue
        size = _SIZE.search(type_text)
        base_type = _clean(type_text.split("\n")[0])
        members, variants = parse_layout(info)
        declared = _declared_bytes(_clean(size.group(1)) if size else "")
        parsed = sum(member_size(base, dims) for base, dims, _n in members)
        repeat = 1
        if members and parsed and declared and declared > parsed and declared % parsed == 0:
            # An array of the documented element, as LAND's VNML is: "a 65x65
            # array of: int8 X, int8 Y, int8 Z". The layout describes one
            # element and the declared size covers all of them.
            repeat = declared // parsed
        fields = sections[current]["fields"]
        assert isinstance(fields, list)  # noqa: S101 - built as a list above
        fields.append(
            {
                "name": name,
                "cardinality": cardinality,
                "type": base_type,
                "size": _clean(size.group(1)) if size else "",
                "description": _description(info),
                "members": members,
                "variants": variants,
                "repeat": repeat,
            }
        )
    if not sections:
        message = "no sections found -- is that really the format-pages export?"
        raise ValueError(message)
    return sections


def _description(info: str) -> str:
    """Take a field's one-line description out of its Info cell.

    Args:
        info: The Info cell's full text.

    Returns:
        The first line, or an empty string when that line is the first member
        of a struct rather than a description -- ``CELL``'s ``DATA`` opens on
        ``uint32 - Flags``, and reporting that as the field's description
        ("DATA - uint32 - Flags") reads like a mistake because it is one.
    """
    first = _clean(info.split("\n", 1)[0])
    return "" if _MEMBER.match(first) else first


def _declared_bytes(size_text: str) -> int:
    """Read a plain byte count out of a declared size.

    Args:
        size_text: e.g. ``"12 bytes"``, ``"12,675 bytes"``, ``"12 or 52 bytes"``.

    Returns:
        The count, or ``0`` when the size is conditional or absent -- a hedged
        size must not be turned into a number here.
    """
    match = re.fullmatch(r"([\d,]+) bytes?", size_text.strip())
    return int(match.group(1).replace(",", "")) if match else 0


def _quote(text: str) -> str:
    """Render a string as a Python literal.

    Args:
        text: Any text from the tables.

    Returns:
        A double-quoted literal, escaped.
    """
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def emit(sections: dict[str, dict[str, object]]) -> str:
    """Render the generated module.

    Args:
        sections: The parsed tables.

    Returns:
        The module source.
    """
    lines = [
        '"""The TES3 record and subrecord schema.',
        "",
        "GENERATED FILE -- do not edit by hand. See ``tools/gen_tes3_schema.py``.",
        "",
        "Source: UESP's *Morrowind Mod:Mod File Format* pages, the community's",
        "reference for the TES3 binary layout (CC-BY-SA). What is taken from them",
        "are format facts -- field names, widths, and whether a subrecord is",
        "required -- which describe Bethesda's file format rather than anyone's",
        "implementation of it. No source code from any project was read or copied;",
        "see ``CREDITS.md``.",
        "",
        "The tables are prose written by people, so this is a *best-effort* schema:",
        "a field with no parsed members is a field whose layout could not be read",
        "mechanically, not a field with no layout. Consumers must treat a missing",
        "or zero size as 'stop decoding' rather than as zero bytes.",
        '"""',
        "",
        "from __future__ import annotations",
        "",
        "from typing import Final",
        "",
        "from mlox_subset.tes3fields.schema_types import Member, Record, Subrecord",
        "",
        "#: Record type (or shared subpage name) -> its documented layout.",
        "RECORDS: Final[dict[str, Record]] = {",
    ]
    for name, section in sections.items():
        fields = section["fields"]
        assert isinstance(fields, list)  # noqa: S101 - built as a list above
        if not fields:
            continue
        lines.append(f"    {_quote(name)}: Record(")
        lines.append(f"        name={_quote(name)},")
        lines.append(f"        description={_quote(str(section['description']))},")
        lines.append("        fields=(")
        for field in fields:
            lines.append("            Subrecord(")
            lines.append(f"                name={_quote(str(field['name']))},")
            lines.append(f"                cardinality={_quote(str(field['cardinality']))},")
            lines.append(f"                type={_quote(str(field['type']))},")
            lines.append(f"                size={_quote(str(field['size']))},")
            lines.append(f"                description={_quote(str(field['description']))},")
            members = field["members"]
            assert isinstance(members, list)  # noqa: S101 - built as a list above
            if members:
                lines.append("                members=(")
                for base, dims, member_name in members:
                    lines.append(
                        f"                    Member({_quote(base)}, {dims!r}, "
                        f"{_quote(member_name)}, {member_size(base, dims)}),"
                    )
                lines.append("                ),")
            if field["repeat"] != 1:
                lines.append(f"                repeat={field['repeat']!r},")
            variants = field["variants"]
            assert isinstance(variants, list)  # noqa: S101 - built as a list above
            if variants:
                rendered = ", ".join(_quote(v) for v in variants)
                lines.append(f"                variants=({rendered},),")
            lines.append("            ),")
        lines.append("        ),")
        lines.append("    ),")
    lines += ["}", ""]
    return "\n".join(lines)


def main(argv: list[str]) -> int:
    """Write the generated module.

    Args:
        argv: Process arguments; one CSV path is expected.

    Returns:
        A process exit code.
    """
    if len(argv) != 2:
        print(__doc__)
        return 2
    sections = parse_csv(Path(argv[1]))
    OUT.write_text(emit(sections), encoding="utf-8")
    records = sum(1 for s in sections.values() if s["fields"])
    fields = sum(len(s["fields"]) for s in sections.values())  # type: ignore[arg-type]
    structs = sum(
        1
        for s in sections.values()
        for f in s["fields"]  # type: ignore[union-attr]
        if f["members"]
    )
    print(f"wrote {records} records, {fields} subrecords ({structs} with layouts) -> {OUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
