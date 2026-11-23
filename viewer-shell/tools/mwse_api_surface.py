#!/usr/bin/env python3
"""The MWSE Lua API's surface, read from MWSE's own definition files (Static 2026-11-23).

MWSE describes its API as Lua tables in `autocomplete/definitions` (what its documentation and
editor completions are built from): `global/<lib>.lua` and `global/<lib>/<member>.lua` for the
libraries (`tes3`, `mwse`, `tes3ui`, `event`, `timer`, `json`, `lfs`, `mge`...), `namedTypes/<type>.lua`
and `namedTypes/<type>/<member>.lua` for the classes (`tes3reference`, `tes3mobileActor`...), and
`events/standard/<event>.lua` for the events. This reads them and writes the names and declared
types only - no descriptions, no code - as JSON for the Preview's Lua (`viewcore/src/luarun.lua`,
`mwse_fill`): every documented call is defined, and what the Preview cannot do answers a value of
the documented type and says once that it is not run in the preview.

    python3 tools/mwse_api_surface.py <MWSE checkout>/autocomplete/definitions SOURCE \\
        [<MWSE checkout>/misc/package/Data Files/MWSE/core/lib/tes3] > viewcore/src/lua_api/mwse.json

The third folder gives `tes3`'s enumerations (`tes3.objectType`, `tes3.effect`, `tes3.skill`...):
the game's own constants, name to value, which a mod compares against what the game answers.

The definition files are evaluated with a small reader of Lua's table literals (strings, long
strings, numbers, booleans, tables), not with Lua: they are data, and are never run.
"""
import json
import os
import re
import sys


class Lua:
    """A reader of the literal Lua tables the definition files are made of."""

    def __init__(self, text):
        self.s, self.i = text, 0

    def skip(self):
        s = self.s
        while self.i < len(s):
            c = s[self.i]
            if c in ' \t\r\n':
                self.i += 1
            elif s.startswith('--', self.i):
                m = re.match(r'--\[(=*)\[', s[self.i:])
                if m:
                    end = s.find(']' + m.group(1) + ']', self.i)
                    self.i = len(s) if end < 0 else end + len(m.group(1)) + 2
                else:
                    end = s.find('\n', self.i)
                    self.i = len(s) if end < 0 else end + 1
            else:
                break

    def value(self):
        self.skip()
        s, i = self.s, self.i
        c = s[i]
        if c == '{':
            return self.table()
        if c in '"\'':
            return self.string(c)
        m = re.match(r'\[(=*)\[', s[i:])
        if m:
            close = ']' + m.group(1) + ']'
            end = s.find(close, i)
            self.i = end + len(close)
            return s[i + len(m.group(0)):end]
        m = re.match(r'-?(0x[0-9a-fA-F]+|\d+\.?\d*(?:[eE][-+]?\d+)?|\.\d+)', s[i:])
        if m:
            self.i += len(m.group(0))
            t = m.group(0)
            return int(t, 16) if 'x' in t.lower() else float(t)
        m = re.match(r'[A-Za-z_][\w.]*', s[i:])
        if m:
            self.i += len(m.group(0))
            w = m.group(0)
            # An expression the reader does not evaluate (a name, a call): skip its arguments.
            self.skip()
            if self.i < len(s) and s[self.i] == '(':
                depth = 0
                while self.i < len(s):
                    if s[self.i] == '(':
                        depth += 1
                    elif s[self.i] == ')':
                        depth -= 1
                        if depth == 0:
                            self.i += 1
                            break
                    self.i += 1
            return {'true': True, 'false': False, 'nil': None}.get(w, w)
        raise ValueError('unreadable at %d: %r' % (i, s[i:i + 40]))

    def string(self, q):
        s, i = self.s, self.i + 1
        out = []
        while s[i] != q:
            if s[i] == '\\':
                i += 1
                out.append({'n': '\n', 't': '\t'}.get(s[i], s[i]))
            else:
                out.append(s[i])
            i += 1
        self.i = i + 1
        return ''.join(out)

    def table(self):
        self.i += 1
        arr, obj = [], {}
        while True:
            self.skip()
            if self.s[self.i] == '}':
                self.i += 1
                break
            m = re.match(r'([A-Za-z_]\w*)\s*=(?!=)', self.s[self.i:])
            if m:
                self.i += len(m.group(0))
                obj[m.group(1)] = self.value()
            elif self.s[self.i] == '[' and not re.match(r'\[=*\[', self.s[self.i:]):
                self.i += 1
                k = self.value()
                self.skip()
                self.i += 1  # ]
                self.skip()
                self.i += 1  # =
                obj[str(k)] = self.value()
            else:
                arr.append(self.value())
            self.skip()
            if self.s[self.i] in ',;':
                self.i += 1
        if obj and arr:
            for n, v in enumerate(arr, 1):
                obj.setdefault(str(n), v)
            return obj
        return obj if obj else arr


def load(path):
    text = open(path, encoding='utf-8').read()
    text = re.sub(r'^\s*return\s*', '', text, count=1)
    return Lua(text).value()


