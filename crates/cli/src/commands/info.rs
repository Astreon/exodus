//! `exodus info`: build facts and folders. No network, creates no folders.

use std::io::Write;

use exodus_core::{ErrorView, MessageArgs, build_info, config, i18n::message};

use super::Context;
use crate::failure::Failure;

pub fn run(context: &Context, out: &mut impl Write) -> Result<(), Failure> {
    for line in lines(context) {
        writeln!(out, "{line}")?;
    }
    Ok(())
}

fn lines(context: &Context) -> Vec<String> {
    let info = build_info();
    let none = MessageArgs::new();
    let mut lines = vec![
        message(
            "info-version",
            &MessageArgs::new().with("version", info.version),
        ),
        message(
            "info-target",
            &MessageArgs::new().with("target", info.target),
        ),
        if info.msa_client_id_configured {
            message("info-client-id-configured", &none)
        } else {
            message("info-client-id-missing", &none)
        },
    ];
    match &context.data_dir {
        Ok(dir) => {
            lines.push(message(
                "info-data-dir",
                &MessageArgs::new().with("path", dir.as_path()),
            ));
            let logs = config::log_dir(dir);
            lines.push(message(
                "info-log-dir",
                &MessageArgs::new().with("path", logs.as_path()),
            ));
        }
        Err(error) => {
            let reason = ErrorView::from(error).message();
            lines.push(message(
                "info-data-dir-unknown",
                &MessageArgs::new().with("reason", reason),
            ));
            lines.push(message("info-log-dir-unknown", &none));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use exodus_core::{CancellationToken, config::ConfigError, i18n::try_message};
    use indicatif::MultiProgress;

    use super::*;

    fn context(data_dir: Result<PathBuf, ConfigError>) -> Context {
        Context {
            data_dir,
            progress: MultiProgress::new(),
            cancel: CancellationToken::new(),
        }
    }

    #[test]
    fn every_cli_text_exists_and_gets_its_variables() {
        let text = MessageArgs::new()
            .with("message", "m")
            .with("path", "p")
            .with("reason", "r")
            .with("version", "v")
            .with("target", "t");
        for id in [
            "cli-error",
            "cli-cancelled",
            "cli-log-dir-unavailable",
            "info-version",
            "info-target",
            "info-client-id-configured",
            "info-client-id-missing",
            "info-data-dir",
            "info-data-dir-unknown",
            "info-log-dir",
            "info-log-dir-unknown",
            "dev-wait-message",
        ] {
            try_message(id, &text).unwrap_or_else(|problem| panic!("{problem}"));
        }
    }

    #[test]
    fn shows_folders_and_never_the_client_id() {
        let dir = PathBuf::from("some").join("data");
        let mut out = Vec::new();
        run(&context(Ok(dir.clone())), &mut out).unwrap();
        let out = String::from_utf8(out).unwrap();

        assert!(out.contains(&format!("Version: {}", env!("CARGO_PKG_VERSION"))));
        assert!(out.contains(&format!("Data folder: {}", dir.display())));
        assert!(out.contains(&format!("Log folder: {}", dir.join("logs").display())));
        let client_id = option_env!("EXODUS_MSA_CLIENT_ID")
            .unwrap_or_default()
            .trim();
        if !client_id.is_empty() {
            assert!(!out.contains(client_id));
        }
    }

    #[test]
    fn explains_an_unknown_data_folder() {
        let mut out = Vec::new();
        run(&context(Err(ConfigError::NoHomeDir)), &mut out).unwrap();
        let out = String::from_utf8(out).unwrap();
        assert!(out.contains("Data folder: unknown (Could not find your home folder"));
        assert!(out.contains("Log folder: unknown"));
    }
}
