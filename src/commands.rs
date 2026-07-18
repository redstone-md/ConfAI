//! Command implementations. The CLI and the TUI both end up here.

use std::time::Duration;

use anyhow::{bail, Context, Result};

use crate::agent::{self, Agent, AgentConfig};
use crate::brand;
use crate::cli::{
    Command, McpCommand, McpPresetCommand, PresetCommand, ProviderCommand, ProviderFields, Target,
};
use crate::domain::{mask, Provider, WireApi};
use crate::mcp;
use crate::net;
use crate::preset;
use crate::store;
use crate::ui::{self, Table};

pub fn dispatch(command: Command) -> Result<()> {
    match command {
        Command::List => list_agents(),
        Command::Provider(cmd) => provider(cmd),
        Command::Preset(cmd) => preset_command(cmd),
        Command::Mcp(cmd) => mcp_command(cmd),
        Command::Model { model, target } => model_command(model, &target),
        Command::Path { target } => paths(&target),
        Command::Edit { target } => edit(&target),
        Command::Doctor => doctor(),
        Command::About => about(),
        Command::Update => update(),
        Command::Undo { target } => undo(&target),
    }
}

/// The agents a command applies to.
///
/// A named agent is used whether or not it looks installed — the user asked for
/// it by name, and refusing because a binary is not on `PATH` would block
/// perfectly valid config edits. Otherwise the command applies to what is here.
fn resolve(target: &Target) -> Result<Vec<Box<dyn Agent>>> {
    if let Some(name) = &target.agent {
        return Ok(vec![agent::find(name)?]);
    }
    let installed = agent::installed();
    if installed.is_empty() {
        bail!("no AI agents detected; run `confai list` to see where ConfAI looked");
    }
    Ok(installed)
}

/// Resolve to exactly one agent, so a destructive edit is never applied blindly
/// to several configs at once.
fn resolve_one(target: &Target) -> Result<Box<dyn Agent>> {
    let mut agents = resolve(target)?;
    if agents.len() > 1 {
        let names: Vec<&str> = agents.iter().map(|a| a.info().id).collect();
        bail!(
            "several agents are installed ({}); pick one with --agent, or use --all",
            names.join(", ")
        );
    }
    Ok(agents.remove(0))
}

fn list_agents() -> Result<()> {
    let mut table = Table::new(["agent", "detected", "providers", "active", "model", "config"]);

    for entry in agent::all() {
        let info = entry.info();
        let detection = entry.detect();

        let (providers, active, model) = match entry.load() {
            Ok(config) => (
                config.providers().len().to_string(),
                config.active_provider().unwrap_or_else(|| "-".into()),
                config.model().unwrap_or_else(|| "-".into()),
            ),
            Err(_) if !detection.installed() => ("-".into(), "-".into(), "-".into()),
            Err(err) => (ui::red("unreadable"), ui::dim(&err.to_string()), "-".into()),
        };

        table.row([
            ui::bold(info.name),
            if detection.installed() {
                ui::green(detection.describe())
            } else {
                ui::dim(detection.describe())
            },
            providers,
            active,
            ui::truncate(&model, 28),
            ui::dim(&info.config_path.display().to_string()),
        ]);
    }

    print!("{}", table.render());
    Ok(())
}

fn provider(command: ProviderCommand) -> Result<()> {
    match command {
        ProviderCommand::List { target, check } => provider_list(&target, check),
        ProviderCommand::Add { id, target, fields, select, sync } => {
            provider_add(&id, &target, &fields, select, sync)
        }
        ProviderCommand::Remove { id, target } => provider_remove(&id, &target),
        ProviderCommand::Use { id, target } => provider_use(&id, &target),
        ProviderCommand::Check { id, target, timeout } => {
            provider_check(id.as_deref(), &target, Duration::from_secs(timeout))
        }
        ProviderCommand::Models { id, target, select, refresh } => {
            provider_models(id.as_deref(), &target, select.as_deref(), refresh)
        }
        ProviderCommand::Sync { id, target, refresh, prune, dry_run } => {
            provider_sync(&id, &target, refresh, prune, dry_run)
        }
    }
}

