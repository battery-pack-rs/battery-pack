use snapbox::cmd::Command;
use snapbox::{Assert, Redactions, file};

#[test]
fn help() {
    // Normalize the platform-specific executable name before matching the snapshot.
    let mut redactions = Redactions::new();
    redactions
        .insert(
            "[BIN]",
            format!("{}{}", env!("CARGO_PKG_NAME"), std::env::consts::EXE_SUFFIX),
        )
        .unwrap();
    let assertion = Assert::new()
        .action_env("SNAPSHOTS")
        .redact_with(redactions);

    // Exercise the generated CLI and verify its complete help output.
    Command::cargo_bin("{{ project_name }}")
        .with_assert(assertion)
        .env_remove("NO_COLOR")
        .env("CLICOLOR_FORCE", "1")
        .arg("--help")
        .assert()
        .stdout_eq(file![_])
        .stderr_eq("");
}
