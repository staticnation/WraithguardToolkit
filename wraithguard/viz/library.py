"""Locating and reading the vendored three.js build shared by every viewer.

This lives in :mod:`wraithguard.viz` alongside the conflict-map, height-delta,
pathgrid and terrain renderers because it solves the same problem one level
down. None of those four needs a 3D engine, but two other pages do -- the mesh
viewer in :mod:`wraithguard.nif.viewer` and the texture comparison's WebGL
wipe in :mod:`wraithguard.images.viewer` -- and a second consumer is what
turned "load one file" from something worth inlining into
:mod:`~wraithguard.nif.viewer` into a concern worth naming on its own.
:mod:`~wraithguard.viz.serve` exists for the adjacent reason of publishing
this same build once per session instead of re-embedding it in every
document, and the two belong together for that reason, not because either
one is a "page" in the sense the rest of this package is -- see the package
docstring for where the pure-renderer guarantee does and does not reach.

**Why three.js is embedded as a classic script.** Modern three.js ships ESM
only, split across ``three.module.min.js`` and ``three.core.min.js``, and **ES
module scripts do not load from ``file://``** -- the origin is ``null`` and the
CORS check fails. Every page built here is written to disk or served and then
opened in a browser, so a module build cannot work regardless of how it is
packaged. The CommonJS build is a single self-contained file with no
``require()`` of its own, so it runs as an ordinary script behind a
three-line ``exports`` shim. That was verified rather than assumed: the shim
was exercised and used to build a real ``BufferGeometry`` with computed
normals before any of this was written.
"""

from __future__ import annotations

import sys
from pathlib import Path
from typing import Final

from wraithguard.logging_setup import get_logger

LOG = get_logger(__name__)

#: The vendored three.js build, relative to this package.
_THREE_ASSET: Final[str] = "assets/three.cjs"


class ViewerError(Exception):
    """Raised when a viewer page cannot be built.

    Not specific to any one viewer -- the mesh viewer and the texture
    comparison both raise this through :func:`three_source`, and either could
    grow its own reasons to raise it later.
    """


def three_source() -> str:
    """Locate and read the vendored three.js build.

    Looks in the same places, and for the same reason, as the help documents:
    a frozen build unpacks its data to ``sys._MEIPASS``, while a source
    checkout has it beside this module.

    Returns:
        The library source.

    Raises:
        ViewerError: If it was not shipped with this build. Reported rather
            than crashed on: a missing viewer is a disappointment, and the
            caller can say so.
    """
    bundled = getattr(sys, "_MEIPASS", None)
    candidates = [
        *([Path(bundled) / "wraithguard" / "viz" / _THREE_ASSET] if bundled else []),
        *([Path(bundled) / _THREE_ASSET] if bundled else []),
        Path(__file__).resolve().parent / _THREE_ASSET,
    ]
    found = _first_readable(candidates)
    if found is not None:
        return found
    raise ViewerError(
        "the 3D viewer library was not shipped with this build; "
        f"looked in {[str(c) for c in candidates]}"
    )


def _first_readable(candidates: list[Path]) -> str | None:
    """Return the contents of the first candidate that can be read.

    Args:
        candidates: Paths to try, in order.

    Returns:
        The file's text, or ``None`` when none of them could be read.
    """
    for candidate in candidates:
        try:
            if candidate.is_file():
                return candidate.read_text(encoding="utf-8")
        except OSError as exc:  # noqa: PERF203 -- candidates must fail independently
            # An unreadable mount or a partial extraction. Hoisting the try out
            # of the loop would make one bad path skip the remaining ones, and
            # the whole purpose here is to try them in turn.
            LOG.warning("cannot read %s: %s", candidate, exc)
    return None


