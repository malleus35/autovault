use std::path::PathBuf;

use crate::cli::{Cli, LogLevel};

pub struct Config {
    pub vault_path: PathBuf,
    pub quiet: bool,
    pub json: bool,
    pub log_level: LogLevel,
    pub dry_run: bool,
    pub jobs: usize,
}

impl Config {
    pub fn from_cli(cli: &Cli) -> Self {
        let vault_path = cli
            .vault
            .clone()
            .or_else(|| std::env::var("AUTOVAULT_PATH").ok().map(PathBuf::from))
            .unwrap_or_else(|| {
                dirs_home().join("autovault")
            });

        Config {
            vault_path,
            quiet: cli.quiet,
            json: cli.json,
            log_level: cli.log_level.clone(),
            dry_run: cli.dry_run,
            jobs: cli.jobs,
        }
    }
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use clap::Parser;

    #[test]
    fn config_from_cli_with_vault_flag() {
        let cli = Cli::parse_from(["autovault", "--vault", "/tmp/myvault", "init"]);
        let config = Config::from_cli(&cli);
        assert_eq!(config.vault_path, PathBuf::from("/tmp/myvault"));
    }

    #[test]
    fn config_from_cli_defaults() {
        let cli = Cli::parse_from(["autovault", "init"]);
        let config = Config::from_cli(&cli);
        assert!(!config.quiet);
        assert!(!config.json);
        assert!(!config.dry_run);
        assert_eq!(config.jobs, 3);
    }

    #[test]
    fn config_uses_env_fallback() {
        std::env::set_var("AUTOVAULT_PATH", "/tmp/envvault");
        let cli = Cli::parse_from(["autovault", "init"]);
        let config = Config::from_cli(&cli);
        assert_eq!(config.vault_path, PathBuf::from("/tmp/envvault"));
        std::env::remove_var("AUTOVAULT_PATH");
    }
}
