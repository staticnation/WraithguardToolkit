# In-game test: does a later edit undelete a reference?

**Result (October 2026): yes, the last record wins.** Vanilla Morrowind.exe shows five piles, and
`ori` on them gives:

| sourceID | targetID | file |
|---|---|---|
| 02000001 | 01000001 | WG_Del_P2.esp (ref 1, the big raised pile: P2's edit after A's deletion) |
| 01000003 | | WG_Del_Base.esm (ref 3, the control) |
| 0180000a | | WG_Del_P1.esp (marker) |
| 01c0000b | | WG_Del_A.esp (marker) |
| 0200000c | | WG_Del_P2.esp (marker) |

Ref 2 (edited by P1, deleted later by A) is gone. OpenMW's code reads it the same way
(`apps/openmw/mwworld/cellstore.cpp`, `CellRefList<X>::load`: a later record for a reference replaces
the earlier one whole, and only a deleted record marks it deleted); the run in OpenMW is still to do.
So `ordercore::refs` and Gardenfell patch 0017 stand.

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

## If OpenMW shows four piles

The two engines would then differ: the Editor follows the game (Morrowind.exe), and the viewer's
Gardenfell follows OpenMW, so tell Robin (`viewer-shell/gardenfell/patches/NOTES-FOR-ROBIN.md`, 0017).

## Not covered

A reference that is *absent* from a file (removed, not flagged DELE) is a different case; Gardenfell's
old comment says an earlier removal is undone by a later file that adds it back. Say so if you want
a fifth plugin for that.
