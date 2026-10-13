"""Generate ``viewer-shell/gardenfell-g7/src/meshes_gen.rs``: G7's NIF types into Robin's.

Gardenfell's ``netimmerse`` types are Greatness7's ``tes3::nif`` layouts ported to
std-only Rust, so the two models match field for field and only the spelling differs
(glam types against arrays, named enums against numbers, slot-map keys against block
indexes). This reads both crates' type definitions and writes one ``Conv`` impl per
struct and data-carrying enum reachable from a block type both crates read, plus the
``block`` function that converts one block. The spelling differences themselves are
hand-written in ``gardenfell-g7/src/meshes.rs``.

Run after a G7 version bump or a new Gardenfell copy::

    python tools/gen_g7_nif.py <G7 checkout>/libs/nif/src

(formatted with rustfmt when it is on PATH), then build ``gardenfell-g7`` and run its
``differential_nif`` test. The run reports how many fields and variants it could not
match ("NO G7 FIELD" / "NO ROBIN VARIANT" in the output, exit status 1): those need a hand-written impl in ``meshes.rs`` and a name in
``_HAND``.
"""

from __future__ import annotations

import re
import shutil
import subprocess
import sys
from pathlib import Path

_REPO = Path(__file__).resolve().parent.parent
_ROBIN = _REPO / "viewer-shell/gardenfell/crates/gardenfell-nif/src/netimmerse/types"
_OUT = _REPO / "viewer-shell/gardenfell-g7/src/meshes_gen.rs"

#: Types converted by hand in meshes.rs (their shapes differ).
_HAND = frozenset({"NiBoundingVolume"})

#: Wrapper and built-in names that are never one of the crates' own types.
_BUILTIN = frozenset({"Vec", "Option", "Link", "String", "Box", "Self"})

Kinds = dict[str, tuple[str, object]]


def parse(folder: Path) -> Kinds:
    """Read every struct and enum definition in a crate's type sources.

    Args:
        folder: A folder of ``.rs`` files (searched recursively).

    Returns:
        Type name -> ``("struct", [(field, type)])``, ``("tuple", fields)``,
        ``("enum", [(variant, payload, discriminant)])`` or ``("flags", int type)``.
    """
    out: Kinds = {}
    for f in sorted(folder.rglob("*.rs")):
        src = f.read_text(encoding="utf-8")
        for m in re.finditer(r"pub struct (\w+)(<[^>]*>)?\s*\{(.*?)\n\}", src, re.S):
            fields = re.findall(r"^\s*pub (\w+):\s*(.+?),\s*(?://.*)?$", m.group(3), re.M)
            out[m.group(1)] = ("struct", fields)
        for m in re.finditer(r"pub struct (\w+)\(([^)]*)\);", src):
            out[m.group(1)] = ("tuple", m.group(2))
        for m in re.finditer(r"pub enum (\w+)\s*\{(.*?)\n\}", src, re.S):
            body = re.sub(r"//[^\n]*", "", m.group(2))
            body = re.sub(r"#\[[^\]]*\]", "", body)
            variants = re.findall(r"^\s*(\w+)\s*(\([^)]*\)|\{[^}]*\})?\s*(=\s*[^,]+)?,", body, re.M)
            out[m.group(1)] = ("enum", variants)
        for m in re.finditer(r"pub struct (\w+):\s*(u\d+)", src):  # bitflags
            out[m.group(1)] = ("flags", m.group(2))
    return out


def top_level_count(payload: str) -> int:
    """Count a tuple variant's fields (commas outside brackets).

    Args:
        payload: The text between the variant's parentheses.

    Returns:
        The number of fields; 0 for an empty payload.
    """
    if not payload.strip():
        return 0
    depth, n = 0, 1
    for c in payload:
        if c in "<([":
            depth += 1
        elif c in ">)]":
            depth -= 1
        elif c == "," and depth == 0:
            n += 1
    return n


def type_names(text: str) -> list[str]:
    """List the crate type names a field type mentions.

    Args:
        text: A Rust type, e.g. ``Vec<Link<NiNode>>``.

    Returns:
        The capitalised names in it, wrappers and built-ins left out.
    """
    return [x for x in re.findall(r"\b([A-Z]\w+)\b", text) if x not in _BUILTIN]