fn provider_list(target: &Target, check: bool) -> Result<()> {
    let mut table = Table::new(["agent", "id", "active", "url", "key", "wire", "models"]);
    if check {
        table = Table::new(["agent", "id", "active", "url", "wire", "status"]);
    }

    for entry in resolve(target)? {
        let config = match entry.load() {
            Ok(config) => config,
            Err(err) => {
                eprintln!("{}: {err:#}", ui::yellow(entry.info().name));
                continue;
            }
        };
        let active = config.active_provider();

        for provider in config.providers() {
            let is_active = active.as_deref() == Some(provider.id.as_str());
            let marker = if is_active { ui::green("*") } else { " ".into() };
            let url = provider.base_url.clone().unwrap_or_else(|| "-".into());
            let wire = provider.wire_api.map(|w| w.to_string()).unwrap_or_else(|| "-".into());

            if check {
                let status = match &provider.base_url {
                    Some(_) => {
                        let probe = net::probe::probe(
                            provider.base_url.as_deref().unwrap_or_default(),
                            provider.api_key.as_deref(),
                            provider.wire_api,
                            net::DEFAULT_TIMEOUT,
                        );
                        if probe.alive() {
                            ui::green(&probe.summary())
                        } else {
                            ui::red(&probe.summary())
                        }
                    }
                    None => ui::dim("no url"),
                };
                table.row([
                    entry.info().id.to_string(),
                    provider.id.clone(),
                    marker,
                    ui::truncate(&url, 42),
                    wire,
                    status,
                ]);
            } else {
                table.row([
                    entry.info().id.to_string(),
                    provider.id.clone(),
                    marker,
                    ui::truncate(&url, 46),
                    provider.api_key.as_deref().map(mask).unwrap_or_else(|| ui::dim("-")),
                    wire,
                    if provider.models.is_empty() {
                        ui::dim("-")
                    } else {
                        provider.models.len().to_string()
                    },
                ]);
            }
        }
    }

    if table.is_empty() {
        println!("{}", ui::dim("no providers configured"));
        return Ok(());
    }
    print!("{}", table.render());
    Ok(())
}

/// Build a [`Provider`] from command-line fields.
fn provider_from_fields(id: &str, fields: &ProviderFields) -> Result<Provider> {
    let wire_api = match &fields.wire_api {
        Some(raw) => Some(WireApi::parse(raw).with_context(|| {
            format!(
                "unknown wire API {raw:?}; expected one of: {}",
                WireApi::ALL.map(|w| w.as_str()).join(", ")
            )
        })?),
        None => None,
    };

    let mut provider = Provider::new(id);
    provider.display_name = fields.name.clone();
    provider.base_url = fields.base_url.clone();
    provider.api_key = fields.api_key.clone();
    provider.wire_api = wire_api;

    for raw in &fields.extras {
        let (key, value) =
            raw.split_once('=').with_context(|| format!("--set expects KEY=VALUE, got {raw:?}"))?;
        provider.extras.insert(key.trim().to_string(), value.trim().to_string());
    }
    Ok(provider)
}

fn provider_add(
    id: &str,
    target: &Target,
    fields: &ProviderFields,
    select: bool,
    sync: bool,
) -> Result<()> {
    let provider = provider_from_fields(id, fields)?;
    let agents = if target.all { resolve(target)? } else { vec![resolve_one(target)?] };

    for entry in agents {
        let mut config = entry.load()?;
        let existed = config.provider(id).is_some();
        config.upsert_provider(&provider)?;

        if sync {
            sync_into(config.as_mut(), id, false, false)?;
        }
        if select {
            config.set_active_provider(id)?;
        }
        config.save()?;

        let verb = if existed { "updated" } else { "added" };
        println!("{} {verb} {} in {}", ui::green("✓"), ui::bold(id), entry.info().name);
        if select {
            println!("  {} now routes through {id}", entry.info().name);
        }
    }
    Ok(())
}

fn provider_remove(id: &str, target: &Target) -> Result<()> {
    let agents = if target.all { resolve(target)? } else { vec![resolve_one(target)?] };
    let mut removed_anywhere = false;

    for entry in agents {
        let mut config = entry.load()?;
        if !config.remove_provider(id)? {
            continue;
        }
        config.save()?;
        removed_anywhere = true;
        println!("{} removed {} from {}", ui::green("✓"), ui::bold(id), entry.info().name);
    }

    if !removed_anywhere {
        bail!("no selected agent has a provider called {id:?}");
    }
    Ok(())
}

fn provider_use(id: &str, target: &Target) -> Result<()> {
    // Without an explicit agent, switch every agent that actually has this
    // endpoint. That is the point of a shared id: one command, one switch.
    let agents = resolve(target)?;
    let mut switched = Vec::new();
    let mut failures = Vec::new();

    for entry in agents {
        let mut config = match entry.load() {
            Ok(config) => config,
            Err(err) => {
                failures.push(format!("{}: {err:#}", entry.info().name));
                continue;
            }
        };
        if config.provider(id).is_none() {
            continue;
        }
        match config.set_active_provider(id).and_then(|()| config.save()) {
            Ok(()) => switched.push(entry.info().name.to_string()),
            Err(err) => failures.push(format!("{}: {err:#}", entry.info().name)),
        }
    }

    for failure in &failures {
        eprintln!("{} {failure}", ui::yellow("!"));
    }
    if switched.is_empty() {
        bail!("no selected agent has a provider called {id:?}");
    }
    println!("{} {} now routes through {}", ui::green("✓"), switched.join(", "), ui::bold(id));
    Ok(())
}

