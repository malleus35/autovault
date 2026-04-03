mod cli;
mod collect;
mod compile;
mod config;
mod conflicts;
mod index;
mod lint;
mod llm;
mod logging;
mod manifest;
mod parser;
mod pipeline;
mod prompts;
mod qa;
mod utils;
mod vault;

use clap::Parser;
use cli::{Cli, Commands};
use config::Config;
use vault::Vault;

// Exit codes per spec: 0=success, 1=general error, 2=lock conflict, 3=LLM failure
const EXIT_LOCK_CONFLICT: i32 = 2;
const EXIT_LLM_FAILURE: i32 = 3;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let config = Config::from_cli(&cli);

    logging::init_tracing(&config.log_level);

    let vault = Vault::new(config.vault_path.clone());

    let result = run_command(&cli.command, &config, &vault).await;

    match result {
        Ok(()) => std::process::exit(0),
        Err(e) => {
            let msg = format!("{:#}", e);
            if msg.contains("Lock file exists") {
                eprintln!("Error: {}", msg);
                std::process::exit(EXIT_LOCK_CONFLICT);
            } else if msg.contains("LLM") || msg.contains("CLI failed") || msg.contains("No LLM backend") {
                eprintln!("Error: {}", msg);
                std::process::exit(EXIT_LLM_FAILURE);
            } else {
                eprintln!("Error: {}", msg);
                std::process::exit(1);
            }
        }
    }
}

async fn run_command(command: &Commands, config: &Config, vault: &Vault) -> anyhow::Result<()> {
    match command {
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
        Commands::Compile => {
            vault.ensure_initialized()?;
            let mut manifest = vault.load_manifest()?;
            let backend = llm::detect_backend()?;
            let result = compile::compile(
                &mut manifest,
                &vault.raw_dir(),
                &vault.wiki_dir(),
                Some(&vault.prompts_dir()),
                backend.as_ref(),
                config.jobs,
                config.dry_run,
                Some(&vault.logs_dir()),
            ).await?;
            vault.save_manifest(&manifest)?;
            if !config.quiet {
                println!(
                    "Compiled: {} new, {} merged, {} errors",
                    result.compiled.len(),
                    result.merged.len(),
                    result.errors.len(),
                );
            }
            Ok(())
        }
        Commands::Index => {
            vault.ensure_initialized()?;
            let manifest = vault.load_manifest()?;
            let backend = llm::detect_backend()?;
            index::build_index(
                &manifest,
                &vault.wiki_dir(),
                Some(&vault.prompts_dir()),
                backend.as_ref(),
            ).await?;
            if !config.quiet {
                println!("Index rebuilt.");
            }
            Ok(())
        }
        Commands::Run { skip } => {
            vault.ensure_initialized()?;
            let backend = llm::detect_backend()?;
            let result = pipeline::run(vault, backend.as_ref(), skip, config.jobs, config.dry_run).await?;
            if !config.quiet {
                println!(
                    "Run complete: {} collected, {} compiled, {} merged, {} errors, {} lint issues, {} QA reviewed",
                    result.collect.new_files.len() + result.collect.modified_files.len(),
                    result.compile.compiled.len(),
                    result.compile.merged.len(),
                    result.compile.errors.len(),
                    result.lint_issues,
                    result.qa_reviewed,
                );
            }
            Ok(())
        }
        Commands::Status { decay } => {
            vault.ensure_initialized()?;
            let manifest = vault.load_manifest()?;

            if *decay {
                // Use wiki-content based evergreen check
                let scores = pipeline::decay_with_evergreen_check(&manifest, &vault.wiki_dir());
                let s = pipeline::status(&manifest, false);
                if config.json {
                    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                        "pending": s.pending,
                        "compiled": s.compiled,
                        "error": s.error,
                        "deleted": s.deleted,
                        "topics": s.topics,
                        "decay_scores": scores,
                    }))?);
                } else {
                    println!("Pending: {}, Compiled: {}, Error: {}, Deleted: {}", s.pending, s.compiled, s.error, s.deleted);
                    for (topic, count) in &s.topics {
                        println!("  {}: {} notes", topic, count);
                    }
                    println!("\nDecay scores (#evergreen exempt):");
                    for (name, score) in &scores {
                        println!("  {}: {:.2}", name, score);
                    }
                }
            } else {
                let s = pipeline::status(&manifest, false);
                if config.json {
                    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                        "pending": s.pending,
                        "compiled": s.compiled,
                        "error": s.error,
                        "deleted": s.deleted,
                        "topics": s.topics,
                    }))?);
                } else {
                    println!("Pending: {}, Compiled: {}, Error: {}, Deleted: {}", s.pending, s.compiled, s.error, s.deleted);
                    for (topic, count) in &s.topics {
                        println!("  {}: {} notes", topic, count);
                    }
                }
            }
            Ok(())
        }
        Commands::Lint { deep, fix } => {
            vault.ensure_initialized()?;
            let backend = if *deep { Some(llm::detect_backend()?) } else { None };
            let result = lint::lint(
                &vault.wiki_dir(),
                Some(&vault.prompts_dir()),
                backend.as_deref(),
                *deep,
                *fix,
            ).await?;
            if config.json {
                let issues: Vec<serde_json::Value> = result.issues.iter().map(|i| {
                    serde_json::json!({"file": i.file, "rule": i.rule, "message": i.message})
                }).collect();
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                    "issues": issues, "fixed": result.fixed
                }))?);
            } else if !config.quiet {
                for issue in &result.issues {
                    println!("[{}] {}: {}", issue.rule, issue.file, issue.message);
                }
                println!("\n{} issues found, {} fixed", result.issues.len(), result.fixed);
            }
            Ok(())
        }
        Commands::Qa { recompile } => {
            vault.ensure_initialized()?;
            let mut manifest = vault.load_manifest()?;
            let backend = llm::detect_backend()?;
            let result = qa::qa(
                &mut manifest,
                &vault.raw_dir(),
                &vault.wiki_dir(),
                Some(&vault.prompts_dir()),
                backend.as_ref(),
                *recompile,
            ).await?;
            if *recompile && !result.recompile_triggered.is_empty() {
                vault.save_manifest(&manifest)?;
            }
            if !config.quiet {
                for review in &result.reviewed {
                    println!("[score={}] {} → {}: {}", review.score, review.raw_file, review.wiki_file, review.feedback);
                }
                if !result.recompile_triggered.is_empty() {
                    println!("{} files marked for recompile", result.recompile_triggered.len());
                }
            }
            Ok(())
        }
        Commands::Conflicts => {
            vault.ensure_initialized()?;
            let backend = llm::detect_backend()?;
            let result = conflicts::detect_conflicts(
                &vault.wiki_dir(),
                &vault.state_dir(),
                Some(&vault.prompts_dir()),
                backend.as_ref(),
            ).await?;
            if config.json {
                println!("{}", serde_json::to_string_pretty(&result.conflicts)?);
            } else if !config.quiet {
                if result.conflicts.is_empty() {
                    println!("No conflicts detected.");
                } else {
                    for c in &result.conflicts {
                        println!(
                            "[{}] {} ↔ {}: {} (tags: {})",
                            c.severity, c.file_a, c.file_b, c.explanation,
                            c.shared_tags.join(", ")
                        );
                    }
                    println!("\n{} conflicts found", result.conflicts.len());
                }
            }
            Ok(())
        }
    }
}
