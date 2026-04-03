mod cli;
mod collect;
mod config;
mod llm;
mod logging;
mod manifest;
mod parser;
mod prompts;
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
        Commands::Collect => {
            vault.ensure_initialized()?;
            let mut manifest = vault.load_manifest()?;
            let result = collect::collect(&vault.raw_dir(), &mut manifest)?;
            vault.save_manifest(&manifest)?;
            if config.json {
                println!("{}", serde_json::json!({
                    "new": result.new_files,
                    "modified": result.modified_files,
                    "deleted": result.deleted_files,
                    "unchanged": result.unchanged_files.len(),
                }));
            } else if !config.quiet {
                println!(
                    "Collected: {} new, {} modified, {} deleted, {} unchanged",
                    result.new_files.len(),
                    result.modified_files.len(),
                    result.deleted_files.len(),
                    result.unchanged_files.len(),
                );
            }
            Ok(())
        }
        Commands::Compile => todo!("compile"),
        Commands::Index => todo!("index"),
        Commands::Run { .. } => todo!("run"),
        Commands::Status { .. } => todo!("status"),
        Commands::Lint { .. } => todo!("lint"),
        Commands::Qa { .. } => todo!("qa"),
        Commands::Conflicts => todo!("conflicts"),
    }
}