def block_types(robin_mod: str, g7_types: str) -> list[str]:
    """List the block types both crates read.

    Args:
        robin_mod: Robin's ``types/mod.rs`` (its ``ni_types!`` list).
        g7_types: G7's ``types.rs`` (its ``NiType`` enum).

    Returns:
        The names in both, sorted.
    """
    r_list = robin_mod[robin_mod.index("ni_types! {") :].split("}", maxsplit=1)[0]
    robin = set(re.findall(r"\b(\w+)\b", r_list)) - {"ni_types"}
    g_enum = g7_types[g7_types.index("pub enum NiType") :].split("\n}", maxsplit=1)[0]
    g7 = set(re.findall(r"^\s+(\w+)\(\1\),", g_enum, re.M))
    print(f"block types: Robin {len(robin)}, G7 {len(g7)}, both {len(robin & g7)}")
    if robin ^ g7:
        print(f"  Robin only: {sorted(robin - g7)}; G7 only: {sorted(g7 - robin)}")
    return sorted(robin & g7)


class _Gen:
    """The walk from the block types through every type their fields use."""

    def __init__(self, robin: Kinds, g7: Kinds) -> None:
        """Start an empty walk.

        Args:
            robin: Robin's types (``parse``).
            g7: G7's types (``parse``).
        """
        self.robin, self.g7 = robin, g7
        self.done: set[str] = set()
        self.structs: list[tuple[str, str, list[str]]] = []
        self.enums: list[tuple[str, str, list[str]]] = []
        self.repr_enums: set[str] = set()
        self.unmatched = 0

    def g7_name(self, name: str) -> str | None:
        """Find G7's spelling of one of Robin's type names.

        Args:
            name: Robin's type name.

        Returns:
            G7's name for it, or None.
        """
        if name in self.g7:
            return name
        return next((k for k in self.g7 if k.lower() == name.lower()), None)

    def walk(self, start: list[str]) -> None:
        """Convert every type reachable from ``start``.

        Args:
            start: Robin's names of the block types.
        """
        todo = list(start)
        while todo:
            rn = todo.pop()
            if rn in self.done or rn in _HAND:
                continue
            self.done.add(rn)
            gn = self.g7_name(rn)
            r, g = self.robin.get(rn), self.g7.get(gn) if gn else None
            if r is None or g is None or gn is None:
                continue
            if r[0] == "struct" and g[0] == "struct":
                todo += self.struct(rn, gn, r[1], g[1])  # type: ignore[arg-type]
            elif r[0] == "enum" and g[0] == "enum":
                todo += self.enum(rn, gn, r[1], g[1])  # type: ignore[arg-type]

    def struct(
        self, rn: str, gn: str, r_fields: list[tuple[str, str]], g_fields: list[tuple[str, str]]
    ) -> list[str]:
        """Write one struct's conversion, field by field.

        Args:
            rn: Robin's name.
            gn: G7's name.
            r_fields: Robin's fields.
            g_fields: G7's fields.

        Returns:
            The types its fields use, to convert next.
        """
        g_by_name = dict(g_fields)
        lines, more = [], []
        for name, ty in r_fields:
            if name not in g_by_name:
                lines.append(f"            {name}: Default::default(), // NO G7 FIELD")
                self.unmatched += 1
                continue
            lines.append(f"            {name}: self.{name}.conv(cx),")
            more += type_names(ty)
            for inner in type_names(g_by_name[name]):
                # G7's named enum where Robin keeps a number.
                if self.g7.get(inner, ("",))[0] == "enum" and (
                    inner not in self.robin or self.robin[inner][0] != "enum"
                ):
                    self.repr_enums.add(inner)
        self.structs.append((gn, rn, lines))
        return more

    def enum(
        self,
        rn: str,
        gn: str,
        r_vars: list[tuple[str, str, str]],
        g_vars: list[tuple[str, str, str]],
    ) -> list[str]:
        """Write one enum's conversion, variant by variant.

        Args:
            rn: Robin's name.
            gn: G7's name.
            r_vars: Robin's variants.
            g_vars: G7's variants.

        Returns:
            The types its variants carry, to convert next.
        """
        if not any(v[1] for v in g_vars):
            self.repr_enums.add(gn)  # fieldless: by number
            return []
        by_name = {v[0]: v for v in r_vars}
        arms, more = [], []
        for name, payload, _ in g_vars:
            low = name.lower()
            match = [
                k for k in by_name if low in (k.lower(), k.lower() + "key", "ni" + k.lower() + "bv")
            ]
            if not match:
                arms.append(
                    f"            g::{gn}::{name}(..) => Default::default(), // NO ROBIN VARIANT"
                )
                self.unmatched += 1
                continue
            rk = match[0]
            n = top_level_count((payload or "").strip()[1:-1])
            xs = ", ".join(f"x{i}" for i in range(n))
            cs = ", ".join(f"x{i}.conv(cx)" for i in range(n))
            arms.append(f"            g::{gn}::{name}({xs}) => r::{rn}::{rk}({cs}),")
            more += type_names(by_name[rk][1] or "")
        self.enums.append((gn, rn, arms))
        return more