#: The JavaScript that layers Morrowind's extra texture slots onto three.js's
#: own Phong shader, shared by the mesh viewer and the texture comparison.
#:
#: **Why this is a string and not a module.** Both consumers assemble a single
#: self-contained HTML document -- there is no bundler, no import graph, and no
#: second file to fetch. The only way two pages can share JavaScript here is to
#: share the source text, so it lives beside the library it patches.
#:
#: **Why it patches three.js rather than replacing it.** Everything below feeds
#: the existing lighting model; none of it touches lighting itself. A shader
#: written from scratch would put the lighting at risk to gain nothing, and
#: getting Phong subtly wrong is far harder to notice than getting a texture
#: layer wrong.
#:
#: **The slots, and where each number comes from.** Layer semantics are from
#: the Morrowind NIF Notes' field walk and mixing schematic:
#:
#: * *detail* -- "multiply 2X", Direct3D's MODULATE2X: base and detail are
#:   multiplied and the **result doubled**. The doubling is what gives the mode
#:   a neutral value, so a half-brightness detail map leaves the base alone.
#: * *dark* -- a plain multiply, ``base.rgb * dark.rgb``. No neutral value; it
#:   can only darken, which is what its name says and what detail is not.
#: * *decals* -- alpha-composited in slot order, each over the running result.
#:   A decal replaces what is beneath it rather than tinting it.
#: * *gloss* -- attenuates specular strength.
#:
#: **What the caller supplies.** ``wants(slot)`` decides whether a layer draws
#: on this compile, and the maps are read off ``drawn.userData``. Neither is
#: hard-wired to a control, because the two pages have entirely different ones
#: -- the mesh viewer has a checkbox per slot, the texture comparison has three
#: toggles over filename-derived siblings.
EXTRA_SLOTS_JS: Final[str] = r"""
    // Attach Morrowind's extra texture slots to a MeshPhongMaterial.
    //
    // `wants(slot)` is asked at *compile* time, not at attach time, and the
    // cache key below is what makes that work.
    function attachExtraSlots(material, drawn, wants) {
      var decalsOf = function () { return drawn.userData.decalMaps || []; };
      var has = drawn.userData.detailMap || drawn.userData.darkMap
        || drawn.userData.glossMap || decalsOf().length;
      if (!has) return;
      // needsUpdate alone does not guarantee onBeforeCompile runs again with
      // fresh state: three.js keys its compiled-program cache off material
      // defines and a handful of properties it already knows about, none of
      // which mention these toggles, so it can and does silently reuse the
      // very first compiled program forever. This is the documented escape
      // hatch -- folding the toggle states into the cache key makes "detail
      // on" and "detail off" look like genuinely different programs, which is
      // what actually forces a recompile.
      material.customProgramCacheKey = function () {
        return [
          drawn.userData.detailMap && wants("detail") ? "D" : "",
          drawn.userData.darkMap && wants("dark") ? "K" : "",
          drawn.userData.glossMap && wants("gloss") ? "G" : "",
          // The *count* is in the key, not just a flag: a shape with two
          // decals compiles a different program from one with three.
          decalsOf().length && wants("decal") ? "C" + decalsOf().length : ""
        ].join("");
      };
      material.onBeforeCompile = function (shader) {
        // vMapUv is not a given: three.js gives every map its own
        // conditionally declared varying (vMapUv for USE_MAP, vNormalMapUv for
        // USE_NORMALMAP, ...) rather than one shared vUv, and vUv itself is
        // declared only under USE_UV or USE_ANISOTROPY, neither of which these
        // slots turn on by themselves. vMapUv is what the base map's own
        // map_fragment chunk uses, immediately above where this is inserted,
        // so it is in scope there -- at the cost of depending on the material
        // having a base map at all. The #ifdef at the bottom is what makes
        // that dependency safe rather than a latent crash.
        var uniformDecl = "";
        var mapExtra = "";
        var specExtra = "";
        if (drawn.userData.detailMap) {
          shader.uniforms.detailMap = { value: drawn.userData.detailMap };
          uniformDecl += "uniform sampler2D detailMap;\n";
          // "Multiply 2X" -- the doubling is on the color, not the texture
          // coordinates. Sampling at scaled UVs instead leaves the layer with
          // no neutral value, so it could only ever darken.
          mapExtra += wants("detail")
            ? "diffuseColor.rgb *= texture2D(detailMap, vMapUv).rgb * 2.0;\n" : "";
        }
        if (drawn.userData.darkMap) {
          shader.uniforms.darkMap = { value: drawn.userData.darkMap };
          uniformDecl += "uniform sampler2D darkMap;\n";
          // A plain multiply, deliberately without the doubling detail gets.
          mapExtra += wants("dark")
            ? "diffuseColor.rgb *= texture2D(darkMap, vMapUv).rgb;\n" : "";
        }
        // One uniform per decal rather than an array: a sampler array indexed
        // by a loop variable is not portable GLSL ES 1.0, and the counts are
        // small enough that unrolling costs nothing.
        var decalMaps = decalsOf();
        if (decalMaps.length) {
          var showDecals = wants("decal");
          decalMaps.forEach(function (tex, slot) {
            var uniform = "decalMap" + slot;
            shader.uniforms[uniform] = { value: tex };
            uniformDecl += "uniform sampler2D " + uniform + ";\n";
            // Mixed against the running result, so slot order is paint order
            // and the last declared ends up on top.
            mapExtra += showDecals
              ? "{ vec4 d = texture2D(" + uniform + ", vMapUv);\n"
                + "diffuseColor.rgb = mix(diffuseColor.rgb, d.rgb, d.a); }\n"
              : "";
          });
        }
        if (drawn.userData.glossMap) {
          shader.uniforms.glossMap = { value: drawn.userData.glossMap };
          uniformDecl += "uniform sampler2D glossMap;\n";
          // A single-channel mask attenuating the highlight. The channel read
          // is chosen by the caller: a vanilla gloss map is luminance in red,
          // while OpenMW's _diffusespec packs specular intensity into alpha.
          specExtra += wants("gloss")
            ? "specularStrength *= texture2D(glossMap, vMapUv)."
              + (drawn.userData.glossChannel || "r") + ";\n" : "";
        }
        shader.fragmentShader = uniformDecl + shader.fragmentShader;
        // Everything above samples vMapUv, which exists only under USE_MAP.
        // That is not merely a property of the file: turning the base map off
        // undefines USE_MAP and forces a recompile, and without this guard the
        // injected code would reference a varying that no longer exists -- the
        // shader fails to compile and the surface renders as nothing, from a
        // toggle combination a user reaches in two clicks.
        if (mapExtra) {
          shader.fragmentShader = shader.fragmentShader.replace(
            "#include <map_fragment>",
            "#include <map_fragment>\n#ifdef USE_MAP\n" + mapExtra + "#endif\n"
          );
        }
        if (specExtra) {
          shader.fragmentShader = shader.fragmentShader.replace(
            "#include <specularmap_fragment>",
            "#include <specularmap_fragment>\n#ifdef USE_MAP\n" + specExtra + "#endif\n"
          );
        }
      };
    }
"""
