//! Passes build time values to the crate: the Microsoft client ID (optional)
//! and the target triple shown by `exodus info`.

use std::{env, fs, path::Path};

const CLIENT_ID_VAR: &str = "EXODUS_MSA_CLIENT_ID";

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let dotenv = Path::new(&manifest_dir).join("../../.env");

    println!("cargo:rerun-if-env-changed={CLIENT_ID_VAR}");
    // Cargo treats a watched file that does not exist as always changed,
    // which would rebuild this crate on every build. So `.env` is watched
    // only once it exists; after creating it, rebuild once (see `.env.example`).
    if dotenv.is_file() {
        println!("cargo:rerun-if-changed={}", dotenv.display());
    } else {
        println!("cargo:rerun-if-changed=build.rs");
    }

    // The environment wins over `.env`; an empty value counts as not set,
    // which is what fork pull requests see when the CI secret is missing.
    let client_id = env::var(CLIENT_ID_VAR)
        .ok()
        .map(|value| clean(&value))
        .filter(|value| !value.is_empty())
        .or_else(|| read_dotenv(&dotenv, CLIENT_ID_VAR))
        .unwrap_or_default();
    println!("cargo:rustc-env={CLIENT_ID_VAR}={client_id}");

    let target = env::var("TARGET").expect("cargo sets TARGET");
    println!("cargo:rustc-env=EXODUS_BUILD_TARGET={target}");
}

/// Reads `key` from a `KEY=VALUE` file. Comments start with `#`.
fn read_dotenv(path: &Path, key: &str) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .find(|(name, _)| name.trim() == key)
        .map(|(_, value)| clean(value))
        .filter(|value| !value.is_empty())
}

fn clean(value: &str) -> String {
    value
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .trim()
        .to_owned()
}