def render(gen: _Gen, blocks: list[str]) -> str:
    """Write the Rust source.

    Args:
        gen: The finished walk.
        blocks: The block types converted.

    Returns:
        meshes_gen.rs.
    """
    code = [
        "// Generated by tools/gen_g7_nif.py from both crates' type definitions; edit the",
        "// generator, not this file. G7's nif types into Robin's, field by field.",
        "",
        "use super::{Conv, Cx};",
        "use gardenfell_nif::netimmerse::types as r;",
        "use tes3::nif as g;",
        "",
    ]
    for gn, rn, lines in gen.structs:
        code += [f"impl Conv<r::{rn}> for g::{gn} {{", f"    fn conv(&self, cx: &Cx) -> r::{rn} {{"]
        code += [f"        r::{rn} {{", *lines, "        }", "    }", "}", ""]
    for gn, rn, arms in gen.enums:
        code += [f"impl Conv<r::{rn}> for g::{gn} {{", f"    fn conv(&self, cx: &Cx) -> r::{rn} {{"]
        code += ["        match self {", *arms, "        }", "    }", "}", ""]
    code += [
        "crate::meshes::repr_enums! {",
        *(f"    {e}," for e in sorted(gen.repr_enums)),
        "}",
        "",
    ]
    code += [
        "/// One block, when its type is one this converts.",
        "pub fn block(t: &g::NiType, cx: &Cx) -> Option<r::NiType> {",
        "    Some(match t {",
        *(f"        g::NiType::{b}(x) => r::NiType::{b}(x.conv(cx))," for b in blocks),
        "        _ => return None,",
        "    })",
        "}",
    ]
    return "\n".join(code) + "\n"


def main() -> int:
    """Generate meshes_gen.rs from G7's sources given on the command line.

    Returns:
        0, or 1 when something could not be matched (the file is still written).
    """
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    g7_src = Path(sys.argv[1])
    robin, g7 = parse(_ROBIN), parse(g7_src)
    blocks = block_types(
        (_ROBIN / "mod.rs").read_text(encoding="utf-8"),
        (g7_src / "types.rs").read_text(encoding="utf-8"),
    )
    # A block both read whose shape is a struct on both sides (NiObject, the abstract
    # base, is left to the fallback).
    blocks = [b for b in blocks if robin.get(b, ("",))[0] == "struct" == g7.get(b, ("",))[0]]
    gen = _Gen(robin, g7)
    gen.walk(blocks)
    _OUT.write_text(render(gen, blocks), encoding="utf-8", newline="\n")
    rustfmt = shutil.which("rustfmt")
    if rustfmt:  # the crate's own formatting; without rustfmt the file still builds
        subprocess.run([rustfmt, "--edition", "2021", str(_OUT)], check=True)  # noqa: S603
    print(
        f"{len(gen.structs)} structs, {len(gen.enums)} enums, {len(gen.repr_enums)} by number, "
        f"{len(blocks)} blocks; {gen.unmatched} unmatched -> {_OUT.relative_to(_REPO)}"
    )
    return 1 if gen.unmatched else 0


if __name__ == "__main__":
    raise SystemExit(main())
