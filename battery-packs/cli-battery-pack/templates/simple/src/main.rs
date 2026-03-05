use clap::Parser;
use tracing::info;

/// {{ project_name }}: A CLI application
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

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if cli.verbose {
        tracing_subscriber::fmt::init();
    }

    info!("Starting {{ project_name }}");
    println!("Hello, {}!", cli.name);
    Ok(())
}
