#!/usr/bin/env python3
"""
ast_mermaid.py — generate Mermaid flowcharts from Python source using the `ast` module.

Four diagram types, all emitted as Mermaid `flowchart` blocks in a single Markdown report
(or to stdout):

  deps     Module import graph (which files import which), grouped by package.
  classes  Class inheritance graph, grouped by module.
  calls    Function/method call graph. One diagram per file by default (a project-wide
           call graph for a large codebase is an unreadable hairball); use --combine-calls
           to force a single merged diagram.
  cfg      A real control-flow flowchart (branches, loops, return/break/continue, try/except)
           for ONE function you name explicitly, e.g. --cfg path/to/file.py:ClassName.method

Everything here is best-effort static analysis. `deps` resolves absolute and relative
imports against the files actually in scope; anything it can't resolve is either dropped
or (with --include-external) shown as a dashed external node. `calls` only tracks calls to
bare names (module-level/imported functions) and self.*/cls.* method calls — attribute
calls on other objects (self.widget.pack(), os.path.join(), ...) are too dynamic to resolve
statically and are skipped to keep the diagrams readable. `cfg`'s try/except handling is an
approximation: it draws a dashed "on exception" edge from the top of the try block to each
handler rather than modeling exactly which statement could raise.

No third-party dependencies — stdlib only.

Usage:
    python ast_mermaid.py wraithguard/ --graphs deps,classes
    python ast_mermaid.py wraithguard/gui/ --graphs calls --min-funcs 3
    python ast_mermaid.py wraithguard/ --list-functions | grep conflicts
    python ast_mermaid.py wraithguard/gui/conflicts.py --cfg wraithguard/gui/conflicts.py:ConflictWindow.compare
    python ast_mermaid.py wraithguard/ --graphs all -o report.md
"""

from __future__ import annotations

import argparse
import ast
import builtins
import fnmatch
import math
import re
import sys
from collections import defaultdict
from pathlib import Path

BUILTIN_NAMES = set(dir(builtins))
# Extremely common no-signal calls: gettext-style i18n wrappers wrap nearly every user-facing
# string, so `_("...")` can appear dozens of times per file. Treated like a builtin — real
# calls, but pure noise in an architecture-level call graph.
NOISE_CALL_NAMES = {"_", "gettext", "ngettext", "pgettext", "npgettext"}


# --------------------------------------------------------------------------- #
# Small shared helpers
# --------------------------------------------------------------------------- #

def sid(name: str) -> str:
    """Sanitize an arbitrary string into a valid, unique-enough Mermaid node id."""
    return "n_" + re.sub(r"[^0-9a-zA-Z_]", "_", name)


def esc(text) -> str:
    """Escape text for safe use inside a quoted Mermaid label."""
    if text is None:
        return ""
    text = str(text)
    text = text.replace("\\", "\\\\")
    text = text.replace('"', "#quot;")
    text = text.replace("\n", "<br/>")
    return text


def truncate(text: str, n: int = 60) -> str:
    text = text.strip()
    return text if len(text) <= n else text[: n - 1] + "…"


def safe_parse(path: Path):
    try:
        return ast.parse(path.read_text(encoding="utf-8", errors="replace"), filename=str(path))
    except SyntaxError as e:
        print(f"warning: skipping {path} (SyntaxError: {e})", file=sys.stderr)
        return None


def node_text(node) -> str:
    try:
        return ast.unparse(node)
    except Exception:
        return type(node).__name__


def stmt_preview(stmt) -> str:
    """Like node_text, but nested defs show only their signature — their body is a
    separate scope, not part of this function's control flow."""
    if isinstance(stmt, (ast.FunctionDef, ast.AsyncFunctionDef)):
        prefix = "async def" if isinstance(stmt, ast.AsyncFunctionDef) else "def"
        try:
            args = ast.unparse(stmt.args)
        except Exception:
            args = "..."
        return f"{prefix} {stmt.name}({args}): ..."
    if isinstance(stmt, ast.ClassDef):
        return f"class {stmt.name}: ..."
    return node_text(stmt)


# --------------------------------------------------------------------------- #
# File discovery / module naming
# --------------------------------------------------------------------------- #

def discover(path: Path, excludes: list[str]):
    if path.is_file():
        return [path], path.parent
    if not path.is_dir():
        raise SystemExit(f"No such file or directory: {path}")

    scan_root = path.parent if (path / "__init__.py").exists() else path
    files = []
    for f in sorted(path.rglob("*.py")):
        if "__pycache__" in f.parts:
            continue
        rel = str(f.relative_to(path))
        if any(fnmatch.fnmatch(rel, pat) or fnmatch.fnmatch(f.name, pat) for pat in excludes):
            continue
        files.append(f)
    return files, scan_root


def module_name_for(f: Path, scan_root: Path) -> str:
    rel = f.relative_to(scan_root)
    parts = list(rel.with_suffix("").parts)
    if parts and parts[-1] == "__init__":
        parts = parts[:-1]
    return ".".join(parts) if parts else f.stem


