use anstream::println;
use anstyle::AnsiColor;
use anstyle_hyperlink::Hyperlink;
use clap::Parser;
use tracing::info;

/// {{ project_name }}: A CLI application
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Name to greet
    #[arg(short, long, default_value = "World")]
    name: String,

    #[command(flatten)]
    color: colorchoice_clap::Color,

    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,
}

fn main() -> anyhow::Result<()> {
    human_panic::setup_panic!();

    let cli = Cli::parse_from(wild::args());

    cli.color.write_global();

    if cli.verbose {
        tracing_subscriber::fmt::init();
    }

    let supports_hyperlinks = supports_hyperlinks::supports_hyperlinks();

    info!("Starting {{ project_name }}");
    let name_link = supports_hyperlinks
        .then(|| Hyperlink::with_url(format!("https://crates.io/crates/{}", cli.name)))
        .unwrap_or_default();
    let name_color = AnsiColor::Green.on_default();
    println!(
        "Hello, {name_link}{name_color}{}{name_color:#}{name_link:#}!",
        cli.name
    );
    Ok(())
}

#[test]
fn verify_cli() {
    use clap::CommandFactory;
    Cli::command().debug_assert();
}
