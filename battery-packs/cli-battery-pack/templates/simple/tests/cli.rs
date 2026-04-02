use snapbox::cmd::Command;
use snapbox::file;

#[test]
fn help() {
    Command::cargo_bin("{{ project_name }}")
        .env("CLICOLOR_FORCE", "1")
        .arg("--help")
        .assert()
        .stdout_eq(file![_])
        .stderr_eq("");
}