fn provider_check(id: Option<&str>, target: &Target, timeout: Duration) -> Result<()> {
    let mut checked = 0;

    for entry in resolve(target)? {
        let config = entry.load()?;
        for provider in config.providers() {
            if id.is_some_and(|wanted| wanted != provider.id) {
                continue;
            }
            let Some(base_url) = provider.base_url.as_deref() else {
                println!("{} {}/{}: no base URL", ui::dim("-"), entry.info().id, provider.id);
                checked += 1;
                continue;
            };

            let result = net::probe::probe(
                base_url,
                provider.api_key.as_deref(),
                provider.wire_api,
                timeout,
            );
            let (icon, summary) = if result.alive() {
                (ui::green("✓"), ui::green(&result.summary()))
            } else {
                (ui::red("✗"), ui::red(&result.summary()))
            };
            println!(
                "{icon} {}/{}  {}  {summary}",
                entry.info().id,
                ui::bold(&provider.id),
                ui::dim(&result.url)
            );
            checked += 1;
        }
    }

    if checked == 0 {
        match id {
            Some(id) => bail!("no selected agent has a provider called {id:?}"),
            None => println!("{}", ui::dim("no providers configured")),
        }
    }
    Ok(())
}

/// List what an endpoint serves, and optionally select one of those models.
///
/// This works for every agent, including the ones whose config has no
/// per-provider model list. Those agents still name a model, and until now the
/// list an endpoint answered with was discovered and then thrown away.
fn provider_models(
    id: Option<&str>,
    target: &Target,
    select: Option<&str>,
    refresh: bool,
) -> Result<()> {
    let entry = resolve_one(target)?;
    let mut config = entry.load()?;

    let provider = match id {
        Some(id) => config
            .provider(id)
            .with_context(|| format!("{} has no provider {id:?}", entry.info().name))?,
        None => {
            let active = config.active_provider().with_context(|| {
                format!("{} has no active provider; name one", entry.info().name)
            })?;
            config.provider(&active).with_context(|| {
                format!("{} points at {active:?}, which is not configured", entry.info().name)
            })?
        }
    };

    let discovery = net::discover_models(&provider, net::DEFAULT_TIMEOUT, refresh);
    let Some(probe) = &discovery.probe else {
        bail!("provider {:?} has no base URL to query", provider.id);
    };
    if !probe.alive() {
        bail!("{} did not answer: {}", probe.url, probe.summary());
    }

    if let Some(wanted) = select {
        let model = discovery
            .models
            .iter()
            .find(|model| model.id == wanted)
            .with_context(|| format!("{} does not serve {wanted:?}", provider.id))?;

        config.set_model_for(&provider.id, &model.id)?;
        config.save()?;
        println!(
            "{} {} now uses {} from {}",
            ui::green("✓"),
            entry.info().name,
            ui::bold(&model.id),
            provider.id
        );
        return Ok(());
    }

    let catalog = net::catalog::Catalog::cached_only();
    let current = config.model();
    let mut table = Table::new(["", "model", "context", "output", "price"]);

    for model in &discovery.models {
        let facts = catalog.as_ref().and_then(|c| c.lookup(&model.id));
        let selected = current
            .as_deref()
            .is_some_and(|m| m == model.id || m.ends_with(&format!("/{}", model.id)));

        table.row([
            if selected { ui::green("*") } else { " ".into() },
            model.id.clone(),
            model.context_limit.map(ui::tokens).unwrap_or_else(|| ui::dim("-")),
            model.output_limit.map(ui::tokens).unwrap_or_else(|| ui::dim("-")),
            facts.and_then(|f| f.price()).unwrap_or_else(|| ui::dim("-")),
        ]);
    }

    println!("{} {}\n", ui::bold(&provider.id), ui::dim(probe.url.as_str()));
    print!("{}", table.render());
    println!(
        "\n{}",
        ui::dim(&format!(
            "select one with `confai provider models {} --select <model>`",
            provider.id
        ))
    );
    Ok(())
}

fn provider_sync(
    id: &str,
    target: &Target,
    refresh: bool,
    prune: bool,
    dry_run: bool,
) -> Result<()> {
    let agents = if target.all { resolve(target)? } else { vec![resolve_one(target)?] };
    let mut touched = false;

    for entry in agents {
        let mut config = entry.load()?;
        if config.provider(id).is_none() {
            continue;
        }
        touched = true;

        let outcome = sync_into(config.as_mut(), id, refresh, prune)?;
        let verb = if dry_run { "would write" } else { "synced" };
        let mut summary = format!("{verb} {} model(s)", outcome.written);
        if prune {
            let verb = if dry_run { "would drop" } else { "dropped" };
            summary.push_str(&format!(", {verb} {}", outcome.pruned));
        }

        if dry_run {
            println!("{} {summary} in {}", ui::dim("·"), entry.info().name);
            continue;
        }
        config.save()?;
        println!("{} {summary} for {} in {}", ui::green("✓"), ui::bold(id), entry.info().name);
    }

    if !touched {
        bail!("no selected agent has a provider called {id:?}");
    }
    Ok(())
}

