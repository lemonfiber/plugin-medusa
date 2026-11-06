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