def index_modules(files: list[Path], scan_root: Path):
    """dotted module name -> file, and dotted module name -> its containing package
    (used as the base for resolving relative imports)."""
    mod_to_file, pkg_of = {}, {}
    for f in files:
        mod = module_name_for(f, scan_root)
        mod_to_file[mod] = f
        if f.name == "__init__.py":
            pkg_of[mod] = mod
        else:
            pkg_of[mod] = mod.rsplit(".", 1)[0] if "." in mod else ""
    return mod_to_file, pkg_of


# --------------------------------------------------------------------------- #
# deps: module import graph
# --------------------------------------------------------------------------- #

def resolve_absolute(name: str, known: set) -> str | None:
    if not name:
        return None
    parts = name.split(".")
    for i in range(len(parts), 0, -1):
        cand = ".".join(parts[:i])
        if cand in known:
            return cand
    return None


def resolve_relative_base(pkg: str, level: int) -> str:
    parts = pkg.split(".") if pkg else []
    up = level - 1
    if up > 0:
        parts = parts[:-up] if up <= len(parts) else []
    return ".".join(parts)


def build_deps_edges(files: list[Path], scan_root: Path, include_external: bool):
    mod_to_file, pkg_of = index_modules(files, scan_root)
    known = set(mod_to_file)
    edges = set()

    for mod, f in mod_to_file.items():
        tree = safe_parse(f)
        if tree is None:
            continue
        pkg = pkg_of[mod]
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                for alias in node.names:
                    tgt = resolve_absolute(alias.name, known)
                    if tgt and tgt != mod:
                        edges.add((mod, tgt))
                    elif include_external:
                        top = alias.name.split(".")[0]
                        edges.add((mod, "EXTERNAL:" + top))

            elif isinstance(node, ast.ImportFrom):
                if node.level == 0:
                    base = node.module or ""
                    tgt = resolve_absolute(base, known)
                    matched = False
                    if tgt and tgt != mod:
                        edges.add((mod, tgt))
                        matched = True
                    for alias in node.names:
                        cand = f"{base}.{alias.name}" if base else alias.name
                        if cand in known and cand != mod:
                            edges.add((mod, cand))
                            matched = True
                    if not matched and include_external and base:
                        edges.add((mod, "EXTERNAL:" + base.split(".")[0]))
                else:
                    base = resolve_relative_base(pkg, node.level)
                    full = f"{base}.{node.module}" if (base and node.module) else (node.module or base)
                    matched = False
                    if full in known and full != mod:
                        edges.add((mod, full))
                        matched = True
                    for alias in node.names:
                        cand = f"{full}.{alias.name}" if full else alias.name
                        if cand in known and cand != mod:
                            edges.add((mod, cand))
                            matched = True
                    if not matched and base in known and base != mod:
                        edges.add((mod, base))
    return mod_to_file, edges


def render_deps_mermaid(mod_to_file: dict, edges: set, direction: str) -> str:
    """Single combined diagram — everything, all packages, all files. Fine for small
    scopes; for a real project this is generally too big to read (or even to render —
    Mermaid has its own size ceiling). Used only for --combine-deps."""
    lines = [f"flowchart {direction}"]
    groups = defaultdict(list)
    for mod in mod_to_file:
        parent = mod.rsplit(".", 1)[0] if "." in mod else "(top level)"
        groups[parent].append(mod)

    for i, (parent, mods) in enumerate(sorted(groups.items())):
        lines.append(f'  subgraph SG{i}["{esc(parent)}"]')
        for mod in sorted(mods):
            leaf = mod.rsplit(".", 1)[-1]
            lines.append(f'    {sid(mod)}["{esc(leaf)}"]')
        lines.append("  end")

    ext_declared = set()
    for a, b in sorted(edges):
        if b.startswith("EXTERNAL:"):
            name = b[len("EXTERNAL:"):]
            if name not in ext_declared:
                lines.append(f'  {sid(b)}(("{esc(name)}")):::external')
                ext_declared.add(name)
            lines.append(f"  {sid(a)} -.-> {sid(b)}")
        else:
            lines.append(f"  {sid(a)} --> {sid(b)}")
    if ext_declared:
        lines.append("  classDef external fill:#eee,stroke:#999,stroke-dasharray: 3 3")
    return "\n".join(lines)


def package_of(mod: str) -> str:
    return mod.rsplit(".", 1)[0] if "." in mod else "(top level)"


def render_package_summary_mermaid(packages: set, pkg_edges: set, direction: str) -> str:
    """One node per top-level package, one edge per package-pair that has ANY import
    between them. This is the small, always-readable big-picture view."""
    lines = [f"flowchart {direction}"]
    for p in sorted(packages):
        lines.append(f'  {sid(p)}["{esc(p)}"]')
    ext_declared = set()
    for a, b in sorted(pkg_edges):
        if b.startswith("EXTERNAL:"):
            name = b[len("EXTERNAL:"):]
            if name not in ext_declared:
                lines.append(f'  {sid(b)}(("{esc(name)}")):::external')
                ext_declared.add(name)
            lines.append(f"  {sid(a)} -.-> {sid(b)}")
        else:
            lines.append(f"  {sid(a)} --> {sid(b)}")
    if ext_declared:
        lines.append("  classDef external fill:#eee,stroke:#999,stroke-dasharray: 3 3")
    return "\n".join(lines)


