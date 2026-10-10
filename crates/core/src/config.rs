//! Launcher configuration and where Exodus keeps its files.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use serde::Serialize;

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

/// Environment variable that overrides the data folder (empty means unset).
pub const DATA_DIR_ENV: &str = "EXODUS_DATA_DIR";

/// Marker file next to the binary that turns on portable mode.
pub const PORTABLE_MARKER: &str = "portable.txt";

/// The standard data folder of this system: `%LOCALAPPDATA%\Exodus\data` on
/// Windows, `~/Library/Application Support/Exodus` on macOS and
/// `$XDG_DATA_HOME/exodus` on Linux.
pub fn default_data_dir() -> Result<PathBuf, ConfigError> {
    ProjectDirs::from("", "", "Exodus")
        .map(|dirs| dirs.data_local_dir().to_path_buf())
        .ok_or(ConfigError::NoHomeDir)
}

/// Where the data folder setting came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DataDirSource {
    /// The `--data-dir` flag.
    Flag,
    /// The `EXODUS_DATA_DIR` environment variable.
    Env,
    /// `portable.txt` next to the binary: data lives in `<exe_dir>/data/`.
    Portable,
    /// The standard folder of this system.
    Default,
}

impl DataDirSource {
    /// Stable lowercase name: `flag`, `env`, `portable` or `default`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Flag => "flag",
            Self::Env => "env",
            Self::Portable => "portable",
            Self::Default => "default",
        }
    }
}

/// Picks the data folder: flag, then environment variable, then portable
/// mode, then `default`. Empty values count as unset. Relative paths are
/// joined to `cwd`, dropping `.` parts without resolving `..`. Only `default` can fail, and only
/// when it is the one picked.
pub fn resolve_data_dir(
    flag: Option<PathBuf>,
    env: Option<OsString>,
    portable_dir: Option<PathBuf>,
    cwd: &Path,
    default: Result<PathBuf, ConfigError>,
) -> Result<(PathBuf, DataDirSource), ConfigError> {
    // `components` drops `.` parts but keeps `..`.
    let absolute = |path: PathBuf| cwd.join(path).components().collect::<PathBuf>();
    let flag = flag.filter(|path| !path.as_os_str().is_empty());
    let env = env.filter(|value| !value.is_empty()).map(PathBuf::from);
    if let Some(path) = flag {
        Ok((absolute(path), DataDirSource::Flag))
    } else if let Some(path) = env {
        Ok((absolute(path), DataDirSource::Env))
    } else if let Some(exe_dir) = portable_dir {
        Ok((absolute(exe_dir.join("data")), DataDirSource::Portable))
    } else {
        default.map(|path| (path, DataDirSource::Default))
    }
}

/// Finds the data folder of this process from `flag`, `EXODUS_DATA_DIR`,
/// `portable.txt` next to the binary and the system default. Creates nothing.
pub fn locate_data_dir(flag: Option<PathBuf>) -> Result<(PathBuf, DataDirSource), ConfigError> {
    // `dunce` resolves links like `std::fs::canonicalize`, but keeps
    // `C:\…` instead of `\\?\C:\…`, which Java and other tools mishandle.
    let portable_dir = std::env::current_exe()
        .and_then(dunce::canonicalize)
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .filter(|dir| dir.join(PORTABLE_MARKER).is_file());
    // Without a working directory, relative paths stay as given.
    let cwd = std::env::current_dir().unwrap_or_default();
    resolve_data_dir(
        flag,
        std::env::var_os(DATA_DIR_ENV),
        portable_dir,
        &cwd,
        default_data_dir(),
    )
}

/// Where the diagnostic logs of `data_dir` live.
pub fn log_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("logs")
}

/// Errors while working out the configuration.
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

    fn resolve(
        flag: Option<&str>,
        env: Option<&str>,
        portable: Option<&str>,
    ) -> Result<(PathBuf, DataDirSource), ConfigError> {
        resolve_data_dir(
            flag.map(PathBuf::from),
            env.map(OsString::from),
            portable.map(PathBuf::from),
            &root().join("cwd"),
            Ok(root().join("default")),
        )
    }

    fn root() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\")
        } else {
            PathBuf::from("/")
        }
    }

    #[test]
    fn data_dir_comes_from_flag_env_portable_then_default() {
        let exe = root().join("exe");
        let exe = exe.to_str().unwrap();
        let flag = root().join("flag");
        let flag = flag.to_str().unwrap();
        let env = root().join("env");
        let env = env.to_str().unwrap();
        let cases = [
            (
                Some(flag),
                Some(env),
                Some(exe),
                root().join("flag"),
                DataDirSource::Flag,
            ),
            (
                Some(""),
                Some(env),
                Some(exe),
                root().join("env"),
                DataDirSource::Env,
            ),
            (
                None,
                Some(""),
                Some(exe),
                root().join("exe").join("data"),
                DataDirSource::Portable,
            ),
            (
                Some(""),
                Some(""),
                None,
                root().join("default"),
                DataDirSource::Default,
            ),
            (
                None,
                None,
                None,
                root().join("default"),
                DataDirSource::Default,
            ),
        ];
        for (flag, env, portable, path, source) in cases {
            let resolved = resolve(flag, env, portable).unwrap();
            assert_eq!(resolved, (path, source), "{flag:?} {env:?} {portable:?}");
        }
    }

    #[test]
    fn relative_paths_join_the_working_directory_and_keep_dot_dot() {
        let (path, _) = resolve(Some("../games"), None, None).unwrap();
        assert_eq!(path, root().join("cwd").join("..").join("games"));
        let (path, _) = resolve(Some("./games/."), None, None).unwrap();
        assert_eq!(path, root().join("cwd").join("games"));
        let (path, source) = resolve(None, Some("rel"), None).unwrap();
        assert_eq!(
            (path, source),
            (root().join("cwd").join("rel"), DataDirSource::Env)
        );
    }

    #[test]
    fn a_missing_home_only_matters_when_the_default_is_needed() {
        let flag = resolve_data_dir(
            Some(PathBuf::from("x")),
            None,
            None,
            &root(),
            Err(ConfigError::NoHomeDir),
        );
        assert_eq!(flag.unwrap().1, DataDirSource::Flag);
        let default = resolve_data_dir(None, None, None, &root(), Err(ConfigError::NoHomeDir));
        assert!(matches!(default, Err(ConfigError::NoHomeDir)));
    }

    #[test]
    fn default_data_dir_is_the_local_data_folder() {
        let Ok(dir) = default_data_dir() else {
            return;
        };
        let dirs = ProjectDirs::from("", "", "Exodus").unwrap();
        assert_eq!(dir, dirs.data_local_dir());
        if cfg!(windows) {
            assert!(dir.ends_with(r"Exodus\data"), "{}", dir.display());
        }
    }

    #[test]
    fn log_dir_is_inside_data_dir() {
        let config = LauncherConfig::new("/data");
        assert_eq!(config.log_dir(), Path::new("/data").join("logs"));
    }
}
