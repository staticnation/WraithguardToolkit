//! Gardenfell's `renderer`: Robin Hjelte's, from `gardenfell-install` (viewer-shell/gardenfell,
//! vendored; carried patches in its `patches/`), re-exported so viewcore's callers keep
//! their paths. Below it, what Wraithguard adds.

pub use gardenfell_install::renderer::*;

use gardenfell_bytes::ini::{lookup, section};
use gardenfell_install::layout::Layout;

/// Robin's `renderer_json`, and for an OpenMW install the `maps` object: whether OpenMW
/// puts the `_n`/`_nh` and `_spec` maps beside a texture on objects (and on the land), and
/// the suffixes it looks for, from the `[Shaders]` block of `settings.cfg`. OpenMW's
/// defaults are off; `found` says whether the file said so or nobody did.
pub fn renderer_json(layout: &Layout, order_text: &str) -> String {
    let base = gardenfell_install::renderer::renderer_json(layout, order_text);
    if !crate::luascan::is_openmw_cfg(&layout.order_file) || !base.ends_with('}') {
        return base;
    }
    // Where Robin's reads settings.cfg from: the user config folder of the cfg chain, else
    // beside the cfg.
    let user_dir = gardenfell_bytes::openmwcfg::read(&layout.order_file).user_config_dir();
    let cfg = user_dir
        .map(|d| d.join("settings.cfg"))
        .into_iter()
        .chain(layout.order_file.parent().map(|p| p.join("settings.cfg")))
        .find(|p| p.is_file());
    let found = cfg.is_some();
    let text = cfg.as_ref().and_then(|p| gardenfell_bytes::codepage::read_text(p).ok()).unwrap_or_default();
    let shaders = section(&text, "Shaders");
    let get = |k: &str| lookup(&shaders, k).map(str::trim);
    let on = |k: &str| get(k).map(|v| v.eq_ignore_ascii_case("true") || v == "1").unwrap_or(false);
    let mut m = crate::json::J::obj();
    m.bool("found", found)
        .bool("setObjNormal", get("auto use object normal maps").is_some())
        .bool("objectNormal", on("auto use object normal maps"))
        .bool("objectSpecular", on("auto use object specular maps"))
        .bool("terrainNormal", on("auto use terrain normal maps"))
        .bool("terrainSpecular", on("auto use terrain specular maps"))
        .str("normalPattern", get("normal map pattern").unwrap_or("_n"))
        .str("normalHeightPattern", get("normal height map pattern").unwrap_or("_nh"))
        .str("specularPattern", get("specular map pattern").unwrap_or("_spec"))
        .str("terrainSpecularPattern", get("terrain specular map pattern").unwrap_or("_diffusespec"));
    format!("{},\"maps\":{}}}", &base[..base.len() - 1], m.done())
}
