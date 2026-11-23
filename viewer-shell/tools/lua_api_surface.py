#!/usr/bin/env python3
"""The OpenMW Lua API's surface, read from OpenMW's own definition files (Static 2026-11-23).

OpenMW documents its scripting API in `files/lua_api/openmw/*.lua` (LuaDoc: `@module`, `@type`,
`@function [parent=#T] name`, `@field [parent=#T] #Type name`, `@return`, `@extends`). This reads
those files and writes the names and declared types only - no descriptions - as JSON for the
Preview's Lua (`viewcore/src/luarun.lua`, `api_fill`): every documented call is defined, and the
ones the Preview cannot do answer a value of the documented type and say once that they are not
run in the preview.

    python3 tools/lua_api_surface.py <openmw checkout>/files/lua_api/openmw COMMIT \
        > viewcore/src/lua_api/openmw.json

The output keeps the source commit, so a newer OpenMW is a re-run and a diff.
"""
import json
import os
import re
import sys

FN = re.compile(r'^--\s*@function\s*\[parent=#(\w+)\]\s*([\w.]+)')
FIELD_P = re.compile(r'^--\s*@field\s*\[parent=#(\w+)\]\s*(\S+)\s+(\w+)')
FIELD = re.compile(r'^--\s*@field\s+(\S+)\s+(\w+)')
TYPE = re.compile(r'^--\s*@type\s+(\w+)')
MODULE = re.compile(r'^--\s*@module\s+([\w.]+)')
RET = re.compile(r'^--\s*@return\s+(\S+)')
EXT = re.compile(r'^--\s*@extends\s+(\S+)')
# "can only be used by player and menu scripts", "Only available from global scripts".
ONLY = re.compile(r'(?:can only be used by|only available (?:from|in)|available only (?:from|in))\s+((?:(?:global|local|player|menu|load)(?:,\s*|\s+and\s+|\s+or\s+)?)+)\s*scripts', re.I)


def short(t):
    """`openmw.core#GameObject` -> `GameObject`; `#list<#string>` -> `list`; `#number` -> `number`."""
    t = t.rstrip(',')
    if t.startswith('#list') or t.startswith('#map') or t.startswith('list<') or t.startswith('#table'):
        return 'table'
    if '#' in t:
        t = t.split('#', 1)[1]
    return re.sub(r'[^\w]', '', t) or 'any'


def read(folder):
    modules, types, only = {}, {}, {}

    def ty(name):
        return types.setdefault(name, {'fns': {}, 'fields': {}, 'extends': None})

    for fname in sorted(os.listdir(folder)):
        if not fname.endswith('.lua'):
            continue
        cur_type = None
        last_fn = None
        module = None
        said = ''   # the description before the next tag
        for line in open(os.path.join(folder, fname), encoding='utf-8'):
            line = line.rstrip('\n')
            if line.startswith('---'):
                said = ''
            elif line.startswith('--') and '@' not in line[:6]:
                said += ' ' + line[2:].strip()
            m = MODULE.match(line)
            if m:
                module = m.group(1)
                modules[module] = module
                ty(module)
                cur_type = module
                continue
            m = TYPE.match(line)
            if m:
                cur_type = m.group(1)
                ty(cur_type)
                last_fn = None
                continue
            m = EXT.match(line)
            if m and cur_type:
                ty(cur_type)['extends'] = short(m.group(1))
                continue
            m = FN.match(line)
            if m:
                parent, name = m.group(1), m.group(2).split('.')[-1]
                ty(parent)['fns'][name] = 'nil'
                last_fn = (parent, name)
                o = ONLY.search(said)
                if o:
                    only[parent + '.' + name] = sorted(set(re.findall(r'global|local|player|menu|load', o.group(1).lower())))
                said = ''
                continue
            m = RET.match(line)
            if m and last_fn:
                p, n = last_fn
                if types[p]['fns'][n] == 'nil':
                    types[p]['fns'][n] = short(m.group(1))
                continue
            m = FIELD_P.match(line)
            if m:
                ty(m.group(1))['fields'][m.group(3)] = short(m.group(2))
                continue
            m = FIELD.match(line)
            if m and cur_type:
                ty(cur_type)['fields'][m.group(2)] = short(m.group(1))
                continue
            if not line.startswith('--'):
                last_fn = None
    return modules, types, only


def main():
    folder, commit = sys.argv[1], sys.argv[2] if len(sys.argv) > 2 else ''
    modules, types, only = read(folder)
    out = {
        'source': 'OpenMW files/lua_api/openmw at ' + commit,
        # package name (`openmw.core`) -> the type its table is
        'packages': {('openmw.' + m.lower() if not m.startswith('openmw') else m): m for m in sorted(modules)},
        'types': {k: types[k] for k in sorted(types)},
        # `Type.name` -> the kinds of script it is only there for, as its documentation says
        'only': only,
    }
    json.dump(out, sys.stdout, indent=0, sort_keys=True)
    sys.stdout.write('\n')


if __name__ == '__main__':
    main()
