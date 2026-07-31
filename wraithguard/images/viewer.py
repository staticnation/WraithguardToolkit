"""A page for looking at two versions of a texture side by side.

The numbers in :class:`~wraithguard.images.compare.Comparison` say *how much*
two textures differ. They cannot say whether the difference matters, because
that depends on what the user is trying to decide -- and a 3% pixel change is a
watermark in one texture and a whole retexture in another. So the numbers pick
the pairs worth looking at, and this shows them.

**Three views of the same pair, because each answers a different question.**

* *Side by side* answers "which do I prefer".
* *Overlay* -- one on top of the other with a wipe -- answers "what moved",
  and is the one that finds a subtle change a side-by-side view hides, because
  the eye is far better at detecting motion than at comparing two things it
  must look back and forth between.
* *Difference* answers "where is the change", and is the only one that shows a
  change too small to see at all.

**Amplification is a display choice and is exposed as one.** A real difference
between two versions of a texture is often two or three levels, which renders
as black. Multiplying it makes it visible but no longer proportional, so the
control is in the user's hands with the factor shown, rather than a fixed
constant that quietly decides what counts as different.

Pixels are drawn with smoothing off. A texture comparison is about the pixels
that are there, and a browser's default interpolation invents new ones that
differ from *both* originals -- which is precisely the wrong thing when the
question is which of two files a user wants.
"""

from __future__ import annotations

import base64
import html
import json
from collections.abc import Callable
from typing import TYPE_CHECKING

from wraithguard.images.compare import Verdict
from wraithguard.logging_setup import get_logger

if TYPE_CHECKING:
    from wraithguard.images.compare import Comparison

LOG = get_logger(__name__)

#: How a blob is published. Either a data URL for a standalone file or a
#: loopback URL for a served page -- the same contract the mesh viewer uses,
#: so one builder produces both.
BlobSink = Callable[[bytes, str], dict[str, str]]

#: A texture's auxiliary maps, keyed by the OpenMW suffix that found them:
#: ``"_n"``, ``"_nh"``, ``"_spec"``. Each value is the displayable bytes and
#: their MIME type. Absent keys mean the collection ships no such map, which is
#: the common case and is why the lit view's controls are conditional.
Maps = dict[str, tuple[bytes, str]]


def _data_url(blob: bytes, content_type: str) -> dict[str, str]:
    """Inline a blob so the page needs nothing else.

    Args:
        blob: The bytes.
        content_type: Its MIME type.

    Returns:
        A payload the page can use as an image source.
    """
    encoded = base64.b64encode(blob).decode("ascii")
    return {"url": f"data:{content_type or 'application/octet-stream'};base64,{encoded}"}


