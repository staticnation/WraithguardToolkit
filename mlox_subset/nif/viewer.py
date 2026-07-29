"""A self-contained 3D page for looking at one or two meshes.

Answers the question a triangle count cannot: *does the winner actually look
different?* Two meshes side by side, orbitable, is the shortest route from "this
conflict is flagged" to a decision.

**Why three.js is embedded as a classic script.** Modern three.js ships ESM
only, split across ``three.module.min.js`` and ``three.core.min.js``, and **ES
module scripts do not load from ``file://``** -- the origin is ``null`` and the
CORS check fails. These pages are written to disk and opened in a browser, so a
module build cannot work here regardless of how it is packaged. The CommonJS
build is a single self-contained file with no ``require()`` of its own, so it
runs as an ordinary script behind a three-line ``exports`` shim. That was
verified rather than assumed: the shim was exercised and used to build a real
``BufferGeometry`` with computed normals before any of this was written.

**Why the orbit controls are ours.** three.js ships ``OrbitControls.js``, but it
imports the bare specifier ``'three'``, which would drag ESM and an import map
back into a page that has just gone to some trouble to avoid both. Dragging to
rotate is forty lines; an import map that works from ``file://`` is not.

**Why the page is self-contained.** The existing visualisations are single files
a user can move, keep or send to someone. Referencing a sibling script would
break that, and would also behave differently in the in-app viewers
(``pywebview``, ``tkinterweb``) than in a browser. One file behaves the same
everywhere.
"""

from __future__ import annotations

import base64
import html
import json
import struct
import sys
import zlib
from collections.abc import Callable
from pathlib import Path
from typing import TYPE_CHECKING, Final

from mlox_subset.images import ImageError, browser_image
from mlox_subset.logging_setup import get_logger

if TYPE_CHECKING:
    from mlox_subset.nif.geometry import Mesh, TreeNode
    from mlox_subset.nif.textures import Resolved, TextureResolver

LOG = get_logger(__name__)

#: The vendored three.js build, relative to this package.
_THREE_ASSET: Final[str] = "assets/three.cjs"

#: Colours for the two sides of a comparison: the overridden mesh and the one
#: that wins. Deliberately not red and green -- the point is to tell them apart,
#: not to say which is better, and roughly 1 in 12 men cannot separate those.
_COLOURS: Final[tuple[str, str]] = ("#6ba3ff", "#ffb86b")


class ViewerError(Exception):
    """Raised when the viewer cannot be built."""


#: Turns one deflated blob into whatever the page should read it from: a
#: ``{"b64": ...}`` for a standalone file, or a ``{"url": ...}`` for a served
#: one. The page branches on which key is present, so the two modes share every
#: line of rendering code and cannot drift apart.
BlobSink = Callable[[bytes, str], dict[str, str]]


def inline_blob(blob: bytes, content_type: str = "") -> dict[str, str]:
    """Carry a blob inside the document.

    Args:
        blob: The bytes. Deflated for geometry, a PNG for a texture.
        content_type: The MIME type, used only when serving. Inline blobs that
            the page hands to an ``Image`` need it as a data URL prefix; the
            rest are read as raw bytes and do not.

    Returns:
        A base64 entry, or a data URL when one is needed.
    """
    encoded = base64.b64encode(blob).decode("ascii")
    if content_type.startswith("image/"):
        return {"url": f"data:{content_type};base64,{encoded}"}
    return {"b64": encoded}


