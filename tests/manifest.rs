//! The manifest, as the core at the pinned commit reads it.

#[test]
fn the_pinned_core_refuses_nothing_in_the_manifest() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/plugin.toml"))
        .unwrap_or_default();
    let read: Result<lemonfiber_plugin::Manifest, _> = toml::from_str(&text);
    let refused: Vec<String> = match read {
        Ok(manifest) => lemonfiber_plugin::refusals(&manifest, &[])
            .into_iter()
            .map(|violation| format!("{}: {}", violation.location, violation.message))
            .collect(),
        Err(why) => vec![why.to_string()],
    };
    assert!(refused.is_empty(), "{refused:#?}");
}

#[test]
fn the_import_patience_the_manifest_writes_is_the_one_the_adapter_assumes() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/plugin.toml"))
        .unwrap_or_default();
    let manifest: toml::Table = toml::from_str(&text).unwrap_or_default();
    let written = manifest
        .get("service")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|service| service.get("env"))
        .find_map(|env| env.get(medusa_adapter::adapter::IMPORT_STUCK_AFTER))
        .and_then(|patience| patience.get("default"))
        .and_then(toml::Value::as_str)
        .map(|minutes| medusa_adapter::adapter::patience(Some(minutes)));
    assert_eq!(
        written,
        Some(Ok(Some(medusa_adapter::medusa::IMPORT_PATIENCE)))
    );
}