/// What one sync changed.
struct SyncOutcome {
    written: usize,
    pruned: usize,
}

/// Probe an endpoint, enrich the result from models.dev and write it back.
///
/// Shared by `provider sync` and the `--sync` flag on `add` and `preset apply`,
/// so the model-discovery rules live in one place.
fn sync_into(
    config: &mut dyn AgentConfig,
    id: &str,
    refresh: bool,
    prune: bool,
) -> Result<SyncOutcome> {
    let provider = config
        .provider(id)
        .with_context(|| format!("{} has no provider {id:?}", config.info().name))?;

    let discovery = net::discover_models(&provider, net::DEFAULT_TIMEOUT, refresh);

    let Some(probe) = &discovery.probe else {
        bail!("provider {id:?} has no base URL to query");
    };
    if !probe.alive() {
        bail!("{} did not answer: {}", probe.url, probe.summary());
    }
    if discovery.models.is_empty() {
        bail!("{} answered but listed no models", probe.url);
    }

    if let Some(err) = &discovery.catalog_error {
        eprintln!("{} models.dev unavailable, limits may be missing: {err}", ui::yellow("!"));
    }
    if !discovery.unknown_to_catalog.is_empty() {
        eprintln!(
            "{} models.dev has no limits for: {}",
            ui::yellow("!"),
            ui::truncate(&discovery.unknown_to_catalog.join(", "), 120)
        );
    }

    if !config.info().capabilities.per_provider_models {
        println!(
            "{} {} stores no model list, so nothing was written. {} serves {} model(s) — \
             pick one with `confai provider models {id} --select <model>`",
            ui::dim("·"),
            config.info().name,
            id,
            discovery.models.len()
        );
        return Ok(SyncOutcome { written: 0, pruned: 0 });
    }

    let mut patch = Provider::new(id);
    patch.models = discovery.models;
    let written = patch.models.len();
    let served: Vec<String> = patch.models.iter().map(|m| m.id.clone()).collect();
    config.upsert_provider(&patch)?;

    let pruned = if prune { config.prune_models(id, &served)? } else { 0 };
    Ok(SyncOutcome { written, pruned })
}

fn mcp_command(command: McpCommand) -> Result<()> {
    match command {
        McpCommand::List { target } => mcp_list(&target),
        McpCommand::Doctor { target, timeout } => mcp_doctor(&target, Duration::from_secs(timeout)),
        McpCommand::Add { name, target, command, args, url, env } => {
            mcp_add(&name, &target, command.as_deref(), &args, url.as_deref(), &env)
        }
        McpCommand::Remove { name, target } => mcp_remove(&name, &target),
        McpCommand::Toggle { name, off, target } => mcp_toggle(&name, !off, &target),
        McpCommand::Preset(McpPresetCommand::List) => mcp_preset_list(),
        McpCommand::Preset(McpPresetCommand::Apply { id, target, name }) => {
            mcp_preset_apply(&id, &target, name.as_deref())
        }
    }
}

/// The agents a MCP command applies to, skipping any that cannot express one.
fn mcp_agents(target: &Target) -> Result<Vec<Box<dyn Agent>>> {
    let agents: Vec<Box<dyn Agent>> =
        resolve(target)?.into_iter().filter(|a| a.info().capabilities.mcp).collect();
    if agents.is_empty() {
        bail!("no selected agent stores MCP servers");
    }
    Ok(agents)
}

fn mcp_list(target: &Target) -> Result<()> {
    let mut table = Table::new(["agent", "server", "kind", "on", "command or url"]);

    for entry in mcp_agents(target)? {
        let config = match entry.load() {
            Ok(config) => config,
            Err(err) => {
                eprintln!("{}: {err:#}", ui::yellow(entry.info().name));
                continue;
            }
        };
        for server in config.mcp_servers() {
            table.row([
                entry.info().id.to_string(),
                server.name.clone(),
                server.transport.kind().to_string(),
                match server.enabled {
                    Some(true) | None => ui::green("yes"),
                    Some(false) => ui::dim("no"),
                },
                ui::truncate(&server.transport.summary(), 54),
            ]);
        }
    }

    if table.is_empty() {
        println!("{}", ui::dim("no MCP servers configured"));
        return Ok(());
    }
    print!("{}", table.render());
    Ok(())
}

