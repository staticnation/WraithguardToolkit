//! Gardenfell's `weather`: Robin Hjelte's, from `gardenfell-install` (viewer-shell/gardenfell,
//! vendored; carried patches in its `patches/`), re-exported so viewcore's callers keep
//! their paths.

pub use gardenfell_install::weather::*;

// The ten weathers and their `glareView` are carried patch 0004 in his crate.

/// The upstream atmosphere fields with Wraithguard's renderer additions (texture maps).
pub fn atmosphere_json(order_file: &std::path::Path, vfs: &crate::vfs::Vfs, layout: &crate::layout::Layout) -> String {
    atmosphere_json_with(order_file, vfs, layout, &WEATHERS)
}

/// The atmosphere for the requested weathers, using our renderer adapter.
pub fn atmosphere_json_with(order_file: &std::path::Path, vfs: &crate::vfs::Vfs, layout: &crate::layout::Layout, weathers: &[&str]) -> String {
    let ini = gardenfell_bytes::openmwcfg::order_text(order_file);
    format!(
        "{{\"ini\":{},\"adjuster\":{},\"mge\":{},\"renderer\":{},\"wondersOfWater\":{}}}",
        ini_json_with(&ini, weathers),
        adjuster_json(vfs),
        mge_json_with(layout, weathers),
        crate::renderer::renderer_json(layout, &ini),
        wonders_json(vfs, layout)
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn atmosphere_keeps_openmw_texture_map_settings() {
        let dir = std::env::temp_dir().join(format!("wg_weather_maps_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let order = dir.join("openmw.cfg");
        std::fs::write(&order, "fallback=Weather_Blizzard_Cloud_Speed,7.5\n").unwrap();
        std::fs::write(dir.join("settings.cfg"), "[Shaders]\nauto use object normal maps = true\nauto use terrain normal maps = false\nauto use terrain specular maps = true\nnormal map pattern = _custom_n\nterrain specular map pattern = _custom_spec\n").unwrap();
        let layout = crate::layout::Layout { order_file: order.clone(), ..Default::default() };
        let json = super::atmosphere_json_with(&order, &crate::vfs::Vfs::default(), &layout, &super::ALL_WEATHERS);
        std::fs::remove_dir_all(&dir).unwrap();
        let parsed = crate::json::parse(&json).unwrap();
        let maps = parsed.get("renderer").unwrap().get("maps").expect("map settings reach the page");
        assert_eq!(maps.get("objectNormal").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(maps.get("terrainNormal").and_then(|v| v.as_bool()), Some(false));
        assert_eq!(maps.get("terrainSpecular").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(maps.get("normalPattern").and_then(|v| v.as_str()), Some("_custom_n"));
        assert_eq!(maps.get("terrainSpecularPattern").and_then(|v| v.as_str()), Some("_custom_spec"));
        assert!(parsed.get("wondersOfWater").is_some());
        assert!(parsed.get("ini").unwrap().get("weathers").unwrap().get("Blizzard").is_some());
    }
}
