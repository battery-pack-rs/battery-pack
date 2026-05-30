//! Process runner for `cargo bp` JSON commands.
//!
//! Spawns `cargo bp` as a subprocess, captures stdout, and parses
//! the JSON payload into the [schema](crate::status) types.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::process::{Command, ExitStatus};

use crate::list::ListReport;
use crate::show::ShowReport;
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
    #[error("`{command}` exited with {status}: {stderr}")]
    ExitStatus {
        /// The command string that failed (e.g. `"cargo bp status --json"`).
        command: String,
        /// Exit status of the subprocess.
        status: ExitStatus,
        /// Captured stderr (UTF-8 lossy).
        stderr: String,
    },

    /// The subprocess emitted output that could not be parsed as the
    /// expected JSON schema.
    #[error("failed to parse `{command}` output as JSON: {source}")]
    Parse {
        /// The command whose output failed to parse.
        command: String,
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

        let output = spawn(&self.program, &mut cmd)?;
        parse_status(&output)
    }
}

// ============================================================================
// ListCommand
// ============================================================================

/// Builder for invoking `cargo bp list --json` and parsing its output.
///
/// # Example
///
/// ```no_run
/// use cargo_bp_script::ListCommand;
///
/// let report = ListCommand::new().run()?;
/// for pack in &report.packs {
///     println!("{} — {}", pack.short_name, pack.description);
/// }
/// # Ok::<(), cargo_bp_script::Error>(())
/// ```
#[derive(Debug, Clone)]
pub struct ListCommand {
    program: OsString,
    cwd: Option<PathBuf>,
    crate_source: Option<PathBuf>,
    filter: Option<String>,
}

impl Default for ListCommand {
    fn default() -> Self {
        Self {
            program: OsString::from("cargo"),
            cwd: None,
            crate_source: None,
            filter: None,
        }
    }
}

impl ListCommand {
    /// Create a new builder with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the program used to invoke `cargo bp`.
    pub fn program(mut self, program: impl Into<OsString>) -> Self {
        self.program = program.into();
        self
    }

    /// Run the command in a different working directory.
    pub fn cwd(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cwd = Some(dir.into());
        self
    }

    /// Forward `--crate-source <path>` to `cargo bp`.
    pub fn crate_source(mut self, path: impl Into<PathBuf>) -> Self {
        self.crate_source = Some(path.into());
        self
    }

    /// Set an optional filter string to narrow results.
    pub fn filter(mut self, filter: impl Into<String>) -> Self {
        self.filter = Some(filter.into());
        self
    }

    /// Spawn `cargo bp list --json`, capture stdout, and parse it
    /// into a [`ListReport`].
    pub fn run(&self) -> Result<ListReport, Error> {
        // Layout: <program> bp [--crate-source <p>] list --json [<filter>]
        let mut cmd = Command::new(&self.program);
        cmd.arg("bp");
        if let Some(cs) = &self.crate_source {
            cmd.arg("--crate-source").arg(cs);
        }
        cmd.arg("list").arg("--json");
        if let Some(f) = &self.filter {
            cmd.arg(f);
        }
        if let Some(d) = &self.cwd {
            cmd.current_dir(d);
        }

        let output = spawn(&self.program, &mut cmd)?;
        parse_list(&output)
    }
}

// ============================================================================
// ShowCommand
// ============================================================================

/// Builder for invoking `cargo bp show --json <pack>` and parsing its output.
///
/// # Example
///
/// ```no_run
/// use cargo_bp_script::ShowCommand;
///
/// let report = ShowCommand::new("cli").run()?;
/// println!("{} v{}", report.name, report.version);
/// for c in &report.crates {
///     println!("  - {c}");
/// }
/// # Ok::<(), cargo_bp_script::Error>(())
/// ```
#[derive(Debug, Clone)]
pub struct ShowCommand {
    program: OsString,
    cwd: Option<PathBuf>,
    crate_source: Option<PathBuf>,
    path: Option<PathBuf>,
    battery_pack: String,
}

impl ShowCommand {
    /// Create a new builder for the given battery pack name.
    pub fn new(battery_pack: impl Into<String>) -> Self {
        Self {
            program: OsString::from("cargo"),
            cwd: None,
            crate_source: None,
            path: None,
            battery_pack: battery_pack.into(),
        }
    }

    /// Override the program used to invoke `cargo bp`.
    pub fn program(mut self, program: impl Into<OsString>) -> Self {
        self.program = program.into();
        self
    }

    /// Run the command in a different working directory.
    pub fn cwd(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cwd = Some(dir.into());
        self
    }

    /// Forward `--crate-source <path>` to `cargo bp`.
    pub fn crate_source(mut self, path: impl Into<PathBuf>) -> Self {
        self.crate_source = Some(path.into());
        self
    }

    /// Forward `--path <path>` to `cargo bp show`.
    pub fn path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Spawn `cargo bp show --json <pack>`, capture stdout, and parse it
    /// into a [`ShowReport`].
    pub fn run(&self) -> Result<ShowReport, Error> {
        // Layout: <program> bp [--crate-source <p>] show --json [--path <p>] <pack>
        let mut cmd = Command::new(&self.program);
        cmd.arg("bp");
        if let Some(cs) = &self.crate_source {
            cmd.arg("--crate-source").arg(cs);
        }
        cmd.arg("show").arg("--json");
        if let Some(p) = &self.path {
            cmd.arg("--path").arg(p);
        }
        cmd.arg(&self.battery_pack);
        if let Some(d) = &self.cwd {
            cmd.current_dir(d);
        }

        let output = spawn(&self.program, &mut cmd)?;
        parse_show(&output)
    }
}

// ============================================================================
// Parsing helpers
// ============================================================================

/// Parse a `cargo bp status --json` payload into a [`StatusReport`].
///
/// Useful when the caller already has the bytes in hand (for example,
/// from their own subprocess wrapper) and just wants the typed report.
pub fn parse_status(bytes: &[u8]) -> Result<StatusReport, Error> {
    serde_json::from_slice(bytes).map_err(|source| Error::Parse {
        command: "cargo bp status --json".into(),
        source,
    })
}

/// Parse a `cargo bp list --json` payload into a [`ListReport`].
pub fn parse_list(bytes: &[u8]) -> Result<ListReport, Error> {
    serde_json::from_slice(bytes).map_err(|source| Error::Parse {
        command: "cargo bp list --json".into(),
        source,
    })
}

/// Parse a `cargo bp show --json` payload into a [`ShowReport`].
pub fn parse_show(bytes: &[u8]) -> Result<ShowReport, Error> {
    serde_json::from_slice(bytes).map_err(|source| Error::Parse {
        command: "cargo bp show --json".into(),
        source,
    })
}

// ============================================================================
// Internal helpers
// ============================================================================

/// Spawn a command and return its stdout on success, or an appropriate error.
fn spawn(program: &OsStr, cmd: &mut Command) -> Result<Vec<u8>, Error> {
    let output = cmd.output().map_err(|source| Error::Spawn {
        program: program_display(program),
        source,
    })?;
    if !output.status.success() {
        return Err(Error::ExitStatus {
            command: format_command(cmd),
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(output.stdout)
}

/// Best-effort display string for an `OsStr`, used only for error messages.
fn program_display(program: &OsStr) -> String {
    program.to_string_lossy().into_owned()
}

/// Best-effort reconstruction of the command line for error messages.
fn format_command(cmd: &Command) -> String {
    let prog = cmd.get_program().to_string_lossy();
    let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy()).collect();
    if args.is_empty() {
        prog.into_owned()
    } else {
        format!("{} {}", prog, args.join(" "))
    }
}
