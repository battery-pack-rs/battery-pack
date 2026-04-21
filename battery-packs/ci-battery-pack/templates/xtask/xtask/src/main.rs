use std::path::{Path, PathBuf};
use xshell::{Shell, cmd};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(|s| s.as_str()) {
        Some("codegen") => {
            let check = args.iter().any(|a| a == "--check");
            codegen(check)
        }
        Some("tidy") => tidy(),
        Some(cmd) => {
            eprintln!("unknown command: {cmd}");
            eprintln!("usage: cargo xtask <codegen [--check] | tidy>");
            std::process::exit(1);
        }
        None => {
            eprintln!("usage: cargo xtask <codegen [--check] | tidy>");
            std::process::exit(1);
        }
    }
}

fn project_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask should be in a subdirectory")
        .to_path_buf()
}

/// TODO: Replace with your codegen logic.
fn codegen(check: bool) -> Result<()> {
    let _sh = Shell::new()?;

    // TODO: Add your codegen logic here.

    if check {
        println!("codegen --check: all generated files are up to date");
    } else {
        println!("codegen: nothing to generate (add your logic here)");
    }
    Ok(())
}

/// Check for trailing whitespace across the repo.
fn tidy() -> Result<()> {
    let sh = Shell::new()?;
    let root = project_root();
    let _dir = sh.push_dir(&root);
    let output = cmd!(sh, "grep -rn --include=*.rs [[:space:]]$$ src/")
        .ignore_status()
        .read()?;
    if !output.is_empty() {
        return Err(format!("trailing whitespace found:\n{output}").into());
    }
    println!("tidy: no trailing whitespace found");
    Ok(())
}