fn mcp_doctor(target: &Target, timeout: Duration) -> Result<()> {
    let mut broken = 0;
    let mut checked = 0;

    for entry in mcp_agents(target)? {
        let config = entry.load()?;
        let servers = config.mcp_servers();
        if servers.is_empty() {
            println!("{} {}: no MCP servers", ui::dim("·"), entry.info().name);
            continue;
        }

        println!("{}", ui::bold(entry.info().name));
        for server in servers {
            let health = mcp::check(&server, timeout);
            checked += 1;
            let icon = match &health {
                mcp::Health::Ok(_) => ui::green("✓"),
                mcp::Health::Broken(_) => {
                    broken += 1;
                    ui::red("✗")
                }
                mcp::Health::Skipped(_) => ui::dim("·"),
            };
            println!(
                "  {icon} {:<22} {}",
                server.name,
                match &health {
                    mcp::Health::Ok(m) => ui::dim(m),
                    mcp::Health::Broken(m) => ui::red(m),
                    mcp::Health::Skipped(m) => ui::dim(m),
                }
            );
        }
    }

    if checked == 0 {
        return Ok(());
    }
    if broken == 0 {
        println!("\n{}", ui::green("every MCP server resolves"));
        return Ok(());
    }
    bail!("{broken} MCP server(s) could not be resolved")
}

/// Build a server from command-line fields, overlaying an existing one so a
/// partial edit does not clear what it did not mention.
fn mcp_server_from_fields(
    existing: Option<mcp::Server>,
    name: &str,
    command: Option<&str>,
    args: &[String],
    url: Option<&str>,
    env: &[String],
) -> Result<mcp::Server> {
    let transport = match (command, url) {
        (_, Some(url)) => mcp::Transport::Remote { url: url.to_string() },
        (Some(command), None) => {
            mcp::Transport::Stdio { command: command.to_string(), args: args.to_vec() }
        }
        (None, None) => match existing.as_ref().map(|s| s.transport.clone()) {
            Some(transport) => transport,
            None => bail!("give either --command or --url for a new server"),
        },
    };

    let mut server = mcp::Server {
        name: name.to_string(),
        transport,
        env: existing.map(|s| s.env).unwrap_or_default(),
        enabled: None,
    };
    for raw in env {
        let (key, value) =
            raw.split_once('=').with_context(|| format!("--env expects KEY=VALUE, got {raw:?}"))?;
        server.env.insert(key.trim().to_string(), value.trim().to_string());
    }
    Ok(server)
}

fn mcp_add(
    name: &str,
    target: &Target,
    command: Option<&str>,
    args: &[String],
    url: Option<&str>,
    env: &[String],
) -> Result<()> {
    let agents = if target.all { mcp_agents(target)? } else { vec![resolve_one(target)?] };

    for entry in agents {
        if !entry.info().capabilities.mcp {
            bail!("{} does not store MCP servers", entry.info().name);
        }
        let mut config = entry.load()?;
        let existed = config.mcp_server(name).is_some();
        let server =
            mcp_server_from_fields(config.mcp_server(name), name, command, args, url, env)?;

        config.upsert_mcp(&server)?;
        config.save()?;
        println!(
            "{} {} {} in {}",
            ui::green("✓"),
            if existed { "updated" } else { "added" },
            ui::bold(name),
            entry.info().name
        );
    }
    Ok(())
}

fn mcp_remove(name: &str, target: &Target) -> Result<()> {
    let agents = if target.all { mcp_agents(target)? } else { vec![resolve_one(target)?] };
    let mut removed_anywhere = false;

    for entry in agents {
        let mut config = entry.load()?;
        if !config.remove_mcp(name)? {
            continue;
        }
        config.save()?;
        removed_anywhere = true;
        println!("{} removed {} from {}", ui::green("✓"), ui::bold(name), entry.info().name);
    }

    if !removed_anywhere {
        bail!("no selected agent has an MCP server called {name:?}");
    }
    Ok(())
}

fn mcp_toggle(name: &str, enabled: bool, target: &Target) -> Result<()> {
    let agents = if target.all { mcp_agents(target)? } else { vec![resolve_one(target)?] };
    let mut changed = false;

    for entry in agents {
        let mut config = entry.load()?;
        if config.mcp_server(name).is_none() {
            continue;
        }
        match config.set_mcp_enabled(name, enabled) {
            Ok(()) => {
                config.save()?;
                changed = true;
                println!(
                    "{} {} {} in {}",
                    ui::green("✓"),
                    if enabled { "enabled" } else { "disabled" },
                    ui::bold(name),
                    entry.info().name
                );
            }
            // Codex and Claude Code have no such flag; say so rather than
            // pretending the server was turned off.
            Err(err) => eprintln!("{} {err:#}", ui::yellow("!")),
        }
    }

    if !changed {
        bail!("no selected agent could toggle an MCP server called {name:?}");
    }
    Ok(())
}

