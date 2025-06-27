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

    /// Limit the number of repositories to fetch (useful for debugging)
    #[arg(long)]
    pub limit: Option<usize>,

    /// Output format: console (default) or markdown
    #[arg(long, value_enum, default_value = "console")]
    pub output: OutputFormat,

    /// Output directory for markdown files (when using markdown output)
    #[arg(long, default_value = "./awesome-stars")]
    pub output_dir: String,

    /// Maximum file size for markdown files in KB (default: 500KB)
    #[arg(long, default_value_t = 500)]
    pub max_file_size_kb: usize,

    /// Repository name (for GitHub integration - not implemented)
    #[arg(long, default_value = "")]
    pub repository: String,

    /// File name (for GitHub integration - not implemented)
    #[arg(long, default_value = "README.md")]
    pub filename: String,

    /// Commit message (for GitHub integration - not implemented)
    #[arg(long, default_value = "update awesome-stars, created by starred")]
    pub message: String,

    /// Include private repos
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub private: bool,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum OutputFormat {
    Console,
    Markdown,
}
