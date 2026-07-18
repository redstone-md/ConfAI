//! Argument parsing. Every command here has an equivalent in the TUI.

use std::sync::OnceLock;

use clap::{Args, Parser, Subcommand};

use crate::brand;

#[derive(Debug, Parser)]
#[command(
    name = "confai",
    version,
    about = brand::TAGLINE,
    long_version = long_version(),
    after_help = links(),
    long_about = None,
    arg_required_else_help = false
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// `--version` shows the wordmark; the plain `-V` string stays terse.
///
/// clap wants these for the lifetime of the program, and they are built from
/// constants, so leaking one copy each is the whole cost.
fn long_version() -> &'static str {
    static TEXT: OnceLock<String> = OnceLock::new();
    TEXT.get_or_init(|| {
        // Leading newline because clap prints the binary name before this.
        format!("\n{}\n\n{}\n{}", brand::LOGO.trim_matches('\n'), brand::signature(), links())
    })
    .as_str()
}

fn links() -> &'static str {
    static TEXT: OnceLock<String> = OnceLock::new();
    TEXT.get_or_init(|| format!("{}\n{}", brand::WEBSITE, brand::REPOSITORY)).as_str()
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Show which agents are installed and what they are pointed at.
    #[command(visible_alias = "ls")]
    List,

    /// Add, edit, remove, switch and health-check endpoints.
    #[command(subcommand, visible_alias = "p")]
    Provider(ProviderCommand),

    /// Apply a named endpoint recipe to one agent or all of them.
    #[command(subcommand)]
    Preset(PresetCommand),

    /// List, add, remove and check the MCP servers an agent launches.
    #[command(subcommand)]
    Mcp(McpCommand),

    /// Show or set the model an agent uses.
    Model {
        /// Model to select. Omit to print the current one.
        model: Option<String>,
        #[command(flatten)]
        target: Target,
    },

    /// Print the config file path of each selected agent.
    Path {
        #[command(flatten)]
        target: Target,
    },

    /// Open an agent's config in `$EDITOR`.
    Edit {
        #[command(flatten)]
        target: Target,
    },

    /// Check that every config parses and every referenced provider resolves.
    Doctor,

    /// Show the version, the links and where ConfAI keeps its own state.
    About,

    /// Check whether a newer release exists, and how to get it.
    Update,

    /// Restore the config ConfAI backed up before its last write.
    Undo {
        #[command(flatten)]
        target: Target,
    },
}

#[derive(Debug, Subcommand)]
pub enum ProviderCommand {
    /// List endpoints across the selected agents.
    #[command(visible_alias = "ls")]
    List {
        #[command(flatten)]
        target: Target,
        /// Call each endpoint's `/v1/models` and report whether it answers.
        #[arg(long)]
        check: bool,
    },

    /// Add an endpoint, or edit the fields you pass on an existing one.
    #[command(visible_alias = "set")]
    Add {
        /// Identifier used in the config and by `provider use`.
        id: String,
        #[command(flatten)]
        target: Target,
        #[command(flatten)]
        fields: ProviderFields,
        /// Select this endpoint after writing it.
        #[arg(long = "use")]
        select: bool,
        /// Pull the model list from the endpoint after writing it.
        #[arg(long)]
        sync: bool,
    },

    /// Remove an endpoint.
    #[command(visible_alias = "rm")]
    Remove {
        id: String,
        #[command(flatten)]
        target: Target,
    },

    /// Route an agent through one of its endpoints.
    #[command(visible_alias = "switch")]
    Use {
        id: String,
        #[command(flatten)]
        target: Target,
    },

    /// Ask endpoints whether they are alive and what they serve.
    Check {
        /// Endpoint to check. Omit to check every endpoint of the selected agents.
        id: Option<String>,
        #[command(flatten)]
        target: Target,
        /// Seconds to wait for each endpoint.
        #[arg(long, default_value_t = 10)]
        timeout: u64,
    },

    /// List the models an endpoint serves, with their limits and prices.
    Models {
        /// Endpoint to ask. Omit to use the agent's active one.
        id: Option<String>,
        #[command(flatten)]
        target: Target,
        /// Select one of them as the agent's model.
        #[arg(long, value_name = "MODEL")]
        select: Option<String>,
        /// Re-download the models.dev catalogue instead of using the daily cache.
        #[arg(long)]
        refresh: bool,
    },

