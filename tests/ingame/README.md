# In-game test: does a later edit undelete a reference?

**Result (corrected 2026-11-01): four piles. A deletion sticks** - in Morrowind.exe, for a
persistent reference in an interior, as Robin's in-game cases (the Seyda Neen lighthouse,
Wolverine Hall's flag; `gardenfell-esp-format.md` §7, §9.0) say for temporary references in
exteriors, and as Gardenfell's round 18fc rule has it. The first run was written up as *five*
piles ("a later record wins", Gardenfell patch 0017): the fifth was the marker coin pile at
`coc`, counted as a pile of the test. There were four countable
piles - the control (ref 3) and the three markers - and nothing at x = -300.

The Construction Set says the same without a count: loading plugins in which a later one edits a
reference an earlier one deleted makes it report an error, which it would have no reason to do if
the later record brought the reference back.

Patch 0017 is withdrawn; `ordercore::refs`, the Editor's warnings and the shared cases
(`ref_cases.json`, `move_cases.json`) follow 18fc.

`ori` on the piles:

| sourceID | targetID | file |
|---|---|---|
| 01000003 | | WG_Del_Base.esm (ref 3, the control) |
| 0180000a | | WG_Del_P1.esp (marker) |
| 01c0000b | | WG_Del_A.esp (marker) |
| 0200000c | | WG_Del_P2.esp (marker) |

Refs 1 (deleted by A; P2's later edit is dropped) and 2 (edited by P1, deleted later by A) are both
gone. **Not yet run: OpenMW.** Its `cellstore.cpp`, `CellRefList<X>::load`, replaces the earlier
record for a reference whole and marks it deleted only if the *new* record is, which would bring
ref 1 back (five real piles, the big one at x = -300); if it does, the two engines differ and the
viewer (Gardenfell, which follows OpenMW elsewhere) and the Editor (which follows the game) would
want to say so. See "What you should see".

Four plugins, one interior cell, three gold piles. Load order (after Morrowind.esm):

    WG_Del_Base.esm   the cell "WG Delete Test": gold_001 refs 1, 2, 3 in a row (x = -300, 0, 300)
    WG_Del_P1.esp     edits ref 2 (raised 120, scale 2)    loads BEFORE the deleter
    WG_Del_A.esp      deletes refs 1 and 2 (a DELE each)
    WG_Del_P2.esp     edits ref 1 (raised 120, scale 2)    loads AFTER the deleter

Copy the four files to `Data Files` (or an OpenMW data folder) and enable them in this order. In
OpenMW: `content=WG_Del_Base.esm`, `content=WG_Del_P1.esp`, `content=WG_Del_A.esp`,
`content=WG_Del_P2.esp`, after Morrowind.esm. Every plugin lists Morrowind.esm as a master (the
Construction Set refuses `gold_001` otherwise); the files record the GOTY size of Morrowind.esm,
79,837,557. A copy of another size makes the game or CS warn about the master, which does not change
the result.

## Run

1. Start a game, open the console (`~`), type `tcl` (collision off; the cell has no floor and
   this also stops you falling), then `coc "WG Delete Test"`.
2. Look ahead (+Y, the way you face on arrival). The piles are 400 units out. Fly up if you do
   not see them. Count the gold piles and note where each one is.

## What you should see

Each of P1, A and P2 also places a **marker** pile of its own, at x = 600, 900 and 1200. All three
must be there: a missing marker means that plugin did not load, and the run proves nothing.

| Ref | History | Last wins | A deletion sticks |
|---|---|---|---|
| 1 (x=-300) | Base places it; A deletes it; **P2 edits it later** | a **big pile, raised** (scale 2, z=220) | gone |
| 2 (x=0) | Base places it; P1 edits it; **A deletes it later** | gone | gone |
| 3 (x=300) | Base places it, nobody touches it | a normal pile | a normal pile |
| markers (x=600, 900, 1200) | one new pile each from P1, A, P2 | three piles | three piles |

So: **five piles** (the big raised one at x=-300, the control, three markers) proves "last wins"; **four**
(no pile at x=-300) proves "a deletion sticks". `ori` on a pile shows its file and cell.

Do it in Morrowind.exe and in OpenMW (they can differ).

## If OpenMW shows five piles

Then OpenMW brings a reference back that Morrowind.exe keeps deleted, and the two engines differ.
Both of Wraithguard's readers use the Morrowind.exe rule today (`ordercore::refs`, Gardenfell's
`dead` set); which engine the viewer's picture should follow is then a decision to make, and Robin
should be told (`viewer-shell/gardenfell/patches/NOTES-FOR-ROBIN.md`, 0017).

## Not covered

A reference that is *absent* from a file (removed, not flagged DELE) is a different case; Gardenfell's
old comment says an earlier removal is undone by a later file that adds it back. Say so if you want
a fifth plugin for that.

## Variant T: the same four plugins, every reference temporary

Optional now: a cross-check that the section does not matter. `WG_DelT_Base.esm`, `WG_DelT_P1.esp`,
`WG_DelT_A.esp`, `WG_DelT_P2.esp` (written by `make_temporary_variant.py` from the files above: same
edits, cell "WG Delete Test T", and a `NAM0` before the first `FRMR` of every cell record, so every
reference is *temporary*, as nearly all of the vanilla exteriors' are). They can be enabled beside the
first set. In the same order, after Morrowind.esm: Base, P1, A, P2. Then `coc "WG Delete Test T"`,
`tcl`, look along +Y; piles at x = -300 (ref 1), 0 (ref 2), 300 (ref 3), and the markers at 600, 900,
1200.

| What you see | It means |
|---|---|
| **Four piles**, nothing at x=-300 | As expected: a deletion sticks for temporary references too. |
| **Five piles**, a big raised one at x=-300 | Surprising: a temporary reference is undeleted by a later record. Count again (the marker at the arrival point), and tell Robin. |
| Fewer than the three markers | A plugin did not load (the master's size is recorded as 642 bytes; a different size only warns). The run proves nothing. |

Do it in OpenMW too.
