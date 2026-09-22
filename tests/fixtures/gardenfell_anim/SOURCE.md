# Animation NIF fixtures

Tiny hand-crafted Morrowind NIFs from **Gardenfell / GrassForge** © 2026 Robin
Hjelte (MIT; its MGE XE shader parts are not used here), each exercising one
animation type. Used to test `wraithguard/nif` animation extraction against real
NIF bytes rather than hand-built blocks.

- `uvsets.nif` -- NiUVController (scrolling/scaling texture).
- `anim.nif`   -- NiKeyframeController (node sway/spin).
- `morph.nif`  -- NiGeomMorpherController (cloth vertex morph), shapes nested
                  under a NiCollisionSwitch.
- `particle_move.nif` -- a NiTriShape under a NiBSParticleNode (particle cloud).
- `flap.nif` + `flap.kf` -- external KF animation (KF loading not yet implemented).