fn mcp_preset_list() -> Result<()> {
    let presets = preset::mcp_all()?;
    let mut table = Table::new(["preset", "name", "command or url", "source", "description"]);

    for entry in &presets {
        let server = entry.server(None)?;
        table.row([
            ui::bold(&entry.id),
            entry.name.clone(),
            ui::truncate(&server.transport.summary(), 40),
            ui::dim(entry.origin.as_str()),
            ui::truncate(&entry.description, 44),
        ]);
    }

    print!("{}", table.render());
    if let Some(dir) = preset::mcp_user_dir() {
        println!("\n{}", ui::dim(&format!("drop your own in {}", dir.display())));
    }
    Ok(())
}

fn mcp_preset_apply(id: &str, target: &Target, name: Option<&str>) -> Result<()> {
    let entry = preset::mcp_find(id)?;
    let server = entry.server(name)?;

    for var in entry.missing_env() {
        eprintln!(
            "{} {} expects ${var} to be set; it is not, so the server will start but may not work",
            ui::yellow("!"),
            entry.name
        );
    }

    for agent_entry in mcp_agents(target)? {
        let mut config = agent_entry.load()?;
        config.upsert_mcp(&server)?;
        config.save()?;
        println!(
            "{} added {} to {}",
            ui::green("✓"),
            ui::bold(&server.name),
            agent_entry.info().name
        );
    }
    Ok(())
}

fn preset_command(command: PresetCommand) -> Result<()> {
    match command {
        PresetCommand::List => preset_list(),
        PresetCommand::Show { id } => preset_show(&id),
        PresetCommand::Apply { id, target, api_key, select, sync } => {
            preset_apply(&id, &target, api_key.as_deref(), select, sync)
        }
    }
}

fn preset_list() -> Result<()> {
    let presets = preset::all()?;
    let mut table = Table::new(["preset", "name", "url", "source", "description"]);

    for entry in &presets {
        let provider = entry.provider(None)?;
        table.row([
            ui::bold(&entry.id),
            entry.name.clone(),
            provider.base_url.clone().unwrap_or_default(),
            ui::dim(entry.origin.as_str()),
            ui::truncate(&entry.description, 48),
        ]);
    }

    print!("{}", table.render());
    if let Some(dir) = preset::user_dir() {
        println!("\n{}", ui::dim(&format!("drop your own presets in {}", dir.display())));
    }
    Ok(())
}

fn preset_show(id: &str) -> Result<()> {
    let entry = preset::find(id)?;
    let provider = entry.provider(None)?;

    println!("{} {}", ui::bold(&entry.name), ui::dim(&format!("({})", entry.id)));
    if !entry.description.is_empty() {
        println!("{}", entry.description);
    }
    if let Some(homepage) = &entry.homepage {
        println!("{} {homepage}", ui::dim("homepage"));
    }
    println!();

    let mut table = Table::new(["field", "value"]);
    table.row(["provider id", &provider.id]);
    table.row(["base url", provider.base_url.as_deref().unwrap_or("-")]);
    table
        .row(["wire api", &provider.wire_api.map(|w| w.to_string()).unwrap_or_else(|| "-".into())]);
    table.row([
        "api key",
        &match (&entry.api_key_env, provider.api_key.as_deref()) {
            (Some(var), Some(_)) => format!("from ${var}"),
            (Some(var), None) => ui::yellow(&format!("required, set ${var} or pass --api-key")),
            (None, _) => "not required".to_string(),
        },
    ]);
    if let Some(model) = &entry.default_model {
        table.row(["default model", model]);
    }
    table.row([
        "models",
        &if provider.models.is_empty() {
            "discovered via `provider sync`".to_string()
        } else {
            provider.models.len().to_string()
        },
    ]);
    print!("{}", table.render());
    Ok(())
}

fn preset_apply(
    id: &str,
    target: &Target,
    api_key: Option<&str>,
    select: bool,
    sync: bool,
) -> Result<()> {
    let entry = preset::find(id)?;
    if entry.missing_key(api_key) {
        let var = entry.api_key_env.as_deref().unwrap_or("the provider's key");
        eprintln!(
            "{} {} needs an API key; pass --api-key or set ${var}. Writing the endpoint without one.",
            ui::yellow("!"),
            entry.name
        );
    }
    let provider = entry.provider(api_key)?;

    // A preset is agent-neutral, so applying it to everything installed is the
    // expected default rather than an error.
    let agents = match &target.agent {
        Some(name) => vec![agent::find(name)?],
        None => resolve(target)?,
    };

    for agent_entry in agents {
        let mut config = agent_entry.load()?;
        config.upsert_provider(&provider)?;

        if sync {
            if let Err(err) = sync_into(config.as_mut(), &provider.id, false, false) {
                eprintln!(
                    "{} sync skipped for {}: {err:#}",
                    ui::yellow("!"),
                    agent_entry.info().name
                );
            }
        }
        if select {
            match config.set_active_provider(&provider.id) {
                Ok(()) => {}
                Err(err) => eprintln!(
                    "{} could not select {} in {}: {err:#}",
                    ui::yellow("!"),
                    provider.id,
                    agent_entry.info().name
                ),
            }
        }
        if let Some(model) = &entry.default_model {
            // Attributed to the preset's own endpoint, not whichever one happens
            // to be active: on opencode a bare name resolves against the active
            // provider, which would file this model under the wrong gateway.
            let _ = config.set_model_for(&provider.id, model);
        }
        config.save()?;

        println!(
            "{} applied {} to {} ({})",
            ui::green("✓"),
            ui::bold(&entry.id),
            agent_entry.info().name,
            ui::dim(&agent_entry.info().config_path.display().to_string())
        );
    }
    Ok(())
}

