//! Facts about this build, fixed at compile time.

use serde::Serialize;

/// What `exodus info` and bug reports show about the running build.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildInfo {
    pub version: &'static str,
    pub target: &'static str,
    /// Whether this build carries a Microsoft client ID. Never the ID itself.
    pub msa_client_id_configured: bool,
}

/// Returns the facts about this build.
pub fn build_info() -> BuildInfo {
    BuildInfo {
        version: env!("CARGO_PKG_VERSION"),
        target: env!("EXODUS_BUILD_TARGET"),
        msa_client_id_configured: msa_client_id().is_some(),
    }
}

/// The Microsoft client ID baked in by `build.rs`, if any. Never log it.
pub(crate) fn msa_client_id() -> Option<&'static str> {
    option_env!("EXODUS_MSA_CLIENT_ID").filter(|id| !id.trim().is_empty())
}