def render_deps_detail_mermaid(mods_in_pkg: set, edges_detail: set, direction: str) -> str:
    """One package's files in full detail; imports into other packages are collapsed to
    a single stub node per target package (not one node per foreign file), which is what
    keeps this bounded regardless of overall project size."""
    lines = [f"flowchart {direction}"]
    for mod in sorted(mods_in_pkg):
        leaf = mod.rsplit(".", 1)[-1]
        lines.append(f'  {sid(mod)}["{esc(leaf)}"]')

    declared = set()
    has_ext = has_pkg = False
    for a, b in sorted(edges_detail):
        if b.startswith("PKG:"):
            pkg = b[len("PKG:"):]
            if b not in declared:
                lines.append(f'  {sid(b)}[["{esc(pkg)}"]]:::pkglink')
                declared.add(b)
                has_pkg = True
            lines.append(f"  {sid(a)} --> {sid(b)}")
        elif b.startswith("EXTERNAL:"):
            name = b[len("EXTERNAL:"):]
            if b not in declared:
                lines.append(f'  {sid(b)}(("{esc(name)}")):::external')
                declared.add(b)
                has_ext = True
            lines.append(f"  {sid(a)} -.-> {sid(b)}")
        else:
            lines.append(f"  {sid(a)} --> {sid(b)}")
    if has_ext:
        lines.append("  classDef external fill:#eee,stroke:#999,stroke-dasharray: 3 3")
    if has_pkg:
        lines.append("  classDef pkglink fill:#dde,stroke:#668")
    return "\n".join(lines)


def build_deps_sections(files: list[Path], scan_root: Path, direction: str, include_external: bool):
    """Default deps output: a package-overview diagram plus one detail diagram per
    top-level package, instead of one giant everything-diagram."""
    mod_to_file, edges = build_deps_edges(files, scan_root, include_external)
    if len(mod_to_file) < 2:
        return []

    packages = {package_of(m) for m in mod_to_file}
    sections = []

    if len(packages) > 1:
        pkg_edges = set()
        for a, b in edges:
            pa = package_of(a)
            if b.startswith("EXTERNAL:"):
                if include_external:
                    pkg_edges.add((pa, b))
                continue
            pb = package_of(b)
            if pa != pb:
                pkg_edges.add((pa, pb))
        sections.append(("Package Dependencies (overview)",
                          render_package_summary_mermaid(packages, pkg_edges, direction)))

    by_pkg = defaultdict(set)
    for m in mod_to_file:
        by_pkg[package_of(m)].add(m)

    for pkg in sorted(by_pkg):
        mods_in_pkg = by_pkg[pkg]
        edges_detail = set()
        for a, b in edges:
            if a not in mods_in_pkg:
                continue
            if b.startswith("EXTERNAL:"):
                edges_detail.add((a, b))
            elif b in mods_in_pkg:
                edges_detail.add((a, b))
            else:
                edges_detail.add((a, "PKG:" + package_of(b)))
        title = f"Module Dependencies — {pkg}" if len(packages) > 1 else "Module Dependencies"
        sections.append((title, render_deps_detail_mermaid(mods_in_pkg, edges_detail, direction)))

    return sections


def build_deps_mermaid(files, scan_root, direction, include_external):
    if len(files) < 2:
        return None
    mod_to_file, edges = build_deps_edges(files, scan_root, include_external)
    return render_deps_mermaid(mod_to_file, edges, direction)


# --------------------------------------------------------------------------- #
# classes: inheritance graph
# --------------------------------------------------------------------------- #

def _gather_classes(files: list[Path], scan_root: Path):
    nodes, module_of, raw_edges = {}, {}, []
    for f in files:
        tree = safe_parse(f)
        if tree is None:
            continue
        mod = module_name_for(f, scan_root)
        for node in ast.walk(tree):
            if isinstance(node, ast.ClassDef):
                cid = f"{mod}.{node.name}"
                nodes[cid] = node.name
                module_of[cid] = mod
                for base in node.bases:
                    raw_edges.append((cid, node_text(base)))
    return nodes, module_of, raw_edges


def build_classes_mermaid(files: list[Path], scan_root: Path, direction: str):
    """Single combined diagram — every class in the project on one canvas. Used only
    for --combine-classes; for any real-sized project, build_classes_sections (the
    per-package split) below is what actually stays readable."""
    nodes, module_of, raw_edges = _gather_classes(files, scan_root)
    if not nodes:
        return None

    simple_to_ids = defaultdict(list)
    for cid, name in nodes.items():
        simple_to_ids[name].append(cid)

    lines = [f"flowchart {direction}"]
    groups = defaultdict(list)
    for cid, mod in module_of.items():
        groups[mod].append(cid)
    for i, (mod, cids) in enumerate(sorted(groups.items())):
        lines.append(f'  subgraph SG{i}["{esc(mod)}"]')
        for cid in sorted(cids):
            lines.append(f'    {sid(cid)}["{esc(nodes[cid])}"]')
        lines.append("  end")

    ext_declared = set()
    for cid, base_name in raw_edges:
        leaf = base_name.split(".")[-1]
        candidates = [c for c in simple_to_ids.get(leaf, []) if c != cid]
        if len(candidates) == 1:
            lines.append(f"  {sid(cid)} -->|extends| {sid(candidates[0])}")
        else:
            key = "EXTBASE_" + base_name
            if base_name not in ext_declared:
                lines.append(f'  {sid(key)}("{esc(base_name)}"):::external')
                ext_declared.add(base_name)
            lines.append(f"  {sid(cid)} -.->|extends| {sid(key)}")
    if ext_declared:
        lines.append("  classDef external fill:#eee,stroke:#999,stroke-dasharray: 3 3")
    return "\n".join(lines)