fn model_command(model: Option<String>, target: &Target) -> Result<()> {
    match model {
        None => {
            let mut table = Table::new(["agent", "model"]);
            for entry in resolve(target)? {
                let current = entry
                    .load()
                    .ok()
                    .and_then(|config| config.model())
                    .unwrap_or_else(|| "-".into());
                table.row([entry.info().name.to_string(), current]);
            }
            print!("{}", table.render());
            Ok(())
        }
        Some(model) => {
            let agents = if target.all { resolve(target)? } else { vec![resolve_one(target)?] };
            for entry in agents {
                let mut config = entry.load()?;
                config.set_model(&model)?;
                config.save()?;
                println!("{} {} now uses {}", ui::green("✓"), entry.info().name, ui::bold(&model));
            }
            Ok(())
        }
    }
}

fn paths(target: &Target) -> Result<()> {
    for entry in resolve(target)? {
        println!("{}", entry.info().config_path.display());
    }
    Ok(())
}

fn edit(target: &Target) -> Result<()> {
    let entry = resolve_one(target)?;
    let path = &entry.info().config_path;

    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| if cfg!(windows) { "notepad".into() } else { "vi".into() });

    let status = std::process::Command::new(&editor)
        .arg(path)
        .status()
        .with_context(|| format!("launching {editor:?}"))?;

    if !status.success() {
        bail!("{editor} exited with {status}");
    }

    // Parse what came back so a typo surfaces now rather than the next time the
    // agent starts.
    entry.load().with_context(|| format!("{} is no longer valid", path.display()))?;
    println!("{} {} still parses", ui::green("✓"), path.display());
    Ok(())
}

fn doctor() -> Result<()> {
    let mut problems = 0;

    for entry in agent::all() {
        let info = entry.info();
        let detection = entry.detect();
        if !detection.installed() {
            println!("{} {} not installed", ui::dim("·"), info.name);
            continue;
        }

        let config = match entry.load() {
            Ok(config) => config,
            Err(err) => {
                problems += 1;
                println!("{} {}: {err:#}", ui::red("✗"), info.name);
                continue;
            }
        };

        println!("{} {} parses ({})", ui::green("✓"), info.name, info.config_path.display());

        let providers = config.providers();
        for provider in &providers {
            if provider.base_url.is_none() {
                problems += 1;
                println!("  {} {} has no base URL", ui::yellow("!"), provider.id);
            }
            if info.capabilities.per_provider_models && provider.models.is_empty() {
                println!(
                    "  {} {} lists no models; `confai provider sync {}` will fill them in",
                    ui::yellow("!"),
                    provider.id,
                    provider.id
                );
            }
        }

        if let Some(active) = config.active_provider() {
            if !providers.iter().any(|p| p.id == active) {
                problems += 1;
                println!("  {} selected provider {active:?} is not configured", ui::red("✗"));
            }
        }
    }

    if problems == 0 {
        println!("\n{}", ui::green("no problems found"));
        return Ok(());
    }
    bail!("{problems} problem(s) found")
}

fn about() -> Result<()> {
    for line in brand::logo_lines() {
        println!("{}", ui::accent(line));
    }
    println!("\n{}", brand::TAGLINE);
    println!("{}\n", ui::dim(&brand::signature()));

    let mut table = Table::plain();
    table.row(["website", &ui::cyan(brand::WEBSITE)]);
    table.row(["source", &ui::cyan(brand::REPOSITORY)]);
    table.row(["licence", "MIT"]);

    let home = dirs::home_dir().map(|home| home.join(".confai"));
    table.row([
        "state",
        &home.as_ref().map(|dir| dir.display().to_string()).unwrap_or_else(|| "-".into()),
    ]);
    table.row([
        "presets",
        &preset::user_dir().map(|dir| dir.display().to_string()).unwrap_or_else(|| "-".into()),
    ]);
    table.row([
        "model catalogue",
        &net::catalog::cache_path()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| "-".into()),
    ]);
    print!("\n{}", table.render());

    println!("\n{}", ui::bold("agents"));
    let mut agents = Table::plain();
    for entry in agent::all() {
        let detection = entry.detect();
        agents.row([
            entry.info().name.to_string(),
            if detection.installed() {
                ui::green(detection.describe())
            } else {
                ui::dim(detection.describe())
            },
            ui::dim(&entry.info().config_path.display().to_string()),
        ]);
    }
    print!("{}", agents.render());
    Ok(())
}

