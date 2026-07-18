use std::process::ExitCode;

use confai::{cli, commands, tui};

use clap::Parser;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();

    // No subcommand means the interactive view; there is nothing else to print.
    let asked_about_updates = matches!(cli.command, Some(cli::Command::Update));
    let result = match cli.command {
        Some(command) => commands::dispatch(command),
        None => tui::run(),
    };

    // Not after `confai update`, which has just said it at length, and not after
    // a failure, where the last line should be the error.
    if result.is_ok() && !asked_about_updates {
        commands::print_update_notice();
    }

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{}: {err:#}", confai::ui::red("error"));
            ExitCode::FAILURE
        }
    }
}