def build_classes_sections(files: list[Path], scan_root: Path, direction: str):
    """Default classes output: one diagram per top-level package. A class that extends
    a base defined in a different package links to a small stub node rather than
    pulling that whole other package's classes into the same canvas."""
    nodes, module_of, raw_edges = _gather_classes(files, scan_root)
    if not nodes:
        return []

    simple_to_ids = defaultdict(list)
    for cid, name in nodes.items():
        simple_to_ids[name].append(cid)

    by_pkg = defaultdict(list)
    for cid, mod in module_of.items():
        by_pkg[package_of(mod)].append(cid)

    multi_pkg = len(by_pkg) > 1
    sections = []
    for pkg in sorted(by_pkg):
        cids_in_pkg = set(by_pkg[pkg])
        lines = [f"flowchart {direction}"]
        for cid in sorted(cids_in_pkg):
            lines.append(f'  {sid(cid)}["{esc(nodes[cid])}"]')

        declared = set()
        has_ext = has_other_pkg = False
        for cid, base_name in raw_edges:
            if cid not in cids_in_pkg:
                continue
            leaf = base_name.split(".")[-1]
            candidates = [c for c in simple_to_ids.get(leaf, []) if c != cid]
            if len(candidates) == 1:
                target = candidates[0]
                if target in cids_in_pkg:
                    lines.append(f"  {sid(cid)} -->|extends| {sid(target)}")
                else:
                    key = "PKGCLS:" + target
                    if key not in declared:
                        other_pkg = module_of[target]
                        lines.append(f'  {sid(key)}[["{esc(nodes[target])} ({esc(other_pkg)})"]]:::pkglink')
                        declared.add(key)
                        has_other_pkg = True
                    lines.append(f"  {sid(cid)} -.->|extends| {sid(key)}")
            else:
                key = "EXTBASE_" + base_name
                if key not in declared:
                    lines.append(f'  {sid(key)}("{esc(base_name)}"):::external')
                    declared.add(key)
                    has_ext = True
                lines.append(f"  {sid(cid)} -.->|extends| {sid(key)}")
        if has_ext:
            lines.append("  classDef external fill:#eee,stroke:#999,stroke-dasharray: 3 3")
        if has_other_pkg:
            lines.append("  classDef pkglink fill:#dde,stroke:#668")
        title = f"Class Hierarchy — {pkg}" if multi_pkg else "Class Hierarchy"
        sections.append((title, "\n".join(lines)))
    return sections


# --------------------------------------------------------------------------- #
# calls: function/method call graph (per file)
# --------------------------------------------------------------------------- #

class CallGraphVisitor(ast.NodeVisitor):
    def __init__(self):
        self.class_stack: list[str] = []
        self.func_stack: list[str] = []
        self.defined: set[str] = set()
        self.class_of: dict[str, str | None] = {}
        self.calls: set[tuple[str, str]] = set()
        self.raw_bare_calls: list[tuple[str, str]] = []  # (caller_qualname, bare_name), resolved after full traversal

    def _qualname(self, name: str) -> str:
        if self.func_stack:
            return self.func_stack[-1] + "::" + name
        if self.class_stack:
            return self.class_stack[-1] + "." + name
        return name

    def visit_ClassDef(self, node: ast.ClassDef):
        self.class_stack.append(node.name)
        for stmt in node.body:
            self.visit(stmt)
        self.class_stack.pop()

    def visit_FunctionDef(self, node):
        self._func(node)

    def visit_AsyncFunctionDef(self, node):
        self._func(node)

    def _func(self, node):
        qn = self._qualname(node.name)
        self.defined.add(qn)
        self.class_of[qn] = self.class_stack[-1] if (self.class_stack and not self.func_stack) else None
        self.func_stack.append(qn)
        for stmt in node.body:
            self.visit(stmt)
        self.func_stack.pop()

    def visit_Call(self, node: ast.Call):
        if self.func_stack:
            caller = self.func_stack[-1]
            func = node.func
            if isinstance(func, ast.Name):
                # Resolved after traversal finishes, once `defined` covers forward-declared
                # siblings/nested defs too — see resolve_bare_calls().
                self.raw_bare_calls.append((caller, func.id))
            elif isinstance(func, ast.Attribute) and isinstance(func.value, ast.Name) and func.value.id in ("self", "cls"):
                cls = self.class_stack[-1] if self.class_stack else None
                callee = f"{cls}.{func.attr}" if cls else func.attr
                self.calls.add((caller, callee))
        self.generic_visit(node)


