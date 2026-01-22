use cli_bp::clap::{Parser, Subcommand};
use cli_bp::tracing::info;

/// {{project-name}}: A CLI application with subcommands
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

fn main() -> cli_bp::anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize tracing if verbose
    if cli.verbose {
        cli_bp::tracing_subscriber::fmt::init();
    }

    info!("Starting {{project-name}}");

    match cli.command {
        Commands::Hello { name } => {
            println!("Hello, {}!", name);
        }
        Commands::Goodbye { name } => {
            println!("Goodbye, {}!", name);
        }
    }

    Ok(())
}
