# Construction Set markers

From **R-Zero's Construction Tools** by R-Zero aka Reizeron (Nexus Mods, Morrowind mod
47908, May Modathon 2020), inspired by LondonRook's Outlander mod markers. The author's
permission: "Do whatever you want, really."

| File | From | Used by the cell viewer for |
|---|---|---|
| `marker_arrow.nif` | Door marker and Travel marker replacer | a door's landing spot (Door links overlay, Editor markers) |
| `marker_travel.nif` | Door marker and Travel marker replacer | travel markers (Editor markers) |
| `marker_creature.nif` | Leveled Creature marker direction fix | leveled-creature spawn points, facing the way they spawn |
| `marker_character.nif` | R-Zero's Construction Tools (`R0\`) | the character gauge at the measuring tape's first point |
| `marker_ruler.nif` | R-Zero's Construction Tools (`R0\`) | the ruler along the measuring tape (700 units: 10 m, a division 10 cm) |

Built into the engine (`viewcore::markers`) and asked for as `__wg/<file>`.

The set's `marker_info.nif` is not carried: the engine's reader finds nothing to draw in it.
