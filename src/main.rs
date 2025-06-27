mod cli;
mod error;
mod github;
mod models;
mod output;
mod status;

use clap::Parser;
use std::error::Error;

use crate::cli::Cli;
use crate::github::GitHubGQL;
use crate::output::generate_output;
use crate::status::StatusIndicator;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    let gh = GitHubGQL::new(&cli.token);

    let mut status = StatusIndicator::new(&format!(
        "Fetching starred repositories for user: {}",
        cli.username
    ));

    let stars = match gh
        .get_user_starred_by_username(&cli.username, None, cli.topic_limit, &status)
        .await
    {
        Ok(stars) => {
            status.finish(Some(&format!(
                "Successfully fetched {} repositories!",
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

    Ok(())
}
