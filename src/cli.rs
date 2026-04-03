use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "autovault", version, about = "Automated knowledge management for Obsidian")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(long, global = true)]
    pub vault: Option<PathBuf>,

    #[arg(long, global = true)]
    pub quiet: bool,

    #[arg(long, global = true)]
    pub json: bool,

    #[arg(long, global = true, default_value = "info")]
    pub log_level: LogLevel,

    #[arg(long, global = true)]
    pub dry_run: bool,

    #[arg(long, global = true, default_value = "3")]
    pub jobs: usize,
}

#[derive(Subcommand)]
pub enum Commands {
    Init,
    Collect,
    Compile,
    Index,
    Run {
        #[arg(long)]
        skip: Vec<Stage>,
    },
    Status {
        #[arg(long)]
        decay: bool,
    },
    Lint {
        #[arg(long)]
        deep: bool,
        #[arg(long)]
        fix: bool,
    },
    Qa {
        #[arg(long)]
        recompile: bool,
    },
    Conflicts,
}

#[derive(Clone, ValueEnum)]
pub enum Stage {
    Lint,
    Qa,
}

#[derive(Clone, ValueEnum)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parse_init() {
        let cli = Cli::parse_from(["autovault", "init"]);
        assert!(matches!(cli.command, Commands::Init));
    }

    #[test]
    fn parse_run_with_skip() {
        let cli = Cli::parse_from(["autovault", "run", "--skip", "lint", "--skip", "qa"]);
        match &cli.command {
            Commands::Run { skip } => assert_eq!(skip.len(), 2),
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn parse_global_flags() {
        let cli = Cli::parse_from([
            "autovault", "--vault", "/tmp/v", "--quiet", "--json",
            "--dry-run", "--jobs", "5", "--log-level", "warn", "collect",
        ]);
        assert_eq!(cli.vault.unwrap().to_str().unwrap(), "/tmp/v");
        assert!(cli.quiet);
        assert!(cli.json);
        assert!(cli.dry_run);
        assert_eq!(cli.jobs, 5);
        assert!(matches!(cli.log_level, LogLevel::Warn));
    }

    #[test]
    fn parse_lint_flags() {
        let cli = Cli::parse_from(["autovault", "lint", "--deep", "--fix"]);
        match &cli.command {
            Commands::Lint { deep, fix } => {
                assert!(deep);
                assert!(fix);
            }
            _ => panic!("expected Lint"),
        }
    }

    #[test]
    fn parse_qa_recompile() {
        let cli = Cli::parse_from(["autovault", "qa", "--recompile"]);
        match &cli.command {
            Commands::Qa { recompile } => assert!(recompile),
            _ => panic!("expected Qa"),
        }
    }

    #[test]
    fn parse_status_decay() {
        let cli = Cli::parse_from(["autovault", "status", "--decay"]);
        match &cli.command {
            Commands::Status { decay } => assert!(decay),
            _ => panic!("expected Status"),
        }
    }
}
