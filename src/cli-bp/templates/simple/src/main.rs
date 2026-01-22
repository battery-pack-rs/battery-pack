use cli_bp::clap::Parser;
use cli_bp::tracing::info;

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

fn main() -> cli_bp::anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize tracing if verbose
    if cli.verbose {
        cli_bp::tracing_subscriber::fmt::init();
    }

    info!("Starting {{project-name}}");
    println!("Hello, {}!", cli.name);

    Ok(())
}
