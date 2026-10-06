three.js r186, bundled from the upstream ESM sources into one CommonJS file.
MIT licensed; the licence text sits beside this file as three-LICENSE.txt.

Not upstream's own build. r186 removed the self-contained build/three.cjs (it is
now a stub that require()s the ESM module), and the viewer runs three.js as a
classic script from file://, where ES modules cannot load. So this file is
built by tools/build_three_cjs.py: it concatenates upstream's unmodified
three.module.js graph into a single module.exports with esbuild -- packaging
only, no minify, no source transform. Rerun that tool to reproduce or update it.

mermaid.min.js is mermaid 12.1.0 (Knut Sveidqvist and contributors), MIT licensed;
the licence text sits beside it as mermaid-LICENSE.txt. It is upstream's own
dist/mermaid.min.js from the npm package, unmodified (SHA-256
6484afc32872a3aa16cac9a76ba1816a1ed4cc870a6593cc2e17757750f518b2): a classic
script that sets a global `mermaid`, so the Lua flowcharts and call graphs draw
from the loopback server or from a file opened from disk, with no connection.
To update it: npm pack mermaid@<version>, copy package/dist/mermaid.min.js and
package/LICENSE here, and change the version and hash above and in CREDITS.md.
