//! The release and digest of the image the adapter fronts, read from `plugin.toml`, the
//! one place they are pinned.

use std::error::Error;
use std::path::Path;

/// The manifest the digest is read from.
const MANIFEST: &str = "plugin.toml";

/// The file the digest is written into, in Cargo's output directory.
const WRITTEN: &str = "fronted.rs";

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed={MANIFEST}");
    let manifest: toml::Table = std::fs::read_to_string(MANIFEST)?.parse()?;
    let services: Vec<&toml::Table> = manifest
        .get("service")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_table)
        .collect();
    let fronted = services
        .iter()
        .find_map(|service| service.get("fronts").and_then(toml::Value::as_str))
        .ok_or("no service of plugin.toml fronts another")?;
    let pinned = services
        .iter()
        .find(|service| service.get("id").and_then(toml::Value::as_str) == Some(fronted))
        .ok_or_else(|| format!("plugin.toml has no service {fronted}"))?;
    let digest = pinned
        .get("digest")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| format!("plugin.toml pins no digest for {fronted}"))?;
    let release = pinned
        .get("tag")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| format!("plugin.toml tags no release for {fronted}"))?;
    let out = std::env::var("OUT_DIR")?;
    std::fs::write(
        Path::new(&out).join(WRITTEN),
        format!(
            "/// The release of the image the adapter fronts, as `plugin.toml` tags it.\npub const RELEASE: &str = {release:?};\n\n/// The digest of the image that release's recordings were taken from, as `plugin.toml` pins it.\npub const DIGEST: &str = {digest:?};\n"
        ),
    )?;
    Ok(())
}
