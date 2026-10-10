//! Runs the real `exodus` binary and checks what a person sees: output and exit codes.

use std::process::{Command, Output};

fn exodus(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_exodus"))
        .args(args)
        .env_remove("EXODUS_LOG")
        .env_remove("EXODUS_DATA_DIR")
        .output()
        .expect("the exodus binary starts")
}

fn exodus_with_env(args: &[&str], data_dir_env: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_exodus"))
        .args(args)
        .env_remove("EXODUS_LOG")
        .env("EXODUS_DATA_DIR", data_dir_env)
        .output()
        .expect("the exodus binary starts")
}

/// The `Data folder:` line of `exodus info`.
fn data_folder_line(output: &Output) -> String {
    let text = stdout(output);
    text.lines()
        .find_map(|line| line.strip_prefix("Data folder: "))
        .unwrap_or_else(|| panic!("no data folder in {text}"))
        .to_owned()
}

/// A path in the temp folder that does not exist.
fn missing_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("exodus-cli-test-{name}-{}", std::process::id()))
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

// covers: AC-2
#[test]
fn help_lists_the_commands_and_exits_with_0() {
    let output = exodus(&["--help"]);
    assert_eq!(output.status.code(), Some(0));
    let text = stdout(&output);
    assert!(text.contains("Usage:"), "{text}");
    assert!(text.contains("info"), "{text}");
}

#[test]
fn help_hides_the_debug_only_dev_wait_command() {
    let text = stdout(&exodus(&["--help"]));
    assert!(!text.contains("dev-wait"), "{text}");
}