def first_type(t):
    """`tes3reference|nil` -> `tes3reference`; `table<string, x>` / `x[]` -> `table`."""
    if not isinstance(t, str) or not t:
        return 'nil'
    t = t.split('|')[0].strip()
    if t.endswith('[]') or t.startswith('table') or t.startswith('{'):
        return 'table'
    if t.startswith('fun('):
        return 'function'
    return re.sub(r'[^\w.]', '', t) or 'nil'


def ret_of(d):
    r = d.get('returns') if isinstance(d, dict) else None
    if isinstance(r, list) and r:
        r = r[0]
    if isinstance(r, dict):
        return first_type(r.get('type'))
    if isinstance(r, str):
        return first_type(r)
    # A method's single answer is often its `valuetype`.
    if isinstance(d, dict) and d.get('valuetype'):
        return first_type(d.get('valuetype'))
    return 'nil'


def members(folder, into):
    """`<folder>/<name>.lua` members into `into` (fns / fields), and sub-libraries by folder."""
    for f in sorted(os.listdir(folder)):
        p = os.path.join(folder, f)
        if not f.endswith('.lua'):
            continue
        name = f[:-4]
        try:
            d = load(p)
        except Exception as e:  # a file the reader does not follow: named, and passed over
            sys.stderr.write('%s: %s\n' % (p, e))
            continue
        if not isinstance(d, dict):
            continue
        kind = d.get('type')
        if kind in ('function', 'method'):
            into['fns'][name] = ret_of(d)
        elif kind == 'value':
            into['fields'][name] = first_type(d.get('valuetype') or d.get('valueType'))
        elif kind in ('lib', 'class'):
            into['fields'][name] = name
        elif kind == 'operator':
            continue   # `__add` and the like: how a value combines, not a name to look up
        else:
            into['fields'][name] = first_type(d.get('valuetype'))


def main():
    root, source = sys.argv[1], (sys.argv[2] if len(sys.argv) > 2 else '')
    types, libs, events = {}, {}, {}

    def ty(n):
        return types.setdefault(n, {'fns': {}, 'fields': {}, 'extends': None})

    # The libraries, `global/<lib>.lua` with its members in `global/<lib>/`, and the libraries
    # nested in them (`mwse/mcm.lua`, a lib, with `mwse/mcm/`). A folder beside a member that is
    # not a lib holds that member's examples, and is passed over.
    def lib(folder, name):
        ty(name)
        if not os.path.isdir(folder):
            return
        members(folder, types[name])
        for f in sorted(os.listdir(folder)):
            if f.endswith('.lua'):
                d = load(os.path.join(folder, f))
                if isinstance(d, dict) and d.get('type') == 'lib':
                    sub = name + '.' + f[:-4]
                    types[name]['fields'][f[:-4]] = sub
                    lib(os.path.join(folder, f[:-4]), sub)

    g = os.path.join(root, 'global')
    for f in sorted(os.listdir(g)):
        if f.endswith('.lua'):
            d = load(os.path.join(g, f))
            if isinstance(d, dict) and d.get('type') == 'lib':
                libs[f[:-4]] = f[:-4]
                lib(os.path.join(g, f[:-4]), f[:-4])
    # The classes.
    nt = os.path.join(root, 'namedTypes')
    for f in sorted(os.listdir(nt)):
        if f.endswith('.lua'):
            n = f[:-4]
            d = load(os.path.join(nt, f))
            t = ty(n)
            if isinstance(d, dict) and isinstance(d.get('inherits'), str):
                t['extends'] = d['inherits']
            sub = os.path.join(nt, n)
            if os.path.isdir(sub):
                members(sub, t)
    # The events: their data's fields, the filter, whether they can be blocked.
    ev = os.path.join(root, 'events')
    for dirpath, _, files in os.walk(ev):
        for f in sorted(files):
            if not f.endswith('.lua'):
                continue
            d = load(os.path.join(dirpath, f))
            if not isinstance(d, dict) or d.get('type') != 'event':
                continue
            data = d.get('eventData') or {}
            events[f[:-4]] = {
                'data': {k: first_type(v.get('type')) for k, v in sorted(data.items()) if isinstance(v, dict)},
                'filter': d.get('filter') if isinstance(d.get('filter'), str) else None,
                'blockable': bool(d.get('blockable')),
            }
    # tes3's enumerations: `name = value` tables of constants (the game's own numbers and names).
    enums = {}
    if len(sys.argv) > 3:
        lib = sys.argv[3]
        for f in sorted(os.listdir(lib)):
            if not f.endswith('.lua') or f == 'init.lua':
                continue
            try:
                d = load(os.path.join(lib, f))
            except Exception as e:
                sys.stderr.write('%s: %s\n' % (f, e))
                continue
            if isinstance(d, dict) and d and all(isinstance(v, (int, float, str, bool)) for v in d.values()):
                enums[f[:-4]] = {k: (int(v) if isinstance(v, float) and v.is_integer() else v) for k, v in sorted(d.items())}
            elif isinstance(d, list) and d and all(isinstance(v, (int, float, str)) for v in d):
                enums[f[:-4]] = {str(i): v for i, v in enumerate(d, 1)}
    out = {'source': 'MWSE autocomplete/definitions, ' + source, 'libs': libs, 'types': types, 'events': events, 'enums': enums}
    json.dump(out, sys.stdout, indent=0, sort_keys=True)
    sys.stdout.write('\n')


if __name__ == '__main__':
    main()