fn update() -> Result<()> {
    match crate::update::check_now()? {
        crate::update::Status::UpToDate { current } => {
            println!("{} {} is the latest release", ui::green("✓"), ui::bold(&current.to_string()));
        }
        crate::update::Status::Unreleased { current, latest } => {
            println!(
                "{} this build is {}, ahead of the latest release {}",
                ui::dim("·"),
                ui::bold(&current.to_string()),
                latest
            );
        }
        crate::update::Status::Newer(available) => {
            println!(
                "{} {} → {}",
                ui::accent(brand::MARK),
                ui::dim(&available.current.to_string()),
                ui::bold(&available.latest.to_string())
            );
            for line in available.headline(8) {
                println!("  {} {line}", ui::dim("·"));
            }
            println!("\n{}", ui::dim(&available.url));
            println!("\n{}", ui::bold("upgrade"));
            for command in upgrade_commands() {
                println!("  {command}");
            }
        }
    }
    Ok(())
}

/// How this build is most likely to be replaced, best guess first.
///
/// ConfAI does not replace its own binary: the installers and `cargo` already
/// do it properly, and a tool that rewrites itself while holding a user's
/// credentials open is a worse trade than printing one line.
fn upgrade_commands() -> Vec<String> {
    let mut commands = vec!["cargo install confai --locked".to_string()];
    if cfg!(windows) {
        commands.push("irm https://redstone.md/confai/install.ps1 | iex".to_string());
    } else {
        commands.push("curl -fsSL https://redstone.md/confai/install.sh | sh".to_string());
    }
    commands.push(format!("or download from {}/releases/latest", brand::REPOSITORY));
    commands
}

/// Print the update notice, if the cache knows of a newer release.
///
/// Goes to stderr so it never contaminates output being piped somewhere.
pub fn print_update_notice() {
    let Some(available) = crate::update::notice() else { return };

    eprintln!(
        "\n{} {} {} {} available",
        ui::accent(brand::MARK),
        ui::dim(&available.current.to_string()),
        ui::dim("→"),
        ui::bold(&available.latest.to_string())
    );
    for line in available.headline(3) {
        eprintln!("  {} {line}", ui::dim("·"));
    }
    eprintln!("  {}", ui::dim("run `confai update` for the rest"));
}

fn undo(target: &Target) -> Result<()> {
    let mut restored = 0;

    for entry in resolve(target)? {
        let path = &entry.info().config_path;
        if store::restore_backup(path)? {
            println!("{} restored {}", ui::green("✓"), path.display());
            restored += 1;
        } else {
            println!("{} no backup for {}", ui::dim("·"), entry.info().name);
        }
    }

    if restored == 0 {
        bail!("nothing to undo; ConfAI has not written to any selected config");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(extras: &[&str]) -> ProviderFields {
        ProviderFields {
            base_url: Some("https://byesu.com/v1".into()),
            wire_api: Some("responses".into()),
            extras: extras.iter().map(|s| s.to_string()).collect(),
            ..ProviderFields::default()
        }
    }

    #[test]
    fn fields_become_a_provider() {
        let provider =
            provider_from_fields("byesu", &fields(&["requires_openai_auth=true"])).unwrap();
        assert_eq!(provider.id, "byesu");
        assert_eq!(provider.wire_api, Some(WireApi::Responses));
        assert_eq!(provider.extras.get("requires_openai_auth").map(String::as_str), Some("true"));
    }

    #[test]
    fn set_values_containing_equals_keep_their_tail() {
        let provider = provider_from_fields("x", &fields(&["query=a=b"])).unwrap();
        assert_eq!(provider.extras.get("query").map(String::as_str), Some("a=b"));
    }

    #[test]
    fn a_malformed_set_flag_says_what_was_expected() {
        let err = provider_from_fields("x", &fields(&["nope"])).unwrap_err().to_string();
        assert!(err.contains("KEY=VALUE"), "{err}");
    }

    #[test]
    fn an_unknown_wire_api_lists_the_valid_ones() {
        let bad = ProviderFields { wire_api: Some("smoke-signals".into()), ..fields(&[]) };
        let err = provider_from_fields("x", &bad).unwrap_err().to_string();
        assert!(err.contains("chat") && err.contains("anthropic"), "{err}");
    }

    #[test]
    fn an_unknown_agent_name_is_rejected_with_the_known_ones() {
        let target = Target { agent: Some("emacs".into()), all: false };
        let Err(err) = resolve(&target) else {
            panic!("an unknown agent name must not resolve");
        };
        assert!(err.to_string().contains("unknown agent"), "{err}");
    }
}