// covers: AC-2
#[test]
fn version_prints_the_package_version_and_exits_with_0() {
    let output = exodus(&["--version"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        stdout(&output).trim(),
        format!("exodus {}", env!("CARGO_PKG_VERSION"))
    );
}

// covers: AC-2, AC-4 (texts from cli.ftl; client ID status shown, never the ID)
#[test]
fn info_prints_every_line_from_the_fluent_texts_and_exits_with_0() {
    let output = exodus(&["info"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let text = stdout(&output);
    let lines: Vec<&str> = text.lines().collect();

    assert_eq!(lines.len(), 5, "{text}");
    assert_eq!(lines[0], format!("Version: {}", env!("CARGO_PKG_VERSION")));
    assert!(lines[1].starts_with("Target: "), "{text}");
    assert!(
        lines[2] == "Microsoft client ID: configured"
            || lines[2]
                == "Microsoft client ID: not set. This build cannot sign in with a Microsoft account.",
        "{text}"
    );
    assert!(lines[3].starts_with("Data folder: "), "{text}");
    assert!(lines[4].starts_with("Log folder: "), "{text}");
}

// covers: spec 0002 AC-1 (flag wins, shows its source, creates nothing)
#[test]
fn info_takes_the_data_folder_from_the_flag_and_creates_nothing() {
    let dir = missing_path("flag");
    let env_dir = missing_path("env-loses");
    let flag = dir.to_str().unwrap();
    let output = exodus_with_env(&["info", "--data-dir", flag], env_dir.to_str().unwrap());
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        data_folder_line(&output),
        format!("{} (from --data-dir)", dir.display())
    );
    assert!(!dir.exists());

    let after = exodus(&["--data-dir", flag, "info"]);
    assert_eq!(data_folder_line(&after), data_folder_line(&output));
}

// covers: spec 0002 AC-1 (the environment variable, and empty means unset)
#[test]
fn info_takes_the_data_folder_from_the_environment_unless_it_is_empty() {
    let dir = missing_path("env");
    let output = exodus_with_env(&["info"], dir.to_str().unwrap());
    assert_eq!(
        data_folder_line(&output),
        format!("{} (from EXODUS_DATA_DIR)", dir.display())
    );
    assert!(!dir.exists());

    let empty = exodus_with_env(&["info", "--data-dir", ""], "");
    assert_eq!(empty.status.code(), Some(0), "{}", stderr(&empty));
    assert!(
        data_folder_line(&empty).ends_with(" (system default)"),
        "{}",
        stdout(&empty)
    );
}

// covers: AC-2 (exit code 2 for misuse)
#[test]
fn an_unknown_command_is_misuse_and_exits_with_2() {
    let output = exodus(&["bogus"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("bogus"), "{}", stderr(&output));
    assert!(stdout(&output).is_empty());
}

#[test]
fn no_command_at_all_is_misuse_and_exits_with_2() {
    let output = exodus(&[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("Usage:"), "{}", stderr(&output));
}

#[test]
fn an_unknown_flag_is_misuse_and_exits_with_2() {
    let output = exodus(&["info", "--no-such-flag"]);
    assert_eq!(output.status.code(), Some(2));
}

/// Linux and macOS only: there the data folder follows `HOME`, so a test can
/// give each run its own folder. Windows takes it from the system instead.
#[cfg(unix)]
mod own_data_folder {
    use std::{
        fs,
        io::{BufRead, BufReader},
        path::{Path, PathBuf},
        process::{Child, Command, Stdio},
        sync::mpsc,
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use super::*;

    /// A fresh, empty home folder, removed again when dropped.
    struct TempHome(PathBuf);

    impl TempHome {
        fn new(name: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "exodus-cli-test-{name}-{}-{nanos}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn command(&self, args: &[&str]) -> Command {
            let mut command = Command::new(env!("CARGO_BIN_EXE_exodus"));
            command
                .args(args)
                .env("HOME", &self.0)
                .env_remove("XDG_DATA_HOME")
                .env_remove("EXODUS_DATA_DIR")
                .env_remove("EXODUS_LOG");
            command
        }

        /// The data folder `exodus info` reports for this home.
        fn data_dir(&self) -> PathBuf {
            let output = self.command(&["info"]).output().unwrap();
            let line = data_folder_line(&output);
            let path = line
                .strip_suffix(" (system default)")
                .unwrap_or_else(|| panic!("not the default folder: {line}"));
            PathBuf::from(path)
        }

        fn entries(&self) -> Vec<PathBuf> {
            let mut found = Vec::new();
            collect(&self.0, &mut found);
            found
        }
    }

    impl Drop for TempHome {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn collect(dir: &Path, found: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect(&path, found);
            }
            found.push(path);
        }
    }

    // covers: AC-2 (`exodus info` creates no folders)
    #[test]
    fn info_shows_the_data_folder_inside_home_and_creates_nothing() {
        let home = TempHome::new("info");

        let data_dir = home.data_dir();

        assert!(data_dir.starts_with(&home.0), "{}", data_dir.display());
        assert!(!data_dir.exists());
        assert_eq!(home.entries(), Vec::<PathBuf>::new());
    }

    /// A running `exodus dev-wait --verbose` whose stderr lines arrive on a channel.
    #[cfg(debug_assertions)]
    struct DevWait {
        child: Child,
        lines: mpsc::Receiver<String>,
        seen: Vec<String>,
    }

    #[cfg(debug_assertions)]
    impl DevWait {
        fn start(home: &TempHome) -> Self {
            let mut child = home
                .command(&["dev-wait", "--verbose"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let stderr = child.stderr.take().unwrap();
            let (sender, lines) = mpsc::channel();
            thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    if sender.send(line).is_err() {
                        break;
                    }
                }
            });
            Self {
                child,
                lines,
                seen: Vec::new(),
            }
        }

        /// Waits until a stderr line contains `text`.
        fn wait_for(&mut self, text: &str) {
            loop {
                match self.lines.recv_timeout(Duration::from_secs(15)) {
                    Ok(line) => {
                        let found = line.contains(text);
                        self.seen.push(line);
                        if found {
                            return;
                        }
                    }
                    Err(_) => {
                        let _ = self.child.kill();
                        panic!("no line with {text:?}; stderr so far: {:#?}", self.seen);
                    }
                }
            }
        }

        fn interrupt(&self) {
            let status = Command::new("kill")
                .args(["-INT", &self.child.id().to_string()])
                .status()
                .unwrap();
            assert!(status.success());
        }

        /// Waits for the exit and returns its code plus all stderr.
        fn finish(mut self) -> (Option<i32>, String) {
            let code = self.child.wait().unwrap().code();
            self.seen.extend(self.lines.iter());
            (code, self.seen.join("\n"))
        }
    }

    // covers: AC-6 (Ctrl+C cancels through the token, exit code 130, log file written)
    #[cfg(debug_assertions)]
    #[test]
    fn ctrl_c_cancels_dev_wait_with_exit_code_130_and_writes_the_log_file() {
        let home = TempHome::new("cancel");
        let log_dir = home.data_dir().join("logs");
        let mut run = DevWait::start(&home);

        run.wait_for("still waiting");
        run.interrupt();
        let (code, stderr) = run.finish();

        assert_eq!(code, Some(130), "{stderr}");
        assert!(stderr.contains("interrupted, cancelling"), "{stderr}");
        assert_eq!(stderr.lines().last(), Some("Cancelled."), "{stderr}");
        let files: Vec<_> = fs::read_dir(&log_dir).unwrap().collect();
        assert_eq!(files.len(), 1, "one log file in {}", log_dir.display());
        let name = files[0].as_ref().unwrap().file_name();
        assert!(
            name.to_string_lossy().starts_with("exodus.log."),
            "{name:?}"
        );
    }

    // covers: AC-6 (an unusable log folder warns once and the command still runs)
    #[cfg(debug_assertions)]
    #[test]
    fn an_unusable_log_folder_warns_once_and_dev_wait_still_runs() {
        let home = TempHome::new("nologs");
        let data_dir = home.data_dir();
        fs::create_dir_all(&data_dir).unwrap();
        fs::write(data_dir.join("logs"), "not a folder").unwrap();
        let mut run = DevWait::start(&home);

        run.wait_for("still waiting");
        run.interrupt();
        let (code, stderr) = run.finish();

        assert_eq!(code, Some(130), "{stderr}");
        let warnings = stderr
            .lines()
            .filter(|line| line.contains("Could not use the log folder"))
            .count();
        assert_eq!(warnings, 1, "{stderr}");
        assert!(data_dir.join("logs").is_file());
    }
}
