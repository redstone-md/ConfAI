use std::process::ExitCode;

use confai::{cli, commands, tui};

use clap::Parser;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();

    // No subcommand means the interactive view; there is nothing else to print.
    let result = match cli.command {
        Some(command) => commands::dispatch(command),
        None => tui::run(),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{}: {err:#}", confai::ui::red("error"));
            ExitCode::FAILURE
        }
    }
}
