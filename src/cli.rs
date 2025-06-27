use clap::Parser;

const VERSION: &str = "0.1.0";

#[derive(Parser)]
#[command(author, version = VERSION, about, long_about = None)]
pub struct Cli {
    /// GitHub username
    #[arg(long, env = "USER")]
    pub username: String,

    /// GitHub token
    #[arg(long, env = "GITHUB_TOKEN")]
    pub token: String,

    /// Sort by category[language/topic] name alphabetically
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub sort: bool,

    /// Category by topic, default is category by language
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub topic: bool,

    /// Topic stargazer_count gt number, set bigger to reduce topics number
    #[arg(long, default_value_t = 500)]
    pub topic_limit: i32,

    /// Repository name
    #[arg(long, default_value = "")]
    pub repository: String,

    /// File name
    #[arg(long, default_value = "README.md")]
    pub filename: String,

    /// Commit message
    #[arg(long, default_value = "update awesome-stars, created by starred")]
    pub message: String,

    /// Include private repos
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub private: bool,
}