def resolve_bare_calls(raw_bare_calls, defined: set) -> set:
    """A bare `name()` call could refer to: a function nested inside the caller itself,
    a sibling in any enclosing scope, or a module-level function — checked in that order,
    matching Python's actual lexical scoping. Falls back to the bare name (shown as
    external) if nothing in `defined` matches."""
    resolved = set()
    for caller, name in raw_bare_calls:
        chain = caller.split("::")
        target = name
        for i in range(len(chain), -1, -1):
            prefix = "::".join(chain[:i])
            cand = f"{prefix}::{name}" if prefix else name
            if cand in defined:
                target = cand
                break
        resolved.add((caller, target))
    return resolved


def _render_call_nodes(lines, defined, class_of, prefix=""):
    by_class = defaultdict(list)
    free = []
    for qn in sorted(defined):
        cls = class_of.get(qn)
        (by_class[cls] if cls else free).append(qn)
    for i, (cls, funcs) in enumerate(sorted(by_class.items())):
        lines.append(f'  subgraph {sid(prefix + "cls_" + cls)}["{esc(cls)}"]')
        for qn in funcs:
            label = qn.split("::")[-1]
            lines.append(f'    {sid(prefix + qn)}["{esc(label)}"]')
        lines.append("  end")
    for qn in free:
        label = qn.split("::")[-1]
        lines.append(f'  {sid(prefix + qn)}["{esc(label)}"]')


def build_calls_mermaid_for_file(f: Path, direction: str, include_builtins: bool):
    """Returns a list of (title_suffix, text, n_funcs) — normally one entry, more than
    one only if the file's call graph was too big for a single diagram (typically one
    exceptionally large class) and got split into labeled parts with stub links between
    them, the same pattern used for cross-package links elsewhere in this file."""
    tree = safe_parse(f)
    if tree is None:
        return []
    v = CallGraphVisitor()
    v.visit(tree)
    if not v.defined:
        return []
    all_calls = v.calls | resolve_bare_calls(v.raw_bare_calls, v.defined)

    chunks = _split_into_chunks(v.defined, v.class_of)
    results = []
    for label, members in chunks:
        text = _render_calls_chunk(members, v.defined, all_calls, v.class_of, direction, include_builtins)
        results.append((label, text, len(members)))
    return results


MAX_CHUNK_NODES = 20  # split a file's call graph if any single class/free-function group exceeds this


def _split_into_chunks(defined: set, class_of: dict):
    """Group qualnames by class (methods of the same class stay together), splitting
    any class — or the free-function group — into labeled parts if it's bigger than
    MAX_CHUNK_NODES. Returns [(label_or_None, [qualnames]), ...]; a single-element
    result means no splitting was needed."""
    by_class = defaultdict(list)
    free = []
    for qn in sorted(defined):
        cls = class_of.get(qn)
        (by_class[cls] if cls else free).append(qn)

    chunks = []
    for cls, funcs in sorted(by_class.items()):
        if len(funcs) <= MAX_CHUNK_NODES:
            chunks.append((cls, funcs))
        else:
            n_parts = math.ceil(len(funcs) / MAX_CHUNK_NODES)
            for i in range(n_parts):
                part = funcs[i * MAX_CHUNK_NODES:(i + 1) * MAX_CHUNK_NODES]
                chunks.append((f"{cls}, part {i + 1}/{n_parts}", part))
    if free:
        if len(free) <= MAX_CHUNK_NODES:
            chunks.append((None, free))
        else:
            n_parts = math.ceil(len(free) / MAX_CHUNK_NODES)
            for i in range(n_parts):
                part = free[i * MAX_CHUNK_NODES:(i + 1) * MAX_CHUNK_NODES]
                chunks.append((f"module-level, part {i + 1}/{n_parts}", part))
    return chunks


def _render_calls_chunk(members: list, defined: set, calls: set, class_of: dict,
                         direction: str, include_builtins: bool) -> str:
    members_set = set(members)
    lines = [f"flowchart {direction}"]
    by_class = defaultdict(list)
    free = []
    for qn in members:
        cls = class_of.get(qn)
        (by_class[cls] if cls else free).append(qn)
    for cls, funcs in sorted(by_class.items()):
        lines.append(f'  subgraph {sid("cls_" + cls)}["{esc(cls)}"]')
        for qn in funcs:
            lines.append(f'    {sid(qn)}["{esc(qn.split("::")[-1])}"]')
        lines.append("  end")
    for qn in free:
        lines.append(f'  {sid(qn)}["{esc(qn.split("::")[-1])}"]')

    declared = set()
    has_ext = has_other = False
    for caller, callee in sorted(calls):
        if caller not in members_set:
            continue
        if callee in members_set:
            lines.append(f"  {sid(caller)} --> {sid(callee)}")
        elif callee in defined:
            # same function, defined elsewhere in this file — but split into a different
            # chunk for size reasons. Link to a stub rather than duplicating it here.
            key = "OTHER:" + callee
            if key not in declared:
                lines.append(f'  {sid(key)}[["{esc(callee.split("::")[-1])}"]]:::pkglink')
                declared.add(key)
                has_other = True
            lines.append(f"  {sid(caller)} -.-> {sid(key)}")
        else:
            leaf = callee.split(".")[-1]
            if not include_builtins and (leaf in BUILTIN_NAMES or leaf in NOISE_CALL_NAMES):
                continue
            key = "EXT_" + callee
            if key not in declared:
                lines.append(f'  {sid(key)}("{esc(callee)}"):::external')
                declared.add(key)
                has_ext = True
            lines.append(f"  {sid(caller)} -.-> {sid(key)}")
    if has_ext:
        lines.append("  classDef external fill:#eee,stroke:#999,stroke-dasharray: 3 3")
    if has_other:
        lines.append("  classDef pkglink fill:#dde,stroke:#668")
    return "\n".join(lines)


