## Texts of the `exodus` command line interface.

## General

cli-error = Error: { $message }
cli-cancelled = Cancelled.
cli-log-dir-unavailable = Could not use the log folder { $path } ({ $reason }). Logs go to the terminal only.

## exodus info

info-version = Version: { $version }
info-target = Target: { $target }
info-client-id-configured = Microsoft client ID: configured
info-client-id-missing = Microsoft client ID: not set. This build cannot sign in with a Microsoft account.
info-data-dir = Data folder: { $path } ({ $source ->
        [flag] from --data-dir
        [env] from EXODUS_DATA_DIR
        [portable] portable mode
       *[default] system default
    })
info-data-dir-unknown = Data folder: unknown ({ $reason })
info-log-dir = Log folder: { $path }
info-log-dir-unknown = Log folder: unknown

## exodus dev-wait (debug builds only)

dev-wait-message = Waiting. Press Ctrl+C to cancel.
