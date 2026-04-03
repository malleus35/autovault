mod cli;
mod config;
mod logging;
mod manifest;
mod utils;
mod vault;

use clap::Parser;
use cli::{Cli, Commands};
use config::Config;
use vault::Vault;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::from_cli(&cli);

    logging::init_tracing(&config.log_level);

    let vault = Vault::new(config.vault_path.clone());

    match &cli.command {
        Commands::Init => {
            vault.init()?;
            if !config.quiet {
                println!("Vault initialized at {}", config.vault_path.display());
            }
            Ok(())
        }
        Commands::Collect => todo!("collect"),
        Commands::Compile => todo!("compile"),
        Commands::Index => todo!("index"),
        Commands::Run { .. } => todo!("run"),
        Commands::Status { .. } => todo!("status"),
        Commands::Lint { .. } => todo!("lint"),
        Commands::Qa { .. } => todo!("qa"),
        Commands::Conflicts => todo!("conflicts"),
    }
}
