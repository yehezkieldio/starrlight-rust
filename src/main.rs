mod cache;
mod cli;
mod error;
mod github;
mod models;
mod output;
mod status;

use clap::Parser;
use std::error::Error;

use crate::cache::{CacheConfig, get_default_cache_dir};
use crate::cli::Cli;
use crate::github::GitHubGQL;
use crate::output::generate_output;
use crate::status::StatusIndicator;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    // Setup cache configuration
    let cache_dir = if let Some(ref cache_dir) = cli.cache_dir {
        std::path::PathBuf::from(cache_dir)
    } else {
        get_default_cache_dir()?
    };

    let cache_config = CacheConfig {
        cache_dir,
        expiry_hours: cli.cache_expiry_hours,
        force_refresh: cli.refresh,
        skip_validation: cli.skip_cache_validation,
    };

    // Handle cache commands
    if let Some(cache_cmd) = &cli.cache {
        use crate::cache::CacheManager;
        use crate::cli::CacheCommand;

        let cache_manager = CacheManager::new(cache_config)?;

        match cache_cmd {
            CacheCommand::Stats => {
                let stats = cache_manager.get_cache_stats()?;
                println!("Cache Statistics:");
                println!("  Total files: {}", stats.total_files);
                println!("  Total size: {}", stats.format_size());
                println!("  Valid entries: {}", stats.valid_entries);
                println!("  Expired entries: {}", stats.expired_entries);

                if let Some(oldest) = stats.oldest_entry {
                    println!("  Oldest entry: {}", oldest.format("%Y-%m-%d %H:%M:%S UTC"));
                }

                if let Some(newest) = stats.newest_entry {
                    println!("  Newest entry: {}", newest.format("%Y-%m-%d %H:%M:%S UTC"));
                }
            }
            CacheCommand::Clear { username } => {
                cache_manager.clear_cache(
                    username,
                    cli.limit,
                    cli.topic_limit,
                    cli.private,
                )?;
                println!("Cache cleared for user: {}", username);
            }
            CacheCommand::ClearAll => {
                cache_manager.clear_all_cache()?;
                println!("All cache files cleared");
            }
        }
        return Ok(());
    }

    // Initialize GitHub client with cache
    let gh = GitHubGQL::new(&cli.token).with_cache(cache_config)?;

    let mut status = StatusIndicator::new(&format!(
        "Processing starred repositories for user: {}",
        cli.username
    ));

    // Choose between streaming and non-streaming modes
    if cli.no_streaming {
        // Original non-streaming approach
        let stars = match gh
            .get_user_starred_by_username(
                &cli.username,
                cli.limit,
                cli.topic_limit,
                cli.private,
                &mut status
            )
            .await
        {
            Ok(stars) => {
                status.finish(Some(&format!(
                    "Successfully processed {} repositories!",
                    stars.len()
                )));
                stars
            }
            Err(e) => {
                status.finish(None);
                eprintln!("Error: {e}");
                return Err(e as Box<dyn Error>);
            }
        };

        generate_output(stars, &cli);
    } else {
        // New streaming approach - memory efficient
        status.update_message("Initializing streaming output...");

        let cli_for_stream = cli.clone();
        match cli.output {
            crate::cli::OutputFormat::Markdown => {
                let mut writer = crate::output::StreamingMarkdownWriter::new(cli_for_stream)?;
                let mut repo_count = 0;

                let result = gh.get_user_starred_by_username_streaming(
                    &cli.username,
                    cli.limit,
                    cli.topic_limit,
                    cli.private,
                    &mut status,
                    |repo| {
                        repo_count += 1;
                        writer.process_repository(repo)
                    }
                ).await;

                match result {
                    Ok(_) => {
                        writer.finalize()?;
                        status.finish(Some(&format!("Successfully processed {} repositories in streaming mode!", repo_count)));
                    }
                    Err(e) => {
                        status.finish(None);
                        eprintln!("Error: {e}");
                        return Err(e as Box<dyn Error>);
                    }
                }
            }
            crate::cli::OutputFormat::Console => {
                let mut writer = crate::output::StreamingConsoleWriter::new(cli_for_stream);
                let mut repo_count = 0;

                let result = gh.get_user_starred_by_username_streaming(
                    &cli.username,
                    cli.limit,
                    cli.topic_limit,
                    cli.private,
                    &mut status,
                    |repo| {
                        repo_count += 1;
                        writer.process_repository(repo)
                    }
                ).await;

                match result {
                    Ok(_) => {
                        writer.finalize()?;
                        status.finish(Some(&format!("Successfully processed {} repositories in streaming mode!", repo_count)));
                    }
                    Err(e) => {
                        status.finish(None);
                        eprintln!("Error: {e}");
                        return Err(e as Box<dyn Error>);
                    }
                }
            }
        }
    }

    Ok(())
}
