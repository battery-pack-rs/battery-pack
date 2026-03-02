use cli::clap::Parser;
use cli::tracing::info;

/// {{project-name}}: A CLI application
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Name to greet
    #[arg(short, long, default_value = "World")]
    name: String,

    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,
}

fn main() -> cli::anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize tracing if verbose
    if cli.verbose {
        cli::tracing_subscriber::fmt::init();
    }

    info!("Starting {{project-name}}");
    println!("Hello, {}!", cli.name);

    Ok(())
}
