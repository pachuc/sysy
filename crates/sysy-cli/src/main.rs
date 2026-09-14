use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "sysy", about = "System designs as data", version)]
struct Cli {
    /// Print machine-readable JSON instead of text
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the version of this CLI
    Version,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Version => {
            if cli.json {
                println!("{{\"version\":\"{}\"}}", env!("CARGO_PKG_VERSION"));
            } else {
                println!("sysy {}", env!("CARGO_PKG_VERSION"));
            }
        }
    }
}
