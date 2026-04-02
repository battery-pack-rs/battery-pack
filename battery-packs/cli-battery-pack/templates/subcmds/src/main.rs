use anstream::println;
use anstyle::AnsiColor;
use anstyle_hyperlink::Hyperlink;
use clap::{Parser, Subcommand};
use tracing::info;

/// {{ project_name }}: A CLI application with subcommands
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Say hello
    Hello {
        /// Name to greet
        #[arg(short, long, default_value = "World")]
        name: String,
    },
    /// Say goodbye
    Goodbye {
        /// Name to bid farewell
        #[arg(short, long, default_value = "World")]
        name: String,
    },
}

fn main() -> anyhow::Result<()> {
    human_panic::setup_panic!();

    let cli = Cli::parse_from(wild::args());

    if cli.verbose {
        tracing_subscriber::fmt::init();
    }

    let supports_hyperlinks = supports_hyperlinks::supports_hyperlinks();

    info!("Starting {{ project_name }}");
    match cli.command {
        Commands::Hello { name } => {
            let name_link = supports_hyperlinks
                .then(|| Hyperlink::with_url(format!("https://crates.io/crates/{name}")))
                .unwrap_or_default();
            let name_color = AnsiColor::Green.on_default();
            println!("Hello, {name_link}{name_color}{name}{name_color:#}{name_link:#}!");
        }
        Commands::Goodbye { name } => {
            let name_link = supports_hyperlinks
                .then(|| Hyperlink::with_url(format!("https://crates.io/crates/{name}")))
                .unwrap_or_default();
            let name_color = AnsiColor::Cyan.on_default();
            println!("Goodbye, {name_link}{name_color}{name}{name_color:#}{name_link:#}!");
        }
    }

    Ok(())
}

#[test]
fn verify_cli() {
    use clap::CommandFactory;
    Cli::command().debug_assert();
}
