#!/usr/bin/env python3
"""Writes the WG_DelT_* plugins from the WG_Del_* ones: the same four plugins and the same
edits, but every reference of the test cell sits AFTER a NAM0 (so it is a *temporary*
reference, which is what almost every reference in the vanilla exteriors is) and the cell is
called "WG Delete Test T", so both sets can be enabled at once.

Why: the first test (README.md) has no NAM0 anywhere, so every reference in it is
persistent. Robin's cases (the Seyda Neen lighthouse, Wolverine Hall's flag; gardenfell
esp-format.md sections 7 and 9.0) are temporary references in exteriors, and there a later
record did NOT bring a deleted reference back. Persistent or temporary may be the difference.

Run in this folder:  python3 make_temporary_variant.py
"""
import struct, os

OLD, NEW = "WG_Del_", "WG_DelT_"
FILES = ["Base.esm", "P1.esp", "A.esp", "P2.esp"]
CELL_OLD, CELL_NEW = b"WG Delete Test\0", b"WG Delete Test T\0"


def parse(path):
    b = open(path, "rb").read()
    i, recs = 0, []
    while i < len(b):
        tag = b[i:i + 4]
        size, h1, fl = struct.unpack("<III", b[i + 4:i + 16])
        body, j, subs = b[i + 16:i + 16 + size], 0, []
        while j < len(body):
            st, ss = body[j:j + 4], struct.unpack("<I", body[j + 4:j + 8])[0]
            subs.append([st, body[j + 8:j + 8 + ss]])
            j += 8 + ss
        recs.append([tag, h1, fl, subs])
        i += 16 + size
    return recs


def write(path, recs):
    out = b""
    for tag, h1, fl, subs in recs:
        body = b"".join(st + struct.pack("<I", len(d)) + d for st, d in subs)
        out += tag + struct.pack("<III", len(body), h1, fl) + body
    open(path, "wb").write(out)
    return len(out)


def convert(recs):
    for rec in recs:
        tag, subs = rec[0], rec[3]
        if tag == b"TES3":
            for s in subs:
                if s[0] == b"MAST":
                    s[1] = s[1].replace(OLD.encode(), NEW.encode())
        elif tag == b"CELL":
            new, refs = [], sum(1 for s in subs if s[0] == b"FRMR")
            for s in subs:
                if s[0] == b"NAME" and s[1] == CELL_OLD:
                    s[1] = CELL_NEW
                if s[0] == b"FRMR":
                    # the count of temporary references follows the cell's own fields
                    if not any(n[0] == b"NAM0" for n in new):
                        new.append([b"NAM0", struct.pack("<I", refs)])
                new.append(s)
            rec[3] = new
    return recs


def main():
    sizes = {}
    # the master first, so the plugins can record its size
    for f in FILES:
        recs = convert(parse(OLD + f))
        if f != "Base.esm":
            # MAST WG_DelT_Base.esm is followed by its DATA (the master's size)
            for k, s in enumerate(recs[0][3]):
                if s[0] == b"MAST" and s[1].startswith(NEW.encode() + b"Base.esm"):
                    recs[0][3][k + 1][1] = struct.pack("<Q", sizes["Base.esm"])
        sizes[f] = write(NEW + f, recs)
        print(NEW + f, sizes[f], "bytes")


if __name__ == "__main__":
    main()
