// ---------------------------------------------------------------------------
// Bundled config template — embedded at compile time via include_str!()
//
// Isolated in its own module so the 20 KiB string literal does not contribute
// to stack pressure in the monolithic lib.rs during release-mode compilation.
// ---------------------------------------------------------------------------

pub const BUNDLED_CONFIG_TEMPLATE: &str = include_str!("../resources/config.json");

#[test]
fn embedded_config_template_is_valid_json() {
    let value: serde_json::Value =
        serde_json::from_str(BUNDLED_CONFIG_TEMPLATE)
            .expect("embedded config template must be valid JSON");
    assert!(value.get("providers").is_some(), "missing providers");
    assert!(value.get("server").is_some(), "missing server");
}
