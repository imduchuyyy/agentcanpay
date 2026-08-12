mod cli;
mod commands;
mod output;

use clap::Parser;
use cli::{Cli, Command};
use output::Output;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let out = Output::new(cli.json);

    let result = match &cli.command {
        Command::Create(args) => commands::create::run(args, &out).await,
        Command::Address(args) => commands::address::run(args, &out),
        Command::Reveal(args) => commands::reveal::run(args, &out).await,
        Command::Balance(args) => commands::balance::run(args, &out).await,
        Command::Chains(args) => commands::chains::run(args, &out).await,
        Command::Transfer(args) => commands::transfer::run(args, &out).await,
    };

    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            out.error(&e);
            e.exit_code()
        }
    }
}
