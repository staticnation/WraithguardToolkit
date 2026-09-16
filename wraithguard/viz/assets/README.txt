three.js r186, bundled from the upstream ESM sources into one CommonJS file.
MIT licensed; the licence text sits beside this file as three-LICENSE.txt.

Not upstream's own build. r186 removed the self-contained build/three.cjs (it is
now a stub that require()s the ESM module), and the viewer runs three.js as a
classic script from file://, where ES modules cannot load. So this file is
built by tools/build_three_cjs.py: it concatenates upstream's unmodified
three.module.js graph into a single module.exports with esbuild -- packaging
only, no minify, no source transform. Rerun that tool to reproduce or update it.
