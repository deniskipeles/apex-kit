use clap::Parser;
use commands::Commands;

mod commands;

#[derive(Parser, Debug)]
#[command(author, version, about = "ApexKit CLI & Server Entrypoint", long_about = None)]
pub struct Cli {
    /// Port to run the server on (reads --port CLI flag, PORT env var, or defaults to 5000)
    #[arg(short, long, env = "PORT", default_value_t = 5000)]
    pub port: u16,

    /// Subcommands for system management (skips starting HTTP server if used)
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[tokio::main]
async fn main() {
    // 1. Load the .env file and FORCE it to override the container's environment variables
    dotenvy::dotenv_override().ok();

    // Initialize standard logging for the console
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    // If a subcommand was passed, execute it and exit immediately.
    if let Some(cmd) = cli.command {
        match commands::execute(cmd).await {
            Ok(_) => std::process::exit(0),
            Err(e) => {
                eprintln!("❌ CLI Error: {}", e);
                std::process::exit(1);
            }
        }
    }

    // No subcommand provided: Boot the API Server with the resolved port
    tracing::info!("Starting ApexKit API Server on port {}", cli.port);
    apexkit_api::start(cli.port).await;
}
