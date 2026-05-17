//! Process runner for `cargo bp` JSON commands.
//!
//! Spawns `cargo bp` as a subprocess, captures stdout, and parses
//! the JSON payload into the [schema](crate::status) types.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::process::{Command, ExitStatus};

use crate::status::StatusReport;

/// Error returned by the runner.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The subprocess could not be spawned at all (e.g. binary not on `$PATH`).
    #[error("failed to spawn `{program}`: {source}")]
    Spawn {
        /// The program that failed to spawn (typically `"cargo"`).
        program: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The subprocess ran but exited with a non-zero status.
    #[error("`cargo bp status --json` exited with {status}: {stderr}")]
    ExitStatus {
        /// Exit status of the subprocess.
        status: ExitStatus,
        /// Captured stderr (UTF-8 lossy).
        stderr: String,
    },

    /// The subprocess emitted output that could not be parsed as the
    /// expected JSON schema.
    #[error("failed to parse `cargo bp status --json` output as JSON: {source}")]
    Parse {
        /// Underlying parse error from `serde_json`.
        #[source]
        source: serde_json::Error,
    },
}

/// Builder for invoking `cargo bp status --json` and parsing its output.
///
/// # Example
///
/// ```no_run
/// use cargo_bp_script::StatusCommand;
///
/// let report = StatusCommand::new().run()?;
/// for pack in &report.packs {
///     println!("{} {}", pack.short_name, pack.version);
/// }
/// # Ok::<(), cargo_bp_script::Error>(())
/// ```
#[derive(Debug, Clone)]
pub struct StatusCommand {
    program: OsString,
    cwd: Option<PathBuf>,
    crate_source: Option<PathBuf>,
    path: Option<PathBuf>,
}

impl Default for StatusCommand {
    fn default() -> Self {
        Self {
            program: OsString::from("cargo"),
            cwd: None,
            crate_source: None,
            path: None,
        }
    }
}

impl StatusCommand {
    /// Create a new builder. By default the runner invokes the `cargo`
    /// binary on `$PATH` so that `cargo bp` is dispatched to the
    /// installed `cargo-bp` subcommand.
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the program used to invoke `cargo bp`.
    ///
    /// The default is `"cargo"`, which means the runner spawns
    /// `cargo bp status --json`. You may instead point at a directly
    /// built `cargo-bp` binary (typically used in tests):
    ///
    /// ```no_run
    /// # use cargo_bp_script::StatusCommand;
    /// let report = StatusCommand::new()
    ///     .program("/path/to/target/debug/cargo-bp")
    ///     .run()?;
    /// # Ok::<(), cargo_bp_script::Error>(())
    /// ```
    ///
    /// In either case the runner appends `bp status --json`, which
    /// works because `cargo-bp`'s top-level command is `bp`.
    pub fn program(mut self, program: impl Into<OsString>) -> Self {
        self.program = program.into();
        self
    }

    /// Run the command in a different working directory. Defaults to
    /// the current process's working directory.
    pub fn cwd(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cwd = Some(dir.into());
        self
    }

    /// Forward `--crate-source <path>` to `cargo bp`.
    pub fn crate_source(mut self, path: impl Into<PathBuf>) -> Self {
        self.crate_source = Some(path.into());
        self
    }

    /// Forward `--path <path>` to `cargo bp status`.
    pub fn path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Spawn `cargo bp status --json`, capture stdout, and parse it
    /// into a [`StatusReport`].
    pub fn run(&self) -> Result<StatusReport, Error> {
        // --- Build the command line.
        // Layout: <program> bp [--crate-source <p>] status --json [--path <p>]
        let mut cmd = Command::new(&self.program);
        cmd.arg("bp");
        if let Some(cs) = &self.crate_source {
            cmd.arg("--crate-source").arg(cs);
        }
        cmd.arg("status").arg("--json");
        if let Some(p) = &self.path {
            cmd.arg("--path").arg(p);
        }
        if let Some(d) = &self.cwd {
            cmd.current_dir(d);
        }

        // --- Spawn and capture output.
        let output = cmd.output().map_err(|source| Error::Spawn {
            program: program_display(&self.program),
            source,
        })?;
        if !output.status.success() {
            return Err(Error::ExitStatus {
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        // --- Parse JSON payload.
        parse_status(&output.stdout)
    }
}

/// Parse a `cargo bp status --json` payload into a [`StatusReport`].
///
/// Useful when the caller already has the bytes in hand (for example,
/// from their own subprocess wrapper) and just wants the typed report.
pub fn parse_status(bytes: &[u8]) -> Result<StatusReport, Error> {
    serde_json::from_slice(bytes).map_err(|source| Error::Parse { source })
}

/// Best-effort display string for an `OsStr`, used only for error messages.
fn program_display(program: &OsStr) -> String {
    program.to_string_lossy().into_owned()
}
