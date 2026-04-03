mod cli;
mod config;

use clap::Parser;
use cli::{Cli, Commands};
use config::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let _config = Config::from_cli(&cli);

    match &cli.command {
        Commands::Init => todo!("init"),
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