def build_calls_mermaid_combined(files: list[Path], scan_root: Path, direction: str, include_builtins: bool):
    lines = [f"flowchart {direction}"]
    all_defined = set()
    all_calls = set()
    all_class_of = {}
    for i, f in enumerate(files):
        tree = safe_parse(f)
        if tree is None:
            continue
        v = CallGraphVisitor()
        v.visit(tree)
        if not v.defined:
            continue
        file_calls = v.calls | resolve_bare_calls(v.raw_bare_calls, v.defined)
        mod = module_name_for(f, scan_root)
        prefix = mod + "::"
        defined_p = {prefix + qn for qn in v.defined}
        class_of_p = {prefix + qn: cls for qn, cls in v.class_of.items()}
        calls_p = {(prefix + a, prefix + b if b in v.defined else b) for a, b in file_calls}

        lines.append(f'  subgraph {sid("mod_" + mod)}["{esc(mod)}"]')
        _render_call_nodes(lines, defined_p, class_of_p, prefix="")
        lines.append("  end")

        all_defined |= defined_p
        all_calls |= calls_p
        all_class_of.update(class_of_p)

    ext_declared = set()
    for caller, callee in sorted(all_calls):
        if callee in all_defined:
            lines.append(f"  {sid(caller)} --> {sid(callee)}")
        else:
            leaf = callee.split(".")[-1]
            if not include_builtins and (leaf in BUILTIN_NAMES or leaf in NOISE_CALL_NAMES):
                continue
            key = "EXT_" + callee
            if key not in ext_declared:
                lines.append(f'  {sid(key)}("{esc(callee)}"):::external')
                ext_declared.add(key)
            lines.append(f"  {sid(caller)} -.-> {sid(key)}")
    if ext_declared:
        lines.append("  classDef external fill:#eee,stroke:#999,stroke-dasharray: 3 3")
    return "\n".join(lines)


# --------------------------------------------------------------------------- #
# cfg: real control-flow flowchart for one function
# --------------------------------------------------------------------------- #

