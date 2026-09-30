# Animation NIF fixtures

Tiny hand-crafted Morrowind NIFs from **Gardenfell / GrassForge** © 2026 Robin
Hjelte (MIT), each exercising one
animation type. They first tested our own NIF reader's animation extraction; now
they test the crate-based mesh readers against real NIF bytes (`native/src/nif.rs`,
`viewer-shell/viewcore/src/nif/from_crate.rs`, `tests/test_nif_inspect.py`).

- `uvsets.nif` -- NiUVController (scrolling/scaling texture).
- `anim.nif`   -- NiKeyframeController (node sway/spin).
- `morph.nif`  -- NiGeomMorpherController (cloth vertex morph), shapes nested
                  under a NiCollisionSwitch.
- `particle_move.nif` -- a NiTriShape under a NiBSParticleNode (particle cloud).
- `flap.nif` + `flap.kf` -- external KF animation.