_PAGE = """<!DOCTYPE html>
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
 .panel button{background:#2c313a;color:#d7dae0;border:1px solid var(--line);
   border-radius:4px;padding:4px 9px;cursor:pointer;font:inherit}
 .panel button:hover{background:#3d4450}
 .panel button.on{background:#3f4a5c;border-color:#5a6880;color:#fff}
 .panel input[type=range]{width:90px;accent-color:#6f8fb8;cursor:pointer}
 .panel .spacer{flex:1}
 #verdict{padding:7px 10px;border-bottom:1px solid var(--line);background:#181b21;
   font-size:12.5px}
 #verdict b{color:#9ecbff;font-weight:600}
 #verdict .num{font-family:Consolas,"Cascadia Mono",monospace;color:var(--ink)}
 #stage{flex:1;position:relative;min-height:0;overflow:auto;padding:12px;
   display:flex;gap:12px;align-items:flex-start;justify-content:center}
 .pane{display:flex;flex-direction:column;gap:5px;min-width:0}
 .pane h4{margin:0;font:600 11px/1.4 "Segoe UI",system-ui,sans-serif;
   color:var(--dim);text-transform:uppercase;letter-spacing:.05em}
 .frame{position:relative;background:#0d0f13;border:1px solid var(--line);
   border-radius:3px;overflow:hidden;line-height:0}
 /* A texture comparison is about the pixels that are there. Smoothing
    invents new ones that match neither original. */
 img{image-rendering:pixelated;display:block;max-width:100%}
 #wipe .frame img.b{position:absolute;inset:0;clip-path:inset(0 0 0 var(--split))}
 #wipe .handle{position:absolute;top:0;bottom:0;width:2px;background:#9ecbff;
   left:var(--split);cursor:ew-resize;box-shadow:0 0 6px #000}
 .miss{padding:2rem;text-align:center;opacity:.75}
 .lit canvas{display:block;background:#0d0f13}
 .panel .sep{width:1px;align-self:stretch;background:var(--line);margin:0 2px}
</style></head><body>
<div class="panel" id="controls"></div>
<div id="verdict">__VERDICT__</div>
<div id="stage"></div>
__LIBRARY__
<script id="payload" type="application/json">__PAYLOAD__</script>
<script>
(function () {
  var data = JSON.parse(document.getElementById("payload").textContent);
  var stage = document.getElementById("stage");
  var controls = document.getElementById("controls");
  // Side by side is the honest default: it shows both files as they are,
  // without a display choice standing between the user and the pixels.
  var mode = "side";
  var amplify = 4;
  var light = {key: 1.4, ambient: 0.25, angle: 0.6, elevation: 0.5};
  // All on to begin with, because the lit view exists precisely to show the
  // maps. Turning them off is the comparison aid, not the default.
  var show = {diffuse: true, normal: true, specular: true};

  function hasMap(suffix) {
    return !!((data.leftMaps && data.leftMaps[suffix]) ||
              (data.rightMaps && data.rightMaps[suffix]));
  }
  var anyNormal = hasMap("_n") || hasMap("_nh");
  var anySpecular = hasMap("_spec");

  function frame(src, label, extra) {
    var pane = document.createElement("div");
    pane.className = "pane";
    var head = document.createElement("h4");
    head.textContent = label + (extra ? " -- " + extra : "");
    var box = document.createElement("div");
    box.className = "frame";
    var img = document.createElement("img");
    img.src = src;
    box.appendChild(img);
    pane.appendChild(head); pane.appendChild(box);
    return pane;
  }

  function renderSide() {
    stage.appendChild(frame(data.left.url, data.leftName, data.leftSize));
    stage.appendChild(frame(data.right.url, data.rightName, data.rightSize));
  }

  function renderWipe() {
    var pane = document.createElement("div");
    pane.className = "pane"; pane.id = "wipe";
    var head = document.createElement("h4");
    head.textContent = "drag the handle -- left is " + data.leftName;
    var box = document.createElement("div");
    box.className = "frame";
    box.style.setProperty("--split", "50%");
    var a = document.createElement("img"); a.src = data.left.url;
    var b = document.createElement("img"); b.src = data.right.url; b.className = "b";
    var handle = document.createElement("div");
    handle.className = "handle";
    box.appendChild(a); box.appendChild(b); box.appendChild(handle);
    // The eye detects motion far better than it compares two things it must
    // look back and forth between, which is what makes a wipe find changes a
    // side-by-side view hides.
    function track(event) {
      var rect = box.getBoundingClientRect();
      var x = Math.min(Math.max(event.clientX - rect.left, 0), rect.width);
      var pct = (x / Math.max(rect.width, 1)) * 100;
      box.style.setProperty("--split", pct + "%");
    }
    var dragging = false;
    handle.addEventListener("mousedown", function (e) { dragging = true; e.preventDefault(); });
    window.addEventListener("mouseup", function () { dragging = false; });
    window.addEventListener("mousemove", function (e) { if (dragging) track(e); });
    box.addEventListener("click", track);
    pane.appendChild(head); pane.appendChild(box);
    stage.appendChild(pane);
  }

  function renderDifference() {
    if (!data.difference) {
      var none = document.createElement("div");
      none.className = "miss";
      none.textContent = data.differenceWhy || "no difference image for this pair";
      stage.appendChild(none);
      return;
    }
    var pane = frame(data.difference.url, "difference",
      "amplified " + amplify + "x; black is unchanged");
    pane.querySelector("img").style.filter = "brightness(" + (amplify / 4) + ")";
    stage.appendChild(pane);
  }

  // -- the lit material view --------------------------------------------
  //
  // A flat side-by-side view cannot compare two normal maps at all: they
  // encode how a surface catches light, and a picture of one is a field of
  // pale blue. Putting each texture on a lit quad with its own normal and
  // specular maps applied is the only view in which "which of these two is
  // better" is a question the eye can answer.
  var lit = null;

  function texture(url, srgb) {
    var image = new Image();
    var tex = new THREE.Texture(image);
    // Color is sRGB; a normal map is a field of vectors and must be read
    // linearly, or every one of them is bent before it is used.
    if (srgb) tex.colorSpace = THREE.SRGBColorSpace;
    image.onload = function () { tex.needsUpdate = true; if (lit) lit.draw(); };
    image.src = url;
    return tex;
  }

  function buildLit() {
    var wrap = document.createElement("div");
    wrap.className = "pane lit";
    var head = document.createElement("h4");
    head.textContent = "lit material -- drag to move the light";
    var renderer = new THREE.WebGLRenderer({antialias: true, alpha: false});
    renderer.setPixelRatio(window.devicePixelRatio || 1);
    var scene = new THREE.Scene();
    // Orthographic: a perspective camera would foreshorten one side of each
    // quad and make the two panes disagree for a reason that is not in the
    // files.
    var camera = new THREE.OrthographicCamera(-2.1, 2.1, 1.05, -1.05, 0.1, 100);
    camera.position.set(0, 0, 10);

    var ambient = new THREE.AmbientLight(0xffffff, 0.25);
    scene.add(ambient);
    var key = new THREE.DirectionalLight(0xffffff, 1.4);
    scene.add(key);

    function makeQuad(x, maps, base) {
      var material = new THREE.MeshPhongMaterial({color: 0xffffff, shininess: 30});
      material.userData.maps = {
        map: texture(base, true),
        normalMap: maps["_nh"] ? texture(maps["_nh"].url, false)
                 : maps["_n"] ? texture(maps["_n"].url, false) : null,
        specularMap: maps["_spec"] ? texture(maps["_spec"].url, true) : null
      };
      var mesh = new THREE.Mesh(new THREE.PlaneGeometry(2, 2), material);
      mesh.position.x = x;
      scene.add(mesh);
      return mesh;
    }

    var quads = [
      makeQuad(-1.05, data.leftMaps || {}, data.left.url),
      makeQuad(1.05, data.rightMaps || {}, data.right.url)
    ];

    function apply() {
      quads.forEach(function (q) {
        var m = q.material, held = m.userData.maps;
        m.map = show.diffuse ? held.map : null;
        m.normalMap = show.normal ? held.normalMap : null;
        m.specularMap = show.specular ? held.specularMap : null;
        // With the diffuse map off the quad must still be a surface rather
        // than a black hole, or turning it off reads as a broken toggle.
        m.color = new THREE.Color(show.diffuse ? 0xffffff : 0x9aa0aa);
        m.specular = new THREE.Color(show.specular ? 0x666666 : 0x000000);
        m.needsUpdate = true;
      });
    }

    function place() {
      var a = light.angle, e = light.elevation;
      key.position.set(Math.cos(e) * Math.sin(a), Math.sin(e), Math.cos(e) * Math.cos(a));
      key.intensity = light.key;
      ambient.intensity = light.ambient;
    }

    function resize() {
      var w = Math.max(stage.clientWidth - 40, 320);
      var h = Math.round(w / 4.0);
      renderer.setSize(w, h, false);
    }

    function draw() { renderer.render(scene, camera); }

    var box = document.createElement("div");
    box.className = "frame";
    box.appendChild(renderer.domElement);
    // Dragging moves the light rather than the camera. On a flat quad there
    // is nothing to orbit, and the light is the only thing worth moving.
    var dragging = false;
    function track(e) {
      var rect = renderer.domElement.getBoundingClientRect();
      light.angle = ((e.clientX - rect.left) / Math.max(rect.width, 1) - 0.5) * Math.PI * 2;
      light.elevation = (0.5 - (e.clientY - rect.top) / Math.max(rect.height, 1)) * Math.PI;
      place(); draw();
    }
    renderer.domElement.addEventListener("mousedown", function (e) {
      dragging = true; track(e); e.preventDefault();
    });
    window.addEventListener("mouseup", function () { dragging = false; });
    window.addEventListener("mousemove", function (e) { if (dragging) track(e); });

    wrap.appendChild(head); wrap.appendChild(box);
    return {node: wrap, apply: apply, place: place, resize: resize, draw: draw};
  }

  function renderLit() {
    if (!data.canLight) {
      var none = document.createElement("div");
      none.className = "miss";
      none.textContent = "the lit view needs three.js, which this page was built without";
      stage.appendChild(none);
      return;
    }
    lit = buildLit();
    stage.appendChild(lit.node);
    lit.apply(); lit.place(); lit.resize(); lit.draw();
  }

  function draw() {
    lit = null;
    stage.textContent = "";
    if (mode === "side") renderSide();
    else if (mode === "wipe") renderWipe();
    else if (mode === "material") renderLit();
    else renderDifference();
    Array.prototype.forEach.call(controls.querySelectorAll("button[data-mode]"),
      function (b) { b.className = b.dataset.mode === mode ? "on" : ""; });
    ampCtl.className = "ctl" + (mode === "difference" ? "" : " off");
    // The light and map controls do nothing outside the lit view, so they are
    // dimmed rather than left looking operative.
    Array.prototype.forEach.call(controls.querySelectorAll("[data-lit]"),
      function (el) {
        var off = mode !== "material";
        el.className = el.className.replace(/ off\b/, "") + (off ? " off" : "");
        Array.prototype.forEach.call(el.querySelectorAll("input"),
          function (i) { i.disabled = off; });
      });
  }

  var modes = [["side", "Side by side"], ["wipe", "Overlay"],
               ["difference", "Difference"]];
  if (data.canLight) modes.push(["material", "Lit material"]);
  modes.forEach(function (pair) {
    var button = document.createElement("button");
    button.type = "button";
    button.dataset.mode = pair[0];
    button.textContent = pair[1];
    button.addEventListener("click", function () { mode = pair[0]; draw(); });
    controls.appendChild(button);
  });

  var ampCtl = document.createElement("span");
  ampCtl.className = "ctl off";
  var ampLabel = document.createElement("label");
  ampLabel.htmlFor = "amp"; ampLabel.textContent = "Amplify";
  var amp = document.createElement("input");
  amp.type = "range"; amp.id = "amp"; amp.min = 1; amp.max = 16; amp.step = 1; amp.value = 4;
  amp.addEventListener("input", function () {
    amplify = parseInt(amp.value, 10);
    if (mode === "difference") draw();
  });
  ampCtl.appendChild(ampLabel); ampCtl.appendChild(amp);
  controls.appendChild(ampCtl);

  if (data.canLight) {
    var sep = document.createElement("span");
    sep.className = "sep";
    controls.appendChild(sep);

    function slider(id, label, max, value, step, onInput) {
      var ctl = document.createElement("span");
      ctl.className = "ctl off";
      ctl.dataset.lit = "1";
      var text = document.createElement("label");
      text.htmlFor = id; text.textContent = label;
      var range = document.createElement("input");
      range.type = "range"; range.id = id;
      range.min = 0; range.max = max; range.step = step; range.value = value;
      range.addEventListener("input", function () {
        onInput(parseFloat(range.value));
        if (lit) { lit.place(); lit.draw(); }
      });
      ctl.appendChild(text); ctl.appendChild(range);
      controls.appendChild(ctl);
    }
    slider("tkey", "Light", 4, light.key, 0.05, function (v) { light.key = v; });
    slider("tamb", "Ambient", 2, light.ambient, 0.05, function (v) { light.ambient = v; });

    // A toggle per map, offered only where a map exists. Turning them off is
    // what makes the "simple comparison" the flat views give, but inside the
    // lit view -- so the lighting stays fixed while the maps come and go, and
    // the change you see is the map rather than the whole scene.
    function toggle(id, label, key, enabled) {
      var ctl = document.createElement("span");
      ctl.className = "ctl off";
      ctl.dataset.lit = "1";
      var box = document.createElement("input");
      box.type = "checkbox"; box.id = id; box.checked = enabled;
      show[key] = enabled;
      var text = document.createElement("label");
      text.htmlFor = id; text.textContent = label;
      box.addEventListener("change", function () {
        show[key] = box.checked;
        if (lit) { lit.apply(); lit.draw(); }
      });
      ctl.appendChild(box); ctl.appendChild(text);
      controls.appendChild(ctl);
    }
    toggle("tdiff", "Diffuse", "diffuse", true);
    if (anyNormal) toggle("tnorm", "Normal", "normal", true);
    if (anySpecular) toggle("tspec", "Specular", "specular", true);
  }

  window.addEventListener("resize", function () {
    if (lit) { lit.resize(); lit.draw(); }
  });

  draw();
})();
</script>
</body></html>
"""