class CFGBuilder:
    """Builds a control-flow graph from a single function's AST body.

    `cur` is always a list of (node_id, edge_label) pairs: the dangling predecessors
    that still need to be wired to whatever comes next. An empty list means every path
    so far has terminated (return/raise/break/continue), so following statements are
    unreachable and are dropped.
    """

    def __init__(self):
        self.nodes: dict[str, tuple[str, str]] = {}
        self.edges: list[tuple[str, str, str | None]] = []
        self._n = 0
        self.loop_stack: list[dict] = []
        self.start_id = self._add("stadium", "Start")
        self.end_id = self._add("stadium", "End")
        self._exc_id = None

    def _new_id(self) -> str:
        self._n += 1
        return f"n{self._n}"

    def _add(self, shape: str, label: str) -> str:
        nid = self._new_id()
        self.nodes[nid] = (shape, label)
        return nid

    def _connect(self, cur, node_id):
        for src, label in cur:
            self.edges.append((src, node_id, label))

    def _exc_node(self):
        if self._exc_id is None:
            self._exc_id = self._add("stadium", "Exception")
        return self._exc_id

    def build(self, func):
        cur = [(self.start_id, None)]
        cur = self._body(func.body, cur)
        for src, label in cur:
            self.edges.append((src, self.end_id, label))
        return self.nodes, self.edges

    def _body(self, stmts, cur):
        buf: list[str] = []

        def flush():
            nonlocal cur, buf
            if buf:
                # Real "\n" here, not "<br/>" — escaping (incl. the <br/> conversion)
                # happens once, at render time, same as every other label in this file.
                label = "\n".join(truncate(l) for l in buf)
                nid = self._add("rect", label)
                self._connect(cur, nid)
                cur = [(nid, None)]
                buf = []

        for stmt in stmts:
            if not cur:
                break  # unreachable / dead code

            if isinstance(stmt, ast.If):
                flush()
                cond = self._add("diamond", node_text(stmt.test))
                self._connect(cur, cond)
                true_exits = self._body(stmt.body, [(cond, "yes")])
                false_exits = self._body(stmt.orelse, [(cond, "no")]) if stmt.orelse else [(cond, "no")]
                cur = true_exits + false_exits

            elif isinstance(stmt, (ast.For, ast.AsyncFor)):
                flush()
                cond = self._add("diamond", f"for {node_text(stmt.target)} in {node_text(stmt.iter)}")
                self._connect(cur, cond)
                self.loop_stack.append({"continue": cond, "breaks": []})
                body_exits = self._body(stmt.body, [(cond, "next item")])
                for src, label in body_exits:
                    self.edges.append((src, cond, label))
                info = self.loop_stack.pop()
                cur = [(cond, "exhausted")] + info["breaks"]
                if stmt.orelse:
                    cur = self._body(stmt.orelse, cur)

            elif isinstance(stmt, ast.While):
                flush()
                cond = self._add("diamond", f"while {node_text(stmt.test)}")
                self._connect(cur, cond)
                self.loop_stack.append({"continue": cond, "breaks": []})
                body_exits = self._body(stmt.body, [(cond, "true")])
                for src, label in body_exits:
                    self.edges.append((src, cond, label))
                info = self.loop_stack.pop()
                cur = [(cond, "false")] + info["breaks"]
                if stmt.orelse:
                    cur = self._body(stmt.orelse, cur)

            elif isinstance(stmt, ast.Break):
                flush()
                nid = self._add("rect", "break")
                self._connect(cur, nid)
                if self.loop_stack:
                    self.loop_stack[-1]["breaks"].append((nid, None))
                cur = []

            elif isinstance(stmt, ast.Continue):
                flush()
                nid = self._add("rect", "continue")
                self._connect(cur, nid)
                if self.loop_stack:
                    self.edges.append((nid, self.loop_stack[-1]["continue"], None))
                cur = []

            elif isinstance(stmt, ast.Return):
                flush()
                text = "return " + node_text(stmt.value) if stmt.value is not None else "return"
                nid = self._add("rect", text)
                self._connect(cur, nid)
                self.edges.append((nid, self.end_id, None))
                cur = []

            elif isinstance(stmt, ast.Raise):
                flush()
                nid = self._add("rect", node_text(stmt))
                self._connect(cur, nid)
                self.edges.append((nid, self._exc_node(), None))
                cur = []

            elif isinstance(stmt, ast.Try):
                flush()
                entry = cur
                try_exits = self._body(stmt.body, cur)
                handler_exits = []
                for h in stmt.handlers:
                    h_label = "except " + node_text(h.type) if h.type is not None else "except"
                    h_node = self._add("rect", h_label)
                    for src, _ in entry:
                        self.edges.append((src, h_node, "on exc"))
                    handler_exits += self._body(h.body, [(h_node, None)])
                combined = (self._body(stmt.orelse, try_exits) if stmt.orelse else try_exits) + handler_exits
                cur = self._body(stmt.finalbody, combined) if stmt.finalbody else combined

            elif isinstance(stmt, (ast.With, ast.AsyncWith)):
                flush()
                items = ", ".join(node_text(it.context_expr) for it in stmt.items)
                nid = self._add("rect", f"with {items}")
                self._connect(cur, nid)
                cur = self._body(stmt.body, [(nid, None)])

            elif isinstance(stmt, getattr(ast, "Match", ())):
                flush()
                try:
                    banner = self._add("rect", f"match {node_text(stmt.subject)}")
                    self._connect(cur, banner)
                    cur = self._match_cases(stmt.cases, [(banner, None)])
                except Exception:
                    nid = self._add("rect", "match ...")
                    self._connect(cur, nid)
                    cur = [(nid, None)]

            else:
                buf.append(stmt_preview(stmt))

        flush()
        return cur

    def _match_cases(self, cases, cur):
        if not cases:
            return cur
        case = cases[0]
        try:
            pat = "case " + node_text(case.pattern)
        except Exception:
            pat = "case ..."
        if case.guard is not None:
            pat += f" if {node_text(case.guard)}"
        cond = self._add("diamond", pat)
        self._connect(cur, cond)
        body_exits = self._body(case.body, [(cond, "match")])
        rest = self._match_cases(cases[1:], [(cond, "no match")])
        return body_exits + rest


def render_cfg_mermaid(nodes: dict, edges: list, direction: str) -> str:
    lines = [f"flowchart {direction}"]
    for nid, (shape, label) in nodes.items():
        text = esc(label)
        if shape == "diamond":
            lines.append(f'  {nid}{{"{text}"}}')
        elif shape == "stadium":
            lines.append(f'  {nid}(["{text}"])')
        else:
            lines.append(f'  {nid}["{text}"]')
    for src, dst, label in edges:
        if label:
            lines.append(f"  {src} -->|{esc(label)}| {dst}")
        else:
            lines.append(f"  {src} --> {dst}")
    return "\n".join(lines)


def find_function(file: Path, qualname: str):
    tree = safe_parse(file)
    if tree is None:
        raise SystemExit(f"Could not parse {file}")
    parts = qualname.split(".")
    if len(parts) == 1:
        for node in tree.body:
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name == parts[0]:
                return node
        raise SystemExit(f"Function '{qualname}' not found at module level in {file}")
    if len(parts) == 2:
        cls_name, meth_name = parts
        for node in tree.body:
            if isinstance(node, ast.ClassDef) and node.name == cls_name:
                for sub in node.body:
                    if isinstance(sub, (ast.FunctionDef, ast.AsyncFunctionDef)) and sub.name == meth_name:
                        return sub
        raise SystemExit(f"Method '{qualname}' not found in {file}")
    raise SystemExit("--cfg qualname must be 'func' or 'Class.method'")


# --------------------------------------------------------------------------- #
# --list-functions
# --------------------------------------------------------------------------- #

def list_functions(files: list[Path]):
    for f in files:
        tree = safe_parse(f)
        if tree is None:
            continue
        for node in tree.body:
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                print(f"{f}:{node.name}")
            elif isinstance(node, ast.ClassDef):
                for sub in node.body:
                    if isinstance(sub, (ast.FunctionDef, ast.AsyncFunctionDef)):
                        print(f"{f}:{node.name}.{sub.name}")