def three_source() -> str:
    """Locate and read the vendored three.js build.

    Looks in the same places, and for the same reason, as the help documents:
    a frozen build unpacks its data to ``sys._MEIPASS``, while a source checkout
    has it beside this module.

    Returns:
        The library source.

    Raises:
        ViewerError: If it was not shipped with this build. Reported rather
            than crashed on: a missing viewer is a disappointment, and the
            caller can say so.
    """
    bundled = getattr(sys, "_MEIPASS", None)
    candidates = [
        *([Path(bundled) / "mlox_subset" / "nif" / _THREE_ASSET] if bundled else []),
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


def _packed(values: list[float] | list[int], fmt: str) -> bytes:
    """Pack numbers as a deflated, base64 binary blob.

    Measured on a 204k-triangle mesh, against writing the same numbers as JSON
    decimals:

    * JSON decimals -- 5.40 MB.
    * base64 typed arrays -- 4.91 MB. Almost no gain: base64 costs 33% and
      hands most of the binary saving straight back.
    * base64 of *deflated* typed arrays -- 1.86 MB, a third of the JSON.

    So the compression is doing the work, not the binary encoding, and it costs
    nothing on the page: browsers inflate this natively with
    ``DecompressionStream`` and no library.

    Args:
        values: The numbers to pack.
        fmt: A :mod:`struct` format character, ``f`` or ``I``.

    Returns:
        The deflated bytes. How they reach the page -- inline as base64, or
        over loopback as a fetch -- is the caller's decision, which is what
        lets one builder produce both a served page and a standalone file.
    """
    raw = struct.pack(f"<{len(values)}{fmt}", *values)
    return zlib.compress(raw, 6)


def texture_bytes(resolved: Resolved, resolver: TextureResolver) -> tuple[bytes, str] | None:
    """Turn a resolved texture into something a browser shows, or give up quietly.

    Read through the resolver rather than off the path directly, because most
    of the base game's textures are inside ``Morrowind.bsa`` and have no path
    at all.

    Args:
        resolved: The outcome of a texture lookup.
        resolver: The resolver that produced it, which knows how to fetch the
            bytes whether they are loose or archived.

    Returns:
        Bytes a browser can display and their MIME type, or ``None`` when
        nothing provides the texture or it is in a format the decoders do not
        handle. Both are ordinary in a mod collection, and a broken texture is
        a finding about the mod rather than a reason to fail the whole view.

        A PNG comes back as the bytes it arrived as -- browsers decode PNG
        better than anything here would, and re-encoding could only lose
        fidelity. Everything else is decoded and re-encoded as PNG.
    """
    if not resolved.found:
        return None
    raw = resolver.read(resolved)
    if raw is None:
        return None
    try:
        return browser_image(raw)
    except (ImageError, ValueError) as exc:
        LOG.debug("cannot decode %s: %s", resolved.reference, exc)
        return None


def _mesh_payload(
    meshes: list[Mesh],
    sink: BlobSink,
    resolver: TextureResolver | None = None,
    cache: dict[str, dict[str, str] | None] | None = None,
) -> list[dict[str, object]]:
    """Reduce meshes to the packed arrays the page needs.

    Positions are flattened before packing, because the page inflates straight
    into a ``Float32Array``: any nesting would only cost the browser a pass to
    undo.

    Args:
        meshes: World-space meshes.
        sink: Turns a blob into something the page can read -- base64 for a
            standalone file, a URL for a served one.
        resolver: Finds texture files across the data folders. Without one the
            meshes are sent untextured, which is a complete and useful view.
        cache: Decoded textures by reference, shared across sides. One texture
            is routinely used by many shapes and by both sides of a
            comparison; decoding a 2048px image once per shape would dominate
            the time to open a view.

    Returns:
        One entry per mesh, JSON-ready.
    """
    decoded: dict[str, dict[str, str] | None] = cache if cache is not None else {}
    payload: list[dict[str, object]] = []
    for mesh in meshes:
        if not mesh.triangles:
            continue
        positions: list[float] = []
        for vertex in mesh.vertices:
            positions.extend(vertex)
        indices: list[int] = []
        for triangle in mesh.triangles:
            indices.extend(triangle)
        uvs: list[float] = []
        # Only send UVs when there is one per vertex. A partial set would make
        # three.js index past the end of the attribute and draw nothing at all,
        # which is a worse outcome than an untextured mesh.
        if len(mesh.uvs) == len(mesh.vertices):
            for u, v in mesh.uvs:
                # NIF measures V downward and OpenGL upward, so an untouched
                # copy renders every texture upside down.
                uvs.extend((u, 1.0 - v))
        image: dict[str, str] | None = None
        extras: dict[str, dict[str, str]] = {}
        if resolver is not None and mesh.texture and uvs:
            if mesh.texture not in decoded:
                found = resolver.resolve(mesh.texture)
                shown = texture_bytes(found, resolver)
                decoded[mesh.texture] = sink(*shown) if shown else None
            image = decoded[mesh.texture]
            # The mesh names only its diffuse texture; OpenMW finds the rest by
            # name. Offering them is the only way a normal map in a texture
            # pack is ever visible here, since no NIF mentions one.
            for suffix, resolved in resolver.siblings(mesh.texture).items():
                key = f"{mesh.texture}{suffix}"
                if key not in decoded:
                    aux = texture_bytes(resolved, resolver)
                    decoded[key] = sink(*aux) if aux else None
                if decoded[key] is not None:
                    extras[suffix] = decoded[key]
        payload.append(
            {
                "name": mesh.name,
                "texture": mesh.texture,
                "image": image,
                "extras": extras,
                "positions": sink(_packed(positions, "f"), ""),
                "indices": sink(_packed(indices, "I"), ""),
                "uvs": sink(_packed(uvs, "f"), "") if uvs else None,
                "vertexCount": len(mesh.vertices),
                "triangleCount": len(mesh.triangles),
            }
        )
    return payload


def _tree_payload(nodes: list[TreeNode]) -> list[dict[str, object]]:
    """Reduce a block tree to JSON the page can render.

    Args:
        nodes: Roots from :func:`~mlox_subset.nif.geometry.block_tree`.

    Returns:
        Nested entries.
    """
    return [
        {
            "index": node.index,
            "type": node.type_name,
            "name": node.name,
            "note": node.note,
            "children": _tree_payload(node.children),
        }
        for node in nodes
    ]


def build_viewer_page(
    sides: list[tuple[str, list[Mesh]]],
    title: str = "Mesh viewer",
    *,
    sink: BlobSink | None = None,
    library_url: str = "",
    trees: list[list[TreeNode]] | None = None,
    resolver: TextureResolver | None = None,
) -> str:
    """Build an HTML page showing one or more meshes.

    Two shapes from one template. With no arguments beyond the meshes it
    produces a **standalone file**: three.js and every byte of geometry inline,
    portable, and multi-megabyte. Given a ``sink`` and a ``library_url`` it
    produces a **served page** of a few kilobytes that fetches both.

    The difference is confined to how bytes arrive. The rendering code is
    identical in both, which is the point: a fallback that shares no code with
    the primary path is a second implementation waiting to rot.

    Args:
        sides: ``(label, meshes)`` pairs. One side shows a single mesh; two
            shows a comparison, each in its own colour and its own viewport.
        title: The page title.
        sink: How geometry reaches the page. Defaults to inlining it.
        library_url: Where to fetch three.js. Empty means inline it.
        resolver: Finds texture files across the data folders. Omitted, the
            meshes render in a flat colour, which stays a complete view rather
            than a degraded one.
        trees: Block hierarchies, one per side. Optional because the geometry
            view stands on its own; supplied, it fills the structure pane with
            what a render cannot show -- collision nodes, controllers, and
            every block that never draws.

    Returns:
        The whole HTML document.

    Raises:
        ViewerError: If three.js is needed and missing.
    """
    blob_sink = sink or inline_blob
    library = "" if library_url else three_source()
    shared_textures: dict[str, dict[str, str] | None] = {}
    scenes = [
        {
            "label": label,
            "colour": _COLOURS[index % len(_COLOURS)],
            "meshes": _mesh_payload(meshes, blob_sink, resolver, shared_textures),
            "tree": _tree_payload(trees[index]) if trees and index < len(trees) else [],
        }
        for index, (label, meshes) in enumerate(sides)
    ]
    empty = all(not scene["meshes"] for scene in scenes)
    if empty:
        LOG.info("viewer built with no geometry: %s", title)
    # json.dumps escapes nothing HTML-significant by default, and the payload
    # carries mod-authored names. "</script>" inside a string would end the
    # element early, so the sequence is broken up rather than trusted.
    data = json.dumps(scenes, separators=(",", ":")).replace("</", "<\\/")
    # The CommonJS build needs its two globals to exist *before* it runs and
    # the namespace pulled back out *after*, whether it arrives inline or over
    # the wire. Serving it without the shim was a real bug: the file ran
    # against an undefined ``exports`` and the page reported "THREE is not
    # defined". Classic scripts execute in order, so three tags do for the
    # served case exactly what one does for the inline one.
    prologue = "<script>var module = {exports:{}}, exports = module.exports;</script>"
    epilogue = "<script>var THREE = module.exports;</script>"
    if library_url:
        middle = f'<script src="{html.escape(library_url, quote=True)}"></script>'
    else:
        middle = f"<script>\n{library}\n</script>"
    library_block = f"{prologue}\n{middle}\n{epilogue}"
    return (
        _PAGE.replace("__TITLE__", html.escape(title))
        .replace("__LIBRARY_BLOCK__", library_block)
        .replace("__DATA__", data)
        .replace("__EMPTY__", "true" if empty else "false")
    )


#: The page. Written as one template rather than assembled from fragments: it
#: is read far more often than it is edited, and a reader needs to see the
#: whole document to judge it.
_PAGE: Final[str] = """<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"><title>__TITLE__</title>
<style>
 :root{--ink:#e6e6e6;--dim:#9aa0aa;--line:#333945;--panel:#20242c}
 html,body{margin:0;height:100%;background:#15171c;color:var(--ink);
   font:13px/1.5 "Segoe UI",system-ui,sans-serif;display:flex;flex-direction:column}
 .panel{display:flex;flex-wrap:wrap;gap:6px 16px;align-items:center;
   margin:0;padding:8px 10px;border-bottom:1px solid var(--line);
   background:var(--panel);font-size:12.5px}
 .panel .ctl{display:flex;align-items:center;gap:5px}
 .panel .ctl.off{opacity:.45}
 .panel label{color:var(--dim);cursor:pointer}
 .panel .swatch{width:10px;height:10px;border-radius:2px;display:inline-block}
 .panel button{background:#2c313a;color:#d7dae0;border:1px solid var(--line);
   border-radius:4px;padding:4px 9px;cursor:pointer;font:inherit}
 .panel button:hover{background:#3d4450}
 .panel .spacer{flex:1}
 .panel input[type=range]{width:78px;accent-color:#6f8fb8;cursor:pointer}
 #body{flex:1;display:flex;min-height:0}
 #tree{width:300px;min-width:160px;max-width:50%;overflow:auto;padding:8px 10px;
   border-right:1px solid var(--line);background:#181b21;font-size:12px;
   font-family:Consolas,"Cascadia Mono",monospace;white-space:nowrap;resize:horizontal}
 #tree ul{list-style:none;margin:0;padding-left:14px}
 #tree>ul{padding-left:0}
 #tree li{line-height:1.55}
 #tree .ty{color:#9ecbff}
 #tree .nm{color:var(--ink)}
 #tree .no{color:var(--dim)}
 #tree .ix{color:#5b6270;margin-right:5px}
 #tree h4{margin:10px 0 3px;font:600 11px/1.4 "Segoe UI",system-ui,sans-serif;
   color:var(--dim);text-transform:uppercase;letter-spacing:.05em}
 #tree h4:first-child{margin-top:0}
 #stage{flex:1;position:relative;min-height:0}
 canvas{display:block}
 #stats{position:absolute;bottom:8px;left:10px;opacity:.75;pointer-events:none}
 #hint{position:absolute;bottom:8px;right:10px;opacity:.55;pointer-events:none}
 #none{position:absolute;inset:0;display:flex;align-items:center;justify-content:center;
   text-align:center;padding:2rem;opacity:.8}
</style></head><body>
<div class="panel" id="controls"></div>
<div id="body">
  <div id="tree"></div>
  <div id="stage">
    <div id="stats"></div>
    <div id="hint">drag to orbit &middot; wheel to zoom</div>
  </div>
</div>
__LIBRARY_BLOCK__
<script>
(function () {
  var scenes = __DATA__;
  var stage = document.getElementById("stage");
  var controls = document.getElementById("controls");
  if (__EMPTY__) {
    stage.innerHTML = '<div id="none">No geometry could be read from this mesh.' +
      '<br>That is a limit of the reader, not a statement about the file.</div>';
    return;
  }

  function inflateBytes(bytes) {
    if (typeof DecompressionStream === "undefined") {
      return Promise.reject(new Error("this browser has no DecompressionStream"));
    }
    var stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream("deflate"));
    return new Response(stream).arrayBuffer();
  }

  // A blob is either carried in the document or fetched from the loopback
  // server. Everything after this point is identical either way.
  function load(ref) {
    if (ref.url) {
      return fetch(ref.url).then(function (r) {
        if (!r.ok) throw new Error("could not fetch geometry (" + r.status + ")");
        return r.arrayBuffer();
      }).then(function (buf) { return inflateBytes(new Uint8Array(buf)); });
    }
    var bin = atob(ref.b64), bytes = new Uint8Array(bin.length);
    for (var i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    return inflateBytes(bytes);
  }

  function unpack(spec) {
    return Promise.all(spec.meshes.map(function (m) {
      var jobs = [load(m.positions), load(m.indices)];
      jobs.push(m.uvs ? load(m.uvs) : Promise.resolve(null));
      return Promise.all(jobs).then(function (bufs) {
        m.positions = new Float32Array(bufs[0]);
        m.indices = new Uint32Array(bufs[1]);
        m.uvs = bufs[2] ? new Float32Array(bufs[2]) : null;
        return m;
      });
    }));
  }

  Promise.all(scenes.map(unpack)).then(build).catch(function (err) {
    stage.innerHTML = '<div id="none">Could not decode the geometry in this page.' +
      '<br>' + String(err && err.message ? err.message : err) + '</div>';
  });

  function build() {
    var renderer = new THREE.WebGLRenderer({antialias: true});
    renderer.setPixelRatio(window.devicePixelRatio || 1);
    stage.appendChild(renderer.domElement);
    var scene = new THREE.Scene();
    scene.background = new THREE.Color(0x15171c);
    var camera = new THREE.PerspectiveCamera(45, 1, 0.1, 200000);

    // One viewport, one camera, one set of geometry groups toggled on and off.
    // Separate panes each framed their own mesh independently, which is the
    // one thing a comparison must not do: two objects at different scales look
    // identical when each is fitted to its own viewport.
    var pivot = new THREE.Group();
    var groups = [];
    scene.add(pivot);

    var textured = true;
    // Whether anything in this view has an OpenMW auxiliary normal map, which
    // decides whether the control for them is worth offering at all.
    var anyNormalMaps = false;
    scenes.forEach(function (spec, index) {
      // The inner group carries the Z-up to Y-up rotation; the outer pivot
      // carries the centring. They cannot be the same object: three.js
      // composes T*R*S, so a position set from a centre measured before the
      // rotation leaves the mesh at R*v - centre rather than R*(v - centre).
      var group = new THREE.Group();
      spec.meshes.forEach(function (m) {
        var g = new THREE.BufferGeometry();
        g.setAttribute("position", new THREE.BufferAttribute(m.positions, 3));
        g.setIndex(new THREE.BufferAttribute(m.indices, 1));
        if (m.uvs) g.setAttribute("uv", new THREE.BufferAttribute(m.uvs, 2));
        // The files carry normals, but not always, and a mesh with none
        // renders flat black. Computing them is cheap and always right.
        g.computeVertexNormals();
        var material = new THREE.MeshPhongMaterial({
          color: spec.colour, side: THREE.DoubleSide
        });
        if (m.image && m.uvs && textured) {
          var image = new Image();
          var tex = new THREE.Texture(image);
          tex.colorSpace = THREE.SRGBColorSpace;
          tex.wrapS = tex.wrapT = THREE.RepeatWrapping;
          // The image arrives after the material is built, so the texture has
          // to be marked dirty and a frame drawn once it lands. Without the
          // redraw the mesh stays flat-coloured until something else happens
          // to trigger one, which looks exactly like a failure to load.
          image.onload = function () { tex.needsUpdate = true; draw(); };
          image.src = m.image.url;
          material.map = tex;
          // Tinting a texture with the side colour would make the two
          // providers look different for a reason that is not in the file.
          material.color = new THREE.Color(0xffffff);
        }
        // An OpenMW-style normal map, found beside the diffuse one by name.
        // Not colour: it is loaded in linear space, because treating a field
        // of vectors as sRGB bends every one of them.
        var extras = m.extras || {};
        var normalSource = extras["_nh"] || extras["_n"] || null;
        var normalTex = null;
        if (normalSource && m.uvs && textured) {
          var nImage = new Image();
          normalTex = new THREE.Texture(nImage);
          normalTex.wrapS = normalTex.wrapT = THREE.RepeatWrapping;
          nImage.onload = function () { normalTex.needsUpdate = true; draw(); };
          nImage.src = normalSource.url;
        }
        var drawn = new THREE.Mesh(g, material);
        drawn.userData.map = material.map || null;
        drawn.userData.normalMap = normalTex;
        drawn.userData.tint = spec.colour;
        if (normalTex) anyNormalMaps = true;
        group.add(drawn);
      });
      group.rotation.x = -Math.PI / 2;
      group.visible = index === 0;
      pivot.add(group);
      groups.push(group);
    });

    pivot.updateMatrixWorld(true);
    // Framed over *every* provider, visible or not, so toggling never moves
    // the camera. A comparison where the view shifts as you switch is a
    // comparison of two different pictures.
    var box = new THREE.Box3();
    groups.forEach(function (g) { box.expandByObject(g); });
    var centre = box.getCenter(new THREE.Vector3());
    var radius = Math.max(box.getSize(new THREE.Vector3()).length() / 2, 1e-3);
    if (!isFinite(radius) || box.isEmpty()) { centre.set(0, 0, 0); radius = 1; }
    pivot.position.copy(centre).negate();
    pivot.updateMatrixWorld(true);

    // Lighting is not decoration here. A normal map changes nothing at all
    // under flat ambient light -- the whole point of one is how it catches a
    // light that moves -- so these are the controls that make a normal map
    // comparison possible.
    var ambient = new THREE.AmbientLight(0xffffff, 0.55);
    scene.add(ambient);
    var key = new THREE.DirectionalLight(0xffffff, 1.6);
    key.position.set(1, 1.4, 1);
    scene.add(key);
    var fill = new THREE.DirectionalLight(0xffffff, 0.5);
    fill.position.set(-1, -0.6, -0.8);
    scene.add(fill);

    var lightState = {ambient: 0.55, key: 1.6, angle: 0.0, headlamp: false};

    function placeLights() {
      ambient.intensity = lightState.ambient;
      key.intensity = lightState.key;
      fill.intensity = lightState.key * 0.31;
      if (lightState.headlamp) {
        // From the camera's own position, so a surface is always lit from
        // wherever you are looking. Useful for reading a normal map's detail,
        // and deliberately *not* the default: it means moving the camera
        // changes the lighting, so two providers can never be compared under
        // identical light while it is on.
        key.position.copy(camera.position);
        fill.position.copy(camera.position).negate();
        return;
      }
      var a = lightState.angle;
      key.position.set(Math.sin(a), 1.4, Math.cos(a));
      fill.position.set(-Math.sin(a), -0.6, -Math.cos(a));
    }

    var treeBox = document.getElementById("tree");

    function renderTree(nodes) {
      var ul = document.createElement("ul");
      nodes.forEach(function (node) {
        var li = document.createElement("li");
        var index = document.createElement("span");
        index.className = "ix"; index.textContent = node.index;
        var type = document.createElement("span");
        type.className = "ty"; type.textContent = node.type;
        li.appendChild(index); li.appendChild(type);
        if (node.name) {
          var nm = document.createElement("span");
          nm.className = "nm"; nm.textContent = " " + node.name;
          li.appendChild(nm);
        }
        if (node.note) {
          var no = document.createElement("span");
          no.className = "no"; no.textContent = "  " + node.note;
          li.appendChild(no);
        }
        if (node.children && node.children.length) li.appendChild(renderTree(node.children));
        ul.appendChild(li);
      });
      return ul;
    }

    function refreshTree() {
      treeBox.textContent = "";
      var any = false;
      scenes.forEach(function (spec, i) {
        if (!groups[i].visible || !spec.tree || !spec.tree.length) return;
        any = true;
        var heading = document.createElement("h4");
        heading.textContent = spec.label;
        heading.style.color = spec.colour;
        treeBox.appendChild(heading);
        treeBox.appendChild(renderTree(spec.tree));
      });
      if (!any) {
        var note = document.createElement("div");
        note.className = "no";
        note.textContent = "no structure to show";
        treeBox.appendChild(note);
      }
    }

    var statsBox = document.getElementById("stats");
    function refresh() {
      var shapes = 0, tris = 0, verts = 0, shown = 0;
      scenes.forEach(function (spec, i) {
        if (!groups[i].visible) return;
        shown++;
        spec.meshes.forEach(function (m) {
          shapes++; tris += m.triangleCount; verts += m.vertexCount;
        });
      });
      statsBox.textContent = shown
        ? shapes + " shape(s), " + tris + " triangles, " + verts + " vertices"
        : "nothing shown";
      Array.prototype.forEach.call(controls.querySelectorAll(".ctl"), function (el, i) {
        if (i < groups.length) el.className = "ctl" + (groups[i].visible ? "" : " off");
      });
      refreshTree();
      draw();
    }

    scenes.forEach(function (spec, index) {
      var id = "side" + index;
      var span = document.createElement("span");
      span.className = "ctl" + (index === 0 ? "" : " off");
      var box2 = document.createElement("input");
      box2.type = "checkbox"; box2.id = id; box2.checked = index === 0;
      var label = document.createElement("label");
      label.htmlFor = id;
      var swatch = document.createElement("span");
      swatch.className = "swatch"; swatch.style.background = spec.colour;
      label.appendChild(swatch);
      label.appendChild(document.createTextNode(" " + spec.label));
      span.appendChild(box2); span.appendChild(label);
      controls.appendChild(span);
      box2.addEventListener("change", function () {
        groups[index].visible = box2.checked;
        refresh();
      });
    });

    var textureBox = document.createElement("input");
    textureBox.type = "checkbox"; textureBox.id = "textured"; textureBox.checked = true;
    var textureCtl = document.createElement("span");
    textureCtl.className = "ctl";
    var textureLabel = document.createElement("label");
    textureLabel.htmlFor = "textured";
    textureLabel.textContent = "Textures";
    textureCtl.appendChild(textureBox); textureCtl.appendChild(textureLabel);
    controls.appendChild(textureCtl);
    textureBox.addEventListener("change", function () {
      // Flat colour is often the better comparison: two versions of a mesh
      // wearing the same texture differ in shape, and the texture hides it.
      scene.traverse(function (o) {
        if (!o.isMesh || !o.userData.map) return;
        o.material.map = textureBox.checked ? o.userData.map : null;
        o.material.color = new THREE.Color(textureBox.checked ? 0xffffff : o.userData.tint);
        o.material.needsUpdate = true;
      });
      textureCtl.className = "ctl" + (textureBox.checked ? "" : " off");
      draw();
    });

    function addSlider(id, label, min, max, value, step, onInput) {
      var ctl = document.createElement("span");
      ctl.className = "ctl";
      var text = document.createElement("label");
      text.htmlFor = id; text.textContent = label;
      var range = document.createElement("input");
      range.type = "range"; range.id = id;
      range.min = min; range.max = max; range.step = step; range.value = value;
      range.addEventListener("input", function () {
        onInput(parseFloat(range.value));
        placeLights();
        draw();
      });
      ctl.appendChild(text); ctl.appendChild(range);
      controls.appendChild(ctl);
      return range;
    }

    addSlider("lightkey", "Light", 0, 4, lightState.key, 0.05,
      function (v) { lightState.key = v; });
    addSlider("lightamb", "Ambient", 0, 2, lightState.ambient, 0.05,
      function (v) { lightState.ambient = v; });
    var angleRange = addSlider("lightang", "Angle", 0, 6.2832, lightState.angle, 0.02,
      function (v) { lightState.angle = v; });

    var lampBox = document.createElement("input");
    lampBox.type = "checkbox"; lampBox.id = "headlamp";
    var lampCtl = document.createElement("span");
    lampCtl.className = "ctl off";
    var lampLabel = document.createElement("label");
    lampLabel.htmlFor = "headlamp";
    lampLabel.textContent = "Follow camera";
    lampCtl.appendChild(lampBox); lampCtl.appendChild(lampLabel);
    controls.appendChild(lampCtl);
    lampBox.addEventListener("change", function () {
      lightState.headlamp = lampBox.checked;
      // The fixed-angle slider means nothing while the light tracks the
      // camera, so it is disabled rather than left to look operative.
      angleRange.disabled = lampBox.checked;
      lampCtl.className = "ctl" + (lampBox.checked ? "" : " off");
      placeLights();
      draw();
    });

    // Only offered when the collection actually ships one. A permanently
    // dead control implies the feature is broken rather than unused.
    if (anyNormalMaps) {
      var normalBox = document.createElement("input");
      normalBox.type = "checkbox"; normalBox.id = "normals";
      var normalCtl = document.createElement("span");
      normalCtl.className = "ctl off";
      var normalLabel = document.createElement("label");
      normalLabel.htmlFor = "normals";
      normalLabel.textContent = "Normal maps";
      normalCtl.appendChild(normalBox); normalCtl.appendChild(normalLabel);
      controls.appendChild(normalCtl);
      normalBox.addEventListener("change", function () {
        scene.traverse(function (o) {
          if (!o.isMesh || !o.userData.normalMap) return;
          o.material.normalMap = normalBox.checked ? o.userData.normalMap : null;
          o.material.needsUpdate = true;
        });
        normalCtl.className = "ctl" + (normalBox.checked ? "" : " off");
        draw();
      });
    }

    var spacer = document.createElement("span");
    spacer.className = "spacer";
    controls.appendChild(spacer);
    [["Show all", function () { groups.forEach(function (g) { g.visible = true; }); }],
     ["Reset view", function () { yaw = 0.6; pitch = 0.5; distance = radius * 3; }]
    ].forEach(function (pair) {
      var button = document.createElement("button");
      button.type = "button";
      button.textContent = pair[0];
      button.addEventListener("click", function () {
        pair[1]();
        Array.prototype.forEach.call(controls.querySelectorAll("input"), function (cb, i) {
          if (i < groups.length) cb.checked = groups[i].visible;
        });
        place(); refresh();
      });
      controls.appendChild(button);
    });

    var yaw = 0.6, pitch = 0.5, distance = radius * 3;
    function place() {
      camera.position.set(
        distance * Math.cos(pitch) * Math.sin(yaw),
        distance * Math.sin(pitch),
        distance * Math.cos(pitch) * Math.cos(yaw));
      camera.lookAt(0, 0, 0);
      // In headlamp mode the light rides the camera, so it has to move here
      // rather than only when a slider changes.
      placeLights();
    }
    function draw() { renderer.render(scene, camera); }
    function resize() {
      var w = stage.clientWidth, h = stage.clientHeight;
      renderer.setSize(w, h, false);
      camera.aspect = w / Math.max(h, 1);
      camera.updateProjectionMatrix();
    }
    var dragging = false, lastX = 0, lastY = 0;
    renderer.domElement.addEventListener("mousedown", function (e) {
      dragging = true; lastX = e.clientX; lastY = e.clientY;
    });
    window.addEventListener("mouseup", function () { dragging = false; });
    window.addEventListener("mousemove", function (e) {
      if (!dragging) return;
      yaw -= (e.clientX - lastX) * 0.01;
      pitch += (e.clientY - lastY) * 0.01;
      // Stop short of the poles: at exactly +/-90 degrees the look-at up
      // vector is parallel to the view and the image flips.
      pitch = Math.max(-1.55, Math.min(1.55, pitch));
      lastX = e.clientX; lastY = e.clientY;
      place(); draw();
    });
    renderer.domElement.addEventListener("wheel", function (e) {
      e.preventDefault();
      distance *= (e.deltaY > 0) ? 1.1 : 0.9;
      distance = Math.max(radius * 0.05, Math.min(radius * 60, distance));
      place(); draw();
    }, {passive: false});
    window.addEventListener("resize", function () { resize(); draw(); });
    resize(); place(); refresh();
  }
})();
</script>
</body></html>
"""