def build_compare_page(
    left: tuple[str, bytes, str],
    right: tuple[str, bytes, str],
    outcome: Comparison,
    *,
    difference: tuple[bytes, str] | None = None,
    title: str = "Texture comparison",
    sink: BlobSink | None = None,
    left_maps: Maps | None = None,
    right_maps: Maps | None = None,
    library_url: str = "",
    library_source: str = "",
) -> str:
    """Build a page comparing two textures.

    Args:
        left: Provider name, displayable bytes, and their MIME type.
        right: The same for the other side.
        outcome: What :func:`~wraithguard.images.compare.compare_images` found.
        difference: A rendered difference image and its MIME type, when the two
            were the same size and one could be made.
        title: The page title, usually the texture's path.
        sink: How to publish a blob. Omitted, blobs are inlined as data URLs so
            the page is a single file that works from ``file://``.
        left_maps: The normal and specular maps that sit beside the first
            texture, when the collection ships any. Their presence is what
            enables the lit material view.
        right_maps: The same for the other side.
        library_url: Where to fetch three.js from, for a served page.
        library_source: The library itself, to inline for a standalone file.
            One or the other; inlining is what makes the export work offline.

    Returns:
        The whole page.
    """
    publish = sink or _data_url
    left_name, left_bytes, left_type = left
    right_name, right_bytes, right_type = right
    payload: dict[str, object] = {
        "leftName": left_name,
        "rightName": right_name,
        "left": publish(left_bytes, left_type),
        "right": publish(right_bytes, right_type),
        "leftSize": _size(outcome.left_size),
        "rightSize": _size(outcome.right_size),
        "difference": publish(*difference) if difference else None,
        "differenceWhy": _why_no_difference(outcome),
        "leftMaps": _publish_maps(left_maps, publish),
        "rightMaps": _publish_maps(right_maps, publish),
        # Whether a lit view is possible at all. Without three.js there is no
        # renderer, and offering the control anyway would be offering a button
        # that cannot work.
        "canLight": bool(library_url or library_source),
    }
    LOG.debug("comparison page: %s, %s", title, outcome.verdict.value)
    return (
        _PAGE.replace("__TITLE__", html.escape(title))
        .replace("__VERDICT__", _verdict_line(outcome))
        .replace("__LIBRARY__", _library_block(library_url, library_source))
        .replace("__PAYLOAD__", json.dumps(payload))
    )


