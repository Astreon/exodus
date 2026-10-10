//! Launcher configuration and where Exodus keeps its files.

use std::path::{Path, PathBuf};

use directories::ProjectDirs;

use crate::{Localize, MessageArgs};

/// Public home of the project, sent in the user agent.
pub const REPO_URL: &str = env!("CARGO_PKG_REPOSITORY");

/// Everything a [`Launcher`](crate::Launcher) needs to start.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct LauncherConfig {
    pub data_dir: PathBuf,
    pub endpoints: Endpoints,
    pub user_agent: String,
}

impl LauncherConfig {
    /// A config for `data_dir` with the real endpoints and the default user agent.
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            endpoints: Endpoints::default(),
            user_agent: default_user_agent(),
        }
    }

    /// Where the diagnostic logs of this data folder live.
    pub fn log_dir(&self) -> PathBuf {
        log_dir(&self.data_dir)
    }
}

/// Base URLs of every external service. Defaults are the real endpoints.
///
/// Only changeable through [`LauncherConfig`] (tests point them at a mock
/// server), never through environment variables, so nobody can redirect
/// token endpoints from outside. Each feature adds its own fields.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct Endpoints {}

/// `Exodus/<version> (+<repo url>)`.
pub fn default_user_agent() -> String {
    format!("Exodus/{} (+{REPO_URL})", env!("CARGO_PKG_VERSION"))
}

/// The standard data folder of this system.
pub fn default_data_dir() -> Result<PathBuf, ConfigError> {
    ProjectDirs::from("", "", "Exodus")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .ok_or(ConfigError::NoHomeDir)
}

/// Where the diagnostic logs of `data_dir` live.
pub fn log_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("logs")
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    #[error("no home directory found for the current user")]
    NoHomeDir,
}

impl Localize for ConfigError {
    fn key(&self) -> &'static str {
        match self {
            Self::NoHomeDir => "config-no-home-dir",
        }
    }

    fn args(&self) -> MessageArgs {
        match self {
            Self::NoHomeDir => MessageArgs::new(),
        }
    }
}

#[cfg(test)]
pub(crate) fn error_samples() -> Vec<Box<dyn Localize>> {
    vec![Box::new(ConfigError::NoHomeDir)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agent_names_version_and_repo() {
        let agent = default_user_agent();
        assert!(agent.starts_with(&format!("Exodus/{} (+", env!("CARGO_PKG_VERSION"))));
        assert!(agent.ends_with(&format!("{REPO_URL})")));
    }

    #[test]
    fn log_dir_is_inside_data_dir() {
        let config = LauncherConfig::new("/data");
        assert_eq!(config.log_dir(), Path::new("/data").join("logs"));
    }
}
