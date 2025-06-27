use clap::Parser;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
   #[arg(long)]
    username: String,

    #[arg(long, env = "GITHUB_TOKEN")]
    token: String,
}

fn main() {
    let cli = Cli::parse();

    println!("Username: {}", cli.username);
    println!("Token: {}", cli.token);
}