def _publish_maps(maps: Maps | None, publish: BlobSink) -> dict[str, object]:
    """Publish a side's auxiliary maps.

    Args:
        maps: The normal and specular maps, if any.
        publish: How to publish a blob.

    Returns:
        Suffix to payload, for the maps that exist.
    """
    if not maps:
        return {}
    return {name: publish(blob, kind) for name, (blob, kind) in maps.items()}


def _library_block(url: str, source: str) -> str:
    """Emit the script tag or inline block for three.js.

    The library is a CommonJS build, so it needs the same ``exports`` shim the
    mesh viewer uses. That is not a detail worth rediscovering: a served page
    without it fails with "THREE is not defined", and the failure looks like a
    broken feature rather than a missing global.

    Args:
        url: Where to fetch it from, for a served page.
        source: The library itself, to inline.

    Returns:
        HTML, or ``""`` when there is no library and the lit view is off.
    """
    if not url and not source:
        return ""
    shim_open = "<script>var exports = {}, module = {exports: exports};</script>"
    shim_close = "<script>var THREE = module.exports || exports;</script>"
    body = f'<script src="{html.escape(url, quote=True)}"></script>' if url else (
        f"<script>{source}</script>"
    )
    return f"{shim_open}\n{body}\n{shim_close}"


def _size(size: tuple[int, int] | None) -> str:
    """Format a size for a heading.

    Args:
        size: Width and height, or ``None``.

    Returns:
        Something like ``"512x512"``, or ``""``.
    """
    return f"{size[0]}x{size[1]}" if size else ""