# --------------------------------------------------------------------------- #
# CLI
# --------------------------------------------------------------------------- #

def render_markdown(sections: list[tuple[str, str]]) -> str:
    parts = ["# AST → Mermaid Flowcharts", ""]
    for title, body in sections:
        parts.append(f"## {title}")
        parts.append("")
        parts.append("```mermaid")
        parts.append(body)
        parts.append("```")
        parts.append("")
    return "\n".join(parts)


def main():
    ap = argparse.ArgumentParser(
        description="Generate Mermaid flowcharts from Python source via AST.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    ap.add_argument("path", help="File or directory to analyze")
    ap.add_argument("--graphs", default="deps,classes",
                     help="Comma-separated: deps,calls,classes,all (default: deps,classes)")
    ap.add_argument("--cfg", action="append", default=[], metavar="FILE:QUALNAME",
                     help="Control-flow flowchart for one function, e.g. "
                          "pkg/mod.py:ClassName.method or pkg/mod.py:func. Repeatable.")
    ap.add_argument("--list-functions", action="store_true",
                     help="Print 'file:qualname' for every function/method in scope, then exit "
                          "(use this to find valid --cfg targets)")
    ap.add_argument("-o", "--output", help="Write the Markdown report here instead of stdout")
    ap.add_argument("--direction", default="TD", choices=["TD", "LR", "BT", "RL"])
    ap.add_argument("--exclude", action="append", default=[],
                     help="Glob pattern to exclude, matched against path relative to the "
                          "scanned root and against filename (repeatable)")
    ap.add_argument("--include-external", action="store_true",
                     help="Show unresolved/external imports as extra nodes in the deps graph")
    ap.add_argument("--include-builtins", action="store_true",
                     help="Include calls to builtins (len, str, print, ...) in call graphs")
    ap.add_argument("--combine-deps", action="store_true",
                     help="Merge all packages into a single deps diagram instead of one overview + one per package")
    ap.add_argument("--combine-classes", action="store_true",
                     help="Merge all packages into a single class-hierarchy diagram instead of one per package")
    ap.add_argument("--combine-calls", action="store_true",
                     help="Merge all per-file call graphs into a single diagram")
    ap.add_argument("--min-funcs", type=int, default=2,
                     help="Skip per-file call graphs with fewer than N functions (default: 2)")
    args = ap.parse_args()

    path = Path(args.path).resolve()
    files, scan_root = discover(path, args.exclude)
    if not files:
        raise SystemExit(f"No .py files found under {path}")

    if args.list_functions:
        list_functions(files)
        return

    graphs = {g.strip() for g in args.graphs.split(",") if g.strip()}
    if "all" in graphs:
        graphs = {"deps", "calls", "classes"}

    sections: list[tuple[str, str]] = []

    def add_section(title, text):
        if text is None:
            return
        if len(text) > 10000:
            print(f"warning: '{title}' is {len(text):,} chars — some Mermaid renderers cap around "
                  f"here. Consider --exclude to narrow scope if it fails to render.", file=sys.stderr)
        sections.append((title, text))

    if "deps" in graphs:
        if args.combine_deps:
            add_section("Module Dependencies", build_deps_mermaid(files, scan_root, args.direction, args.include_external))
        else:
            for title, text in build_deps_sections(files, scan_root, args.direction, args.include_external):
                add_section(title, text)
        if len(files) < 2:
            print("note: deps graph needs more than one file in scope, skipping", file=sys.stderr)

    if "classes" in graphs:
        if args.combine_classes:
            add_section("Class Hierarchy", build_classes_mermaid(files, scan_root, args.direction))
        else:
            for title, text in build_classes_sections(files, scan_root, args.direction):
                add_section(title, text)

    if "calls" in graphs:
        if args.combine_calls:
            add_section("Call Graph (combined)",
                         build_calls_mermaid_combined(files, scan_root, args.direction, args.include_builtins))
        else:
            for f in files:
                rel = f.relative_to(scan_root) if scan_root in f.parents or f == scan_root else f.name
                for label, text, n_funcs in build_calls_mermaid_for_file(f, args.direction, args.include_builtins):
                    if n_funcs < args.min_funcs:
                        continue
                    title = f"Call Graph — {rel}" + (f" ({label})" if label else "")
                    add_section(title, text)

    for spec in args.cfg:
        if ":" not in spec:
            raise SystemExit(f"--cfg expects FILE:QUALNAME, got: {spec}")
        file_part, _, qual = spec.rpartition(":")
        func_node = find_function(Path(file_part), qual)
        builder = CFGBuilder()
        nodes, edges = builder.build(func_node)
        add_section(f"CFG — {qual} ({file_part})", render_cfg_mermaid(nodes, edges, args.direction))

    if not sections:
        raise SystemExit("Nothing to render — check --graphs/--cfg and the path.")

    md = render_markdown(sections)
    if args.output:
        Path(args.output).write_text(md, encoding="utf-8")
        print(f"Wrote {len(sections)} diagram(s) to {args.output}", file=sys.stderr)
    else:
        print(md)


if __name__ == "__main__":
    main()
