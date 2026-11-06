//! The core the crates are built against is the core `targets.toml` proves the plugin against.

use toml::{Table, Value};

/// Where every core crate comes from.
const CORE: &str = "https://github.com/lemonfiber/lemonfiber";

/// The dependency tables a core crate may be pinned in.
const SECTIONS: [&str; 2] = ["dependencies", "dev-dependencies"];

fn read(name: &str) -> Table {
    let path = format!("{}/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .parse()
        .unwrap_or_default()
}

/// Each core crate and what it is pinned at: its `rev`, else its `tag`.
fn pins(manifest: &Table) -> Vec<(String, String)> {
    SECTIONS
        .iter()
        .filter_map(|section| manifest.get(*section).and_then(Value::as_table))
        .flat_map(Table::iter)
        .filter_map(|(name, spec)| {
            let spec = spec.as_table()?;
            if spec.get("git").and_then(Value::as_str) != Some(CORE) {
                return None;
            }
            let pin = spec
                .get("rev")
                .or_else(|| spec.get("tag"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            Some((name.clone(), pin.to_owned()))
        })
        .collect()
}

/// What `targets.toml` names, as a crate would be pinned at it: the commit itself, or
/// the release's tag.
fn named(targets: &Table) -> Option<String> {
    let text = |key: &str| targets.get(key).and_then(Value::as_str);
    text("commit")
        .map(str::to_owned)
        .or_else(|| text("lemonfiber").map(|release| format!("v{release}")))
}

#[test]
fn every_core_crate_is_pinned_at_the_core_targets_toml_names() {
    let named = named(&read("targets.toml"));
    let pins = pins(&read("Cargo.toml"));
    assert!(!pins.is_empty(), "Cargo.toml pins no crate of {CORE}");
    let astray: Vec<&(String, String)> = pins
        .iter()
        .filter(|(_, pin)| named.as_deref() != Some(pin.as_str()))
        .collect();
    assert!(
        astray.is_empty(),
        "targets.toml names {named:?}, and these core crates are pinned elsewhere: {astray:#?}"
    );
}