    /// Pull an endpoint's model list into the config, with limits from models.dev.
    Sync {
        id: String,
        #[command(flatten)]
        target: Target,
        /// Re-download the models.dev catalogue instead of using the daily cache.
        #[arg(long)]
        refresh: bool,
        /// Also drop models the endpoint no longer serves.
        #[arg(long)]
        prune: bool,
        /// Print what would change without writing it.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum McpCommand {
    /// List the MCP servers configured across the selected agents.
    #[command(visible_alias = "ls")]
    List {
        #[command(flatten)]
        target: Target,
    },

    /// Check that each server could actually start.
    Doctor {
        #[command(flatten)]
        target: Target,
        /// Seconds to wait for a remote server.
        #[arg(long, default_value_t = 10)]
        timeout: u64,
    },

    /// Add a server, or edit the fields you pass on an existing one.
    Add {
        /// Name the agent will know it by.
        name: String,
        #[command(flatten)]
        target: Target,
        /// Executable to launch for a stdio server.
        #[arg(long, value_name = "PROGRAM", conflicts_with = "url")]
        command: Option<String>,
        /// Argument for the command, repeatable and order-preserving.
        #[arg(long = "arg", value_name = "ARG")]
        args: Vec<String>,
        /// Endpoint for a remote server instead of a command.
        #[arg(long, value_name = "URL")]
        url: Option<String>,
        /// Environment variable, repeatable: `--env TOKEN=abc`.
        #[arg(long = "env", value_name = "KEY=VALUE")]
        env: Vec<String>,
    },

    /// Remove a server.
    #[command(visible_alias = "rm")]
    Remove {
        name: String,
        #[command(flatten)]
        target: Target,
    },

    /// Turn a server on or off without removing it, where the agent allows it.
    Toggle {
        name: String,
        /// Turn it off rather than on.
        #[arg(long)]
        off: bool,
        #[command(flatten)]
        target: Target,
    },

    /// Recipes for well-known MCP servers.
    #[command(subcommand)]
    Preset(McpPresetCommand),
}

#[derive(Debug, Subcommand)]
pub enum McpPresetCommand {
    /// List available MCP server recipes.
    #[command(visible_alias = "ls")]
    List,

    /// Write a recipe into the selected agents.
    Apply {
        id: String,
        #[command(flatten)]
        target: Target,
        /// Name to record it under. Defaults to the preset id.
        #[arg(long)]
        name: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum PresetCommand {
    /// List available presets.
    #[command(visible_alias = "ls")]
    List,

    /// Show what a preset would write.
    Show { id: String },

    /// Write a preset's endpoint into the selected agents.
    Apply {
        id: String,
        #[command(flatten)]
        target: Target,
        /// API key for the endpoint. Falls back to the preset's environment variable.
        #[arg(long)]
        api_key: Option<String>,
        /// Select the endpoint after writing it.
        #[arg(long = "use")]
        select: bool,
        /// Pull the model list from the endpoint after writing it.
        #[arg(long)]
        sync: bool,
    },
}

/// Which agents a command applies to.
///
/// Flattened into every subcommand so the selection rules are written once:
/// an explicit `--agent` wins, `--all` means every installed agent, and with
/// neither the command applies to all installed agents when that is safe.
#[derive(Debug, Args, Clone, Default)]
pub struct Target {
    /// Agent to act on, e.g. `codex`, `claude`, `opencode`.
    #[arg(long, short = 'a', value_name = "AGENT")]
    pub agent: Option<String>,

    /// Act on every installed agent.
    #[arg(long, conflicts_with = "agent")]
    pub all: bool,
}

/// Endpoint fields settable from the command line. Unset fields are left alone
/// on an existing endpoint rather than cleared.
#[derive(Debug, Args, Clone, Default)]
pub struct ProviderFields {
    /// Endpoint base URL, e.g. `https://byesu.com/v1`.
    #[arg(long, value_name = "URL")]
    pub base_url: Option<String>,

    /// API key or bearer token.
    #[arg(long, value_name = "KEY")]
    pub api_key: Option<String>,

    /// Wire protocol: `chat`, `responses` or `anthropic`.
    #[arg(long, value_name = "API")]
    pub wire_api: Option<String>,

    /// Human-readable name shown in the agent's UI.
    #[arg(long, value_name = "NAME")]
    pub name: Option<String>,

    /// Backend-specific key, repeatable: `--set requires_openai_auth=true`.
    #[arg(long = "set", value_name = "KEY=VALUE")]
    pub extras: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_command_tree_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn no_arguments_means_the_interactive_ui() {
        let cli = Cli::try_parse_from(["confai"]).unwrap();
        assert!(cli.command.is_none());
    }

    #[test]
    fn agent_and_all_cannot_both_be_given() {
        assert!(Cli::try_parse_from(["confai", "list"]).is_ok());
        assert!(
            Cli::try_parse_from(["confai", "provider", "ls", "--agent", "codex", "--all"]).is_err()
        );
    }

    #[test]
    fn provider_add_collects_repeated_set_flags() {
        let cli = Cli::try_parse_from([
            "confai",
            "provider",
            "add",
            "byesu",
            "--base-url",
            "https://byesu.com/v1",
            "--set",
            "requires_openai_auth=true",
            "--set",
            "supports_websockets=false",
            "--use",
        ])
        .unwrap();

        let Some(Command::Provider(ProviderCommand::Add { id, fields, select, .. })) = cli.command
        else {
            panic!("parsed into the wrong command");
        };
        assert_eq!(id, "byesu");
        assert_eq!(fields.extras.len(), 2);
        assert!(select);
    }

    #[test]
    fn short_aliases_reach_the_same_commands() {
        assert!(matches!(
            Cli::try_parse_from(["confai", "p", "ls"]).unwrap().command,
            Some(Command::Provider(ProviderCommand::List { .. }))
        ));
        assert!(matches!(
            Cli::try_parse_from(["confai", "ls"]).unwrap().command,
            Some(Command::List)
        ));
    }
}