def _why_no_difference(outcome: Comparison) -> str:
    """Explain why there is no difference image, in the page's own words.

    An empty pane invites the reader to assume the feature is broken. Saying
    which of several ordinary reasons applies is the whole difference between
    a missing image and an answer.

    Args:
        outcome: The comparison.

    Returns:
        A sentence.
    """
    if outcome.verdict is Verdict.DIFFERENT_SIZE:
        return (
            "These are different sizes, so there is nothing to subtract. "
            "Rescaling one would invent pixels and then show you differences "
            "in the pixels it invented."
        )
    if outcome.verdict is Verdict.NOT_COMPARABLE:
        return outcome.detail
    if outcome.verdict in (Verdict.IDENTICAL, Verdict.SAME_PIXELS):
        return "The images are identical, so the difference is blank everywhere."
    if outcome.verdict is Verdict.UNDECODABLE:
        return outcome.detail
    return "No difference image was built for this pair."


def _verdict_line(outcome: Comparison) -> str:
    """Write the one-line summary above the panes.

    Args:
        outcome: The comparison.

    Returns:
        Escaped HTML.
    """
    verdict = html.escape(outcome.verdict.value)
    detail = html.escape(outcome.detail)
    parts = [f"<b>{verdict}</b> &mdash; {detail}"]
    if outcome.verdict is Verdict.DIFFERENT:
        parts.append(
            f'<span class="num">{outcome.changed_share:.2%}</span> of pixels changed, '
            f'worst channel <span class="num">{outcome.worst_channel}</span>, '
            f'mean <span class="num">{outcome.mean_channel:.2f}</span>'
        )
    if outcome.left_role is not outcome.right_role:
        parts.append(
            f"roles: {html.escape(outcome.left_role.value)} against "
            f"{html.escape(outcome.right_role.value)}"
        )
    return " &nbsp;·&nbsp; ".join(parts)
