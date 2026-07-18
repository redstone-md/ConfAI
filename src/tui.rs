//! Interactive view: browse every detected agent and edit its providers in place.
//!
//! State is a snapshot of what was on disk. Every mutation reloads the agent's
//! config, edits it, saves, and re-snapshots, so the TUI never holds a stale
//! parsed document across a write and unknown keys keep surviving round trips.
//!
//! Everything the user can do is an [`Action`]. Keys, the hint bar, the help
//! screen and the command palette all go through that one list, so a binding
//! cannot exist without being searchable and documented.
//!
//! Nothing here names a colour, a product or a URL; all of that comes from
//! [`crate::brand`], so the CLI and the TUI cannot drift apart.

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;

use anyhow::{Context, Result};
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::layout::{Alignment, Constraint, Layout, Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Clear, List, ListItem, ListState, Paragraph, Scrollbar,
    ScrollbarOrientation, ScrollbarState, Wrap,
};
use ratatui::{DefaultTerminal, Frame};

use crate::agent::{self, Agent, AgentConfig, Capabilities, Detection};
use crate::brand::{self, palette};
use crate::domain::{mask, Model, Provider, WireApi};
use crate::net::{self, catalog};
use crate::preset::{self, Preset};
use crate::ui;

pub fn run() -> Result<()> {
    let mut terminal = ratatui::try_init()?;
    let mouse = MouseCapture::enable();
    let outcome = App::new().run(&mut terminal);
    drop(mouse);

    // Restore first, whatever happened, so an error message is never printed
    // into the alternate screen and then wiped.
    let restored = ratatui::try_restore();
    match (outcome, restored) {
        (Err(err), _) => Err(err),
        (Ok(()), Err(err)) => Err(err.into()),
        (Ok(()), Ok(())) => Ok(()),
    }
}

/// Mouse reporting, turned back off however the run ends.
///
/// A terminal left reporting clicks swallows every selection in the shell that
/// spawned us, so the panic hook has to undo it as well as the drop.
struct MouseCapture;

impl MouseCapture {
    fn enable() -> Self {
        let _ = execute!(io::stdout(), EnableMouseCapture);
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = execute!(io::stdout(), DisableMouseCapture);
            previous(info);
        }));
        Self
    }
}

impl Drop for MouseCapture {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), DisableMouseCapture);
    }
}

/// Marks the selected row; drawn in the leftmost cell of every pane.
const CURSOR_BAR: &str = "▌";
/// A fact that has been established this session.
const DOT_FILLED: &str = "●";
/// A fact that is simply unknown, as opposed to bad.
const DOT_HOLLOW: &str = "○";
/// Half-evidence: one of the two things looked for was found.
const DOT_HALF: &str = "◐";
/// The nothing-recorded placeholder, distinct from a literal empty string.
const ABSENT: &str = "—";

/// Below this many rows the header collapses to one line.
const COMPACT_HEIGHT: u16 = 30;
/// Below this many columns the tagline is dropped from the header.
const TAGLINE_WIDTH: u16 = 100;
/// The agent pane never grows; the provider pane is the one with columns to fill.
const AGENT_PANE_WIDTH: u16 = 30;

/// Keys that only move the cursor, so they dispatch no action and appear in the
/// help screen by hand.
const NAVIGATION: &[(&str, &str)] = &[
    ("↑ ↓ / k j", "move within the focused pane"),
    ("tab / ← →", "switch between the agent and provider panes"),
    ("esc", "close an overlay, clear the filter, or quit"),
];

/// Cyrillic letters and the Latin character sharing the same physical key on a
/// ЙЦУКЕН layout.
const CYRILLIC_TO_LATIN: &[(char, char)] = &[
    ('й', 'q'),
    ('ц', 'w'),
    ('у', 'e'),
    ('к', 'r'),
    ('е', 't'),
    ('н', 'y'),
    ('г', 'u'),
    ('ш', 'i'),
    ('щ', 'o'),
    ('з', 'p'),
    ('х', '['),
    ('ъ', ']'),
    ('ф', 'a'),
    ('ы', 's'),
    ('в', 'd'),
    ('а', 'f'),
    ('п', 'g'),
    ('р', 'h'),
    ('о', 'j'),
    ('л', 'k'),
    ('д', 'l'),
    ('ж', ';'),
    ('э', '\''),
    ('я', 'z'),
    ('ч', 'x'),
    ('с', 'c'),
    ('м', 'v'),
    ('и', 'b'),
    ('т', 'n'),
    ('ь', 'm'),
    ('б', ','),
    ('ю', '.'),
];

/// The character the same physical key would send on a US layout.
///
/// Bindings are positions on a keyboard, not letters of the alphabet: with the
/// OS switched to Russian, `q` arrives as `й` and every shortcut would die.
fn normalise_key(ch: char) -> char {
    let lower = ch.to_lowercase().next().unwrap_or(ch);
    let Some((_, latin)) = CYRILLIC_TO_LATIN.iter().find(|(cyrillic, _)| *cyrillic == lower) else {
        return ch;
    };
    if ch == lower {
        *latin
    } else {
        latin.to_uppercase().next().unwrap_or(*latin)
    }
}

/// The same keystroke with its character folded to that US-layout position.
fn normalise_event(key: KeyEvent) -> KeyEvent {
    match key.code {
        KeyCode::Char(ch) => KeyEvent { code: KeyCode::Char(normalise_key(ch)), ..key },
        _ => key,
    }
}

/// A bounded cursor into a list whose length changes under it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Cursor {
    index: usize,
}

impl Cursor {
    fn index(self) -> usize {
        self.index
    }

    /// Keep the cursor inside a list of `len` items, parking on the last one
    /// when the list shrank.
    fn clamp(&mut self, len: usize) {
        self.index = self.index.min(len.saturating_sub(1));
    }

    fn step(&mut self, delta: isize, len: usize) {
        if len == 0 {
            self.index = 0;
            return;
        }
        let last = len - 1;
        self.index = match delta {
            d if d < 0 => self.index.saturating_sub(d.unsigned_abs()),
            d => (self.index + d as usize).min(last),
        };
        self.index = self.index.min(last);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pane {
    Agents,
    Providers,
}

impl Pane {
    fn other(self) -> Self {
        match self {
            Pane::Agents => Pane::Providers,
            Pane::Providers => Pane::Agents,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tone {
    Info,
    Good,
    Bad,
}

impl Tone {
    fn style(self) -> Style {
        Style::default().fg(match self {
            Tone::Info => palette::MUTED,
            Tone::Good => palette::GOOD,
            Tone::Bad => palette::BAD,
        })
    }

    fn glyph(self) -> &'static str {
        match self {
            Tone::Info => "·",
            Tone::Good => "✓",
            Tone::Bad => "✗",
        }
    }
}

#[derive(Debug, Clone)]
struct Status {
    text: String,
    tone: Tone,
}

impl Status {
    fn new(tone: Tone, text: impl Into<String>) -> Self {
        Self { text: text.into(), tone }
    }
}

impl Default for Status {
    fn default() -> Self {
        Self::new(Tone::Info, format!("{} · press ? for help", brand::TAGLINE))
    }
}

/// What a probe found, kept so the health column survives the status line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Health {
    alive: bool,
    millis: u128,
}

/// Probe results for this session, keyed by the agent the provider belongs to.
///
/// The same provider id means different things under different agents, so the
/// agent id is part of the key rather than an afterthought.
#[derive(Debug, Default)]
struct HealthCache {
    entries: HashMap<(String, String), Health>,
}

impl HealthCache {
    fn get(&self, agent_id: &str, provider_id: &str) -> Option<Health> {
        self.entries.get(&(agent_id.to_string(), provider_id.to_string())).copied()
    }

    fn record(&mut self, agent_id: &str, provider_id: &str, health: Health) {
        self.entries.insert((agent_id.to_string(), provider_id.to_string()), health);
    }

    /// Drop what we knew, because the endpoint it described has changed.
    fn forget(&mut self, agent_id: &str, provider_id: &str) {
        self.entries.remove(&(agent_id.to_string(), provider_id.to_string()));
    }
}

/// A case-insensitive substring filter over the provider list.
#[derive(Debug, Default, Clone)]
struct Filter {
    query: String,
    /// Whether keystrokes are extending the query rather than moving the cursor.
    editing: bool,
}

impl Filter {
    fn active(&self) -> bool {
        !self.query.trim().is_empty()
    }

    fn clear(&mut self) {
        self.query.clear();
        self.editing = false;
    }

    /// Matches on anything the user can see or is likely to remember: the id
    /// they typed, the host they pasted, or a model they know it serves.
    fn matches(&self, provider: &Provider) -> bool {
        let needle = self.query.trim().to_lowercase();
        if needle.is_empty() {
            return true;
        }
        let hit = |hay: &str| hay.to_lowercase().contains(&needle);
        hit(&provider.id)
            || provider.host().is_some_and(hit)
            || provider.display_name.as_deref().is_some_and(hit)
            || provider.models.iter().any(|model| hit(&model.id))
    }
}

/// One agent as it looked the last time it was read from disk.
struct AgentEntry {
    id: String,
    name: String,
    detection: Detection,
    capabilities: Capabilities,
    config_path: PathBuf,
    providers: Vec<Provider>,
    active: Option<String>,
    /// The model the agent is set to use, for agents that name one.
    model: Option<String>,
    /// Why the config could not be parsed, if it could not be.
    error: Option<String>,
}

impl AgentEntry {
    fn snapshot(handle: &dyn Agent) -> Self {
        let info = handle.info();
        let mut entry = Self {
            id: info.id.to_string(),
            name: info.name.to_string(),
            detection: handle.detect(),
            capabilities: info.capabilities,
            config_path: info.config_path.clone(),
            providers: Vec::new(),
            active: None,
            model: None,
            error: None,
        };
        match handle.load() {
            Ok(config) => {
                entry.providers = config.providers();
                entry.active = config.active_provider();
                entry.model = config.model();
            }
            // A missing config for an agent that is not installed is expected,
            // not something to shout about in the list.
            Err(err) if entry.detection.installed() => entry.error = Some(format!("{err:#}")),
            Err(_) => {}
        }
        entry
    }

    fn is_active(&self, provider_id: &str) -> bool {
        self.active.as_deref() == Some(provider_id)
    }

    /// Glyph and colour for how much evidence there is that this agent exists.
    fn detection_mark(&self) -> (&'static str, Style) {
        match (self.detection.binary_on_path, self.detection.config_exists) {
            (true, true) => (DOT_FILLED, Style::default().fg(palette::GOOD)),
            (false, false) => (DOT_HOLLOW, Style::default().fg(palette::FAINT)),
            _ => (DOT_HALF, Style::default().fg(palette::WARN)),
        }
    }
}

/// Everything the user can ask for, in one place.
///
/// Keys dispatch through this and so does the command palette, so the two can
/// never drift into being separate lists of what the program can do.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Action {
    /// Route the agent through a provider. `None` means the one under the cursor.
    Use(Option<String>),
    /// Move the agent cursor onto a named agent.
    SelectAgent(String),
    /// Choose which of the endpoint's models the agent should use.
    Models,
    Detail,
    Filter,
    Add,
    Edit,
    Delete,
    Check,
    CheckAll,
    Sync {
        prune: bool,
    },
    /// Apply a named preset, or `None` to open the picker.
    Preset(Option<String>),
    Reload,
    Palette,
    Help,
    Quit,
}

/// Every context-free action, in the order the help screen lists them.
fn menu() -> Vec<Action> {
    vec![
        Action::Detail,
        Action::Filter,
        Action::Use(None),
        Action::Models,
        Action::Add,
        Action::Edit,
        Action::Delete,
        Action::Check,
        Action::CheckAll,
        Action::Sync { prune: false },
        Action::Sync { prune: true },
        Action::Preset(None),
        Action::Reload,
        Action::Palette,
        Action::Help,
        Action::Quit,
    ]
}

impl Action {
    fn label(&self) -> String {
        match self {
            Action::Use(Some(id)) => format!("use {id}"),
            Action::Use(None) => "use provider".into(),
            Action::SelectAgent(id) => format!("agent {id}"),
            Action::Models => "set model".into(),
            Action::Detail => "provider detail".into(),
            Action::Filter => "filter providers".into(),
            Action::Add => "add provider".into(),
            Action::Edit => "edit provider".into(),
            Action::Delete => "delete provider".into(),
            Action::Check => "check provider".into(),
            Action::CheckAll => "check all providers".into(),
            Action::Sync { prune: false } => "sync models".into(),
            Action::Sync { prune: true } => "sync models and prune".into(),
            Action::Preset(Some(id)) => format!("preset {id}"),
            Action::Preset(None) => "apply a preset".into(),
            Action::Reload => "reload from disk".into(),
            Action::Palette => "command palette".into(),
            Action::Help => "help".into(),
            Action::Quit => "quit".into(),
        }
    }

    /// The short form the hint bar has room for.
    fn hint(&self) -> &'static str {
        match self {
            Action::Use(_) => "use",
            Action::SelectAgent(_) => "agent",
            Action::Models => "model",
            Action::Detail => "detail",
            Action::Filter => "filter",
            Action::Add => "add",
            Action::Edit => "edit",
            Action::Delete => "del",
            Action::Check => "check",
            Action::CheckAll => "check all",
            Action::Sync { prune: false } => "sync",
            Action::Sync { prune: true } => "sync+prune",
            Action::Preset(_) => "preset",
            Action::Reload => "reload",
            Action::Palette => "commands",
            Action::Help => "help",
            Action::Quit => "quit",
        }
    }

    fn description(&self) -> String {
        match self {
            Action::Use(_) => "make this the provider the agent routes through".into(),
            Action::SelectAgent(_) => "edit this agent's config".into(),
            Action::Models => "choose which model this agent uses".into(),
            Action::Detail => "show everything recorded about this provider".into(),
            Action::Filter => "narrow the provider list by id, host or model".into(),
            Action::Add => "add a provider to this agent".into(),
            Action::Edit => "change this provider's url, key or wire api".into(),
            Action::Delete => "remove this provider from the config".into(),
            Action::Check => "ask the selected provider whether it is up".into(),
            Action::CheckAll => "check every provider of this agent in turn".into(),
            Action::Sync { prune: false } => "read the model list from the endpoint".into(),
            Action::Sync { prune: true } => {
                "read the model list and drop what is no longer served".into()
            }
            Action::Preset(Some(_)) => "apply this preset to the selected agent".into(),
            Action::Preset(None) => "choose a preset to apply".into(),
            Action::Reload => "re-read every config from disk".into(),
            Action::Palette => "search every action by name".into(),
            Action::Help => "the about screen and the full key map".into(),
            Action::Quit => format!("leave {}", brand::NAME),
        }
    }

    /// The direct binding, where the action has one.
    fn binding(&self) -> Option<&'static str> {
        Some(match self {
            Action::Use(_) => "u",
            Action::SelectAgent(_) => return None,
            Action::Models => "m",
            Action::Detail => "enter",
            // `/` is not on the same key on every layout, so a chord backs it up.
            Action::Filter => "/ or ctrl+f",
            Action::Add => "a",
            Action::Edit => "e",
            Action::Delete => "d",
            Action::Check => "c",
            Action::CheckAll => "C",
            Action::Sync { prune: false } => "s",
            Action::Sync { prune: true } => "S",
            Action::Preset(_) => "p",
            Action::Reload => "r",
            Action::Palette => "ctrl+p or ctrl+k",
            Action::Help => "?",
            Action::Quit => "q",
        })
    }

    /// Why this cannot run right now, if it cannot.
    ///
    /// The palette shows blocked actions rather than hiding them, so it stays a
    /// stable map of what the program can do.
    fn unavailable(&self, app: &App) -> Option<String> {
        let no_agent = || app.agent().is_none().then(|| "no agent selected".to_string());
        let no_provider = || app.provider().is_none().then(|| "no provider selected".to_string());

        match self {
            Action::Quit | Action::Help | Action::Reload | Action::Palette | Action::Filter => None,
            Action::SelectAgent(_) => None,
            Action::Add | Action::Preset(_) | Action::Use(Some(_)) => no_agent(),
            // Enter on the agent pane only moves the focus, so it is always fine.
            Action::Detail if app.focus == Pane::Agents => None,
            // Deliberately not gated on `per_provider_models`: that flag means
            // the config stores a model *list*, not that the agent has a model.
            // For the agents it is false for, this picker is the only way to
            // choose one at all.
            Action::Models => no_provider(),
            Action::Detail | Action::Edit | Action::Delete | Action::Check | Action::Use(None) => {
                no_provider()
            }
            Action::CheckAll => app
                .agent()
                .is_none_or(|agent| agent.providers.is_empty())
                .then(|| "this agent has no providers to check".to_string()),
            Action::Sync { .. } => no_provider().or_else(|| {
                app.agent().filter(|agent| !agent.capabilities.per_provider_models).map(|agent| {
                    format!("{} stores no model list; press m to choose a model", agent.name)
                })
            }),
        }
    }

    fn run(self, app: &mut App) {
        match self {
            Action::Use(id) => app.use_provider(id),
            Action::SelectAgent(id) => {
                app.select_agent(&id);
                app.provider_cursor = Cursor::default();
                app.focus = Pane::Providers;
            }
            Action::Models => app.schedule(Job::Models, "asking the endpoint what it serves…"),
            Action::Detail => app.open_detail(),
            Action::Filter => {
                app.filter.editing = true;
                app.focus = Pane::Providers;
            }
            Action::Add => app.open_add_form(),
            Action::Edit => app.open_edit_form(),
            Action::Delete => app.ask_delete(),
            Action::Check => app.schedule(Job::Check, "checking…"),
            Action::CheckAll => app.schedule(Job::CheckAll, "checking every provider…"),
            Action::Sync { prune: false } => {
                app.schedule(Job::Sync { prune: false }, "syncing models…")
            }
            Action::Sync { prune: true } => {
                app.schedule(Job::Sync { prune: true }, "syncing models, dropping stale…")
            }
            Action::Preset(Some(id)) => app.apply_preset_by_id(&id),
            Action::Preset(None) => app.open_presets(),
            Action::Reload => {
                app.reload();
                app.say(Tone::Info, "reloaded from disk");
            }
            Action::Palette => app.open_palette(),
            Action::Help => app.overlay = Some(Overlay::About { scroll: 0 }),
            Action::Quit => app.quit = true,
        }
    }

    /// Everything reachable from where the app currently stands, for the palette.
    fn catalogue(app: &App) -> Vec<Action> {
        let mut actions: Vec<Action> = app
            .agent()
            .map(|agent| agent.providers.iter().map(|p| Action::Use(Some(p.id.clone()))).collect())
            .unwrap_or_default();

        // The bare forms of these only re-open what the palette already lists.
        actions.extend(menu().into_iter().filter(|action| {
            !matches!(action, Action::Use(None) | Action::Preset(None) | Action::Palette)
        }));

        actions.extend(
            preset::all()
                .unwrap_or_default()
                .into_iter()
                .map(|entry| Action::Preset(Some(entry.id))),
        );
        actions.extend(app.agents.iter().map(|agent| Action::SelectAgent(agent.id.clone())));
        actions
    }
}

/// One hint-bar entry: a key, what it does, and what clicking it runs.
struct Hint {
    key: String,
    label: String,
    action: Option<Action>,
}

impl Hint {
    fn plain(key: &str, label: &str) -> Self {
        Self { key: key.to_string(), label: label.to_string(), action: None }
    }

    fn of(action: Action) -> Self {
        Self {
            key: action.binding().unwrap_or("").to_string(),
            label: action.hint().to_string(),
            action: Some(action),
        }
    }

    fn bar(actions: impl IntoIterator<Item = Action>) -> Vec<Self> {
        actions.into_iter().map(Hint::of).collect()
    }
}

/// What a form is editing, and therefore whether the id is up for grabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldKind {
    Id,
    BaseUrl,
    ApiKey,
    DisplayName,
    WireApi,
}

impl FieldKind {
    fn label(self) -> &'static str {
        match self {
            FieldKind::Id => "id",
            FieldKind::BaseUrl => "base url",
            FieldKind::ApiKey => "api key",
            FieldKind::DisplayName => "display name",
            FieldKind::WireApi => "wire api",
        }
    }

    /// Fields chosen from a fixed set cycle rather than accepting free text, so
    /// they cannot be spelled wrong.
    fn is_choice(self) -> bool {
        matches!(self, FieldKind::WireApi)
    }
}

/// The wire API choices in cycle order; the empty string means "unset".
const WIRE_CHOICES: [&str; 4] = ["", "chat", "responses", "anthropic"];

fn next_wire(current: &str) -> &'static str {
    let at = WIRE_CHOICES.iter().position(|c| *c == current.trim()).unwrap_or(0);
    WIRE_CHOICES[(at + 1) % WIRE_CHOICES.len()]
}

/// A small field editor over one provider.
#[derive(Debug, Clone)]
struct Form {
    agent_id: String,
    /// The id being edited, or `None` when adding, in which case the id is a field.
    original_id: Option<String>,
    fields: Vec<(FieldKind, String)>,
    cursor: Cursor,
    /// Whether keystrokes go into `buffer` rather than moving between fields.
    editing: bool,
    buffer: String,
}

impl Form {
    fn add(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            original_id: None,
            fields: vec![
                (FieldKind::Id, String::new()),
                (FieldKind::BaseUrl, String::new()),
                (FieldKind::ApiKey, String::new()),
                (FieldKind::DisplayName, String::new()),
                (FieldKind::WireApi, String::new()),
            ],
            cursor: Cursor::default(),
            editing: false,
            buffer: String::new(),
        }
    }

    fn edit(agent_id: impl Into<String>, provider: &Provider) -> Self {
        Self {
            agent_id: agent_id.into(),
            original_id: Some(provider.id.clone()),
            fields: vec![
                (FieldKind::BaseUrl, provider.base_url.clone().unwrap_or_default()),
                (FieldKind::ApiKey, provider.api_key.clone().unwrap_or_default()),
                (FieldKind::DisplayName, provider.display_name.clone().unwrap_or_default()),
                (
                    FieldKind::WireApi,
                    provider.wire_api.map(|w| w.as_str().to_string()).unwrap_or_default(),
                ),
            ],
            cursor: Cursor::default(),
            editing: false,
            buffer: String::new(),
        }
    }

    fn title(&self) -> String {
        match &self.original_id {
            Some(id) => format!("edit {id}"),
            None => "add provider".to_string(),
        }
    }

    fn value(&self, kind: FieldKind) -> &str {
        self.fields.iter().find(|(k, _)| *k == kind).map(|(_, v)| v.as_str()).unwrap_or_default()
    }

    /// Enter on a field: cycle a choice, otherwise start text entry.
    fn activate(&mut self) {
        let index = self.cursor.index();
        let (kind, value) = &mut self.fields[index];
        if kind.is_choice() {
            *value = next_wire(value).to_string();
            return;
        }
        self.buffer = value.clone();
        self.editing = true;
    }

    fn commit(&mut self) {
        if self.editing {
            let index = self.cursor.index();
            self.fields[index].1 = std::mem::take(&mut self.buffer);
            self.editing = false;
        }
    }

    fn cancel_entry(&mut self) {
        self.editing = false;
        self.buffer.clear();
    }

    fn push(&mut self, ch: char) {
        if self.editing {
            self.buffer.push(ch);
        }
    }

    fn backspace(&mut self) {
        if self.editing {
            self.buffer.pop();
        }
    }

    /// The provider this form describes, or why it is not usable yet.
    fn provider(&self) -> Result<Provider> {
        let id = match &self.original_id {
            Some(id) => id.clone(),
            None => self.value(FieldKind::Id).trim().to_string(),
        };
        agent::validate_provider_id(&id)?;

        let mut provider = Provider::new(id);
        provider.base_url = optional(self.value(FieldKind::BaseUrl));
        provider.api_key = optional(self.value(FieldKind::ApiKey));
        provider.display_name = optional(self.value(FieldKind::DisplayName));
        provider.wire_api = match optional(self.value(FieldKind::WireApi)) {
            Some(raw) => {
                Some(WireApi::parse(&raw).with_context(|| format!("unknown wire api {raw:?}"))?)
            }
            None => None,
        };
        Ok(provider)
    }
}

/// Trim and treat blank as absent, matching how the CLI reads optional flags.
fn optional(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// A pending y/n question about a destructive edit.
#[derive(Debug, Clone)]
struct Confirm {
    prompt: String,
    agent_id: String,
    provider_id: String,
}

struct PresetPicker {
    presets: Vec<Preset>,
    cursor: Cursor,
}

/// The provider detail, with the model facts it was opened with.
///
/// The catalogue is read once when the overlay opens; drawing must never wait
/// on a file, let alone the network.
struct Detail {
    scroll: u16,
    catalog: Option<catalog::Catalog>,
}

/// The command palette: a query and the actions it still matches.
struct CommandPalette {
    query: String,
    entries: Vec<Action>,
    /// Indices into `entries`, best match first.
    matches: Vec<usize>,
    cursor: Cursor,
}

impl CommandPalette {
    fn new(app: &App) -> Self {
        let entries = Action::catalogue(app);
        let mut palette =
            Self { query: String::new(), entries, matches: Vec::new(), cursor: Cursor::default() };
        palette.rebuild();
        palette
    }

    fn rebuild(&mut self) {
        self.matches = rank_by(&self.entries, &self.query, Action::label);
        self.cursor.clamp(self.matches.len());
    }

    fn selected(&self) -> Option<&Action> {
        self.matches.get(self.cursor.index()).and_then(|index| self.entries.get(*index))
    }
}

/// The models an endpoint answered with, filtered as you type.
///
/// Real gateways serve thirty or more, so this is a search box rather than a
/// plain list.
struct ModelPicker {
    provider_id: String,
    models: Vec<Model>,
    catalog: Option<catalog::Catalog>,
    /// What the agent uses today, so the picker can mark it.
    current: Option<String>,
    query: String,
    matches: Vec<usize>,
    cursor: Cursor,
}

impl ModelPicker {
    fn new(provider_id: String, models: Vec<Model>, current: Option<String>) -> Self {
        let mut picker = Self {
            provider_id,
            models,
            // Prices are enrichment, never a reason to stall the interface.
            catalog: catalog::Catalog::cached_only(),
            current,
            query: String::new(),
            matches: Vec::new(),
            cursor: Cursor::default(),
        };
        picker.rebuild();
        // Open on the model already in use, so Enter is a no-op rather than a
        // surprise when the user only meant to look.
        if let Some(at) =
            picker.matches.iter().position(|index| picker.is_current(&picker.models[*index]))
        {
            picker.cursor = Cursor { index: at };
        }
        picker
    }

    fn rebuild(&mut self) {
        self.matches = rank_by(&self.models, &self.query, |model| model.id.clone());
        self.cursor.clamp(self.matches.len());
    }

    fn selected(&self) -> Option<&Model> {
        self.matches.get(self.cursor.index()).and_then(|index| self.models.get(*index))
    }

    fn is_current(&self, model: &Model) -> bool {
        is_current_model(self.current.as_deref(), &model.id)
    }
}

/// Whether `current` names `model_id`, in either the bare form most agents write
/// or the `provider/model` form opencode writes.
fn is_current_model(current: Option<&str>, model_id: &str) -> bool {
    current.is_some_and(|current| current == model_id || current.ends_with(&format!("/{model_id}")))
}

/// How well `needle` matches `haystack` as a subsequence; lower is better.
///
/// A gap between matched characters costs more than a late start, so a run like
/// `csl` in `use vendor` beats the same letters scattered across a longer
/// label. `None` means the characters are not all there in order.
fn subsequence_score(haystack: &str, needle: &str) -> Option<u32> {
    let wanted: Vec<char> =
        needle.to_lowercase().chars().filter(|ch| !ch.is_whitespace()).collect();
    if wanted.is_empty() {
        return Some(0);
    }

    let hay: Vec<char> = haystack.to_lowercase().chars().collect();
    let mut score = 0u32;
    let mut from = 0usize;
    let mut previous: Option<usize> = None;

    for want in wanted {
        let at = hay[from..].iter().position(|ch| *ch == want)? + from;
        score += match previous {
            Some(prev) => (at - prev - 1) as u32 * 2,
            None => at as u32,
        };
        previous = Some(at);
        from = at + 1;
    }
    Some(score)
}

/// Indices of the items whose label `query` matches, best first. Ties keep
/// source order so the list does not reshuffle while you type.
fn rank_by<T>(items: &[T], query: &str, label: impl Fn(&T) -> String) -> Vec<usize> {
    let mut scored: Vec<(u32, usize)> = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            subsequence_score(&label(item), query).map(|score| (score, index))
        })
        .collect();
    scored.sort_by_key(|(score, index)| (*score, *index));
    scored.into_iter().map(|(_, index)| index).collect()
}

enum Overlay {
    About { scroll: u16 },
    Detail(Detail),
    Form(Form),
    Confirm(Confirm),
    Presets(PresetPicker),
    Palette(CommandPalette),
    Models(ModelPicker),
}

/// Work that must be visibly announced before it blocks the event loop.
#[derive(Debug, Clone, Copy)]
enum Job {
    Check,
    CheckAll,
    /// Ask the endpoint what it serves, then offer the answer as a picker.
    Models,
    /// `prune` also drops models the endpoint no longer serves.
    Sync {
        prune: bool,
    },
}

/// Where a list was last drawn and how far it had scrolled, so a click can be
/// resolved back to the row under the pointer.
#[derive(Debug, Default, Clone, Copy)]
struct RowsAt {
    area: Rect,
    offset: usize,
}

impl RowsAt {
    fn row(&self, at: Position, len: usize) -> Option<usize> {
        if !self.area.contains(at) {
            return None;
        }
        let index = self.offset + (at.y - self.area.y) as usize;
        (index < len).then_some(index)
    }
}

/// Every clickable region of the last frame.
#[derive(Debug, Default)]
struct Hits {
    agents: Rect,
    agent_rows: RowsAt,
    providers: Rect,
    provider_rows: RowsAt,
    overlay: Option<Rect>,
    overlay_rows: RowsAt,
    hints: Vec<(Rect, Action)>,
}

struct App {
    agents: Vec<AgentEntry>,
    agent_cursor: Cursor,
    provider_cursor: Cursor,
    focus: Pane,
    filter: Filter,
    health: HealthCache,
    overlay: Option<Overlay>,
    status: Status,
    pending: Option<Job>,
    hits: Hits,
    quit: bool,
}

impl App {
    fn new() -> Self {
        let mut app = Self {
            agents: Vec::new(),
            agent_cursor: Cursor::default(),
            provider_cursor: Cursor::default(),
            focus: Pane::Agents,
            filter: Filter::default(),
            health: HealthCache::default(),
            overlay: None,
            status: Status::default(),
            pending: None,
            hits: Hits::default(),
            quit: false,
        };
        app.reload();
        if let Some(available) = crate::update::notice() {
            let headline = available.headline(1).pop().unwrap_or_default();
            let detail =
                if headline.is_empty() { String::new() } else { format!(" — {headline}") };
            app.say(
                Tone::Good,
                format!(
                    "{} v{} available{detail} · run `confai update`",
                    brand::MARK,
                    available.latest
                ),
            );
        }
        app
    }

    fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.quit {
            self.draw(terminal);

            // The job runs after the frame that announced it, so the terminal
            // shows "checking…" rather than freezing on the previous view.
            if let Some(job) = self.pending.take() {
                self.run_job(terminal, job);
                continue;
            }

            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => self.on_key(key),
                Event::Mouse(mouse) => self.on_mouse(mouse),
                _ => {}
            }
        }
        Ok(())
    }

    /// Render a frame and remember where everything landed.
    ///
    /// A failed draw means the terminal is gone, not that the run is wrong; the
    /// loop keeps going and the next `event::read` reports the real problem.
    fn draw(&mut self, terminal: &mut DefaultTerminal) {
        let mut hits = Hits::default();
        let _ = terminal.draw(|frame| hits = self.render(frame));
        self.hits = hits;
    }

    fn reload(&mut self) {
        self.agents =
            agent::all().iter().map(|handle| AgentEntry::snapshot(handle.as_ref())).collect();
        self.agent_cursor.clamp(self.agents.len());
        self.provider_cursor.clamp(self.provider_count());
    }

    fn agent(&self) -> Option<&AgentEntry> {
        self.agents.get(self.agent_cursor.index())
    }

    /// The providers the filter lets through, in config order.
    fn visible_providers(&self) -> Vec<&Provider> {
        match self.agent() {
            Some(agent) => agent.providers.iter().filter(|p| self.filter.matches(p)).collect(),
            None => Vec::new(),
        }
    }

    fn provider_count(&self) -> usize {
        self.visible_providers().len()
    }

    fn provider(&self) -> Option<&Provider> {
        self.visible_providers().get(self.provider_cursor.index()).copied()
    }

    fn say(&mut self, tone: Tone, text: impl Into<String>) {
        self.status = Status::new(tone, text);
    }

    /// Run an action, or say why it cannot run. Keys, clicks and the palette all
    /// arrive here.
    fn dispatch(&mut self, action: Action) {
        if let Some(reason) = action.unavailable(self) {
            self.say(Tone::Bad, reason);
            return;
        }
        action.run(self);
    }

    /// Reload one agent's snapshot after its config changed on disk.
    fn refresh_selected(&mut self) {
        let index = self.agent_cursor.index();
        let Some(id) = self.agents.get(index).map(|entry| entry.id.clone()) else { return };
        if let Ok(handle) = agent::find(&id) {
            self.agents[index] = AgentEntry::snapshot(handle.as_ref());
        }
        self.provider_cursor.clamp(self.provider_count());
    }

    /// Load the selected agent's config, apply `edit`, save, and re-snapshot.
    ///
    /// Every mutating action goes through here so the load/save/refresh/report
    /// sequence exists once.
    fn apply<F>(&mut self, what: &str, edit: F)
    where
        F: FnOnce(&mut dyn AgentConfig) -> Result<String>,
    {
        let Some(agent_id) = self.agent().map(|a| a.id.clone()) else {
            self.say(Tone::Bad, "no agent selected");
            return;
        };

        let outcome = agent::find(&agent_id).and_then(|handle| {
            let mut config = handle.load()?;
            let message = edit(config.as_mut())?;
            config.save()?;
            Ok(message)
        });

        match outcome {
            Ok(message) => {
                self.refresh_selected();
                self.say(Tone::Good, message);
            }
            Err(err) => self.say(Tone::Bad, format!("{what} failed: {err:#}")),
        }
    }

    fn on_key(&mut self, raw: KeyEvent) {
        // Command matching uses the key's US-layout position so bindings survive
        // a Cyrillic layout; text entry keeps the character actually typed.
        let key = normalise_event(raw);
        if self.overlay.is_some() {
            self.on_overlay_key(key, raw);
        } else if self.filter.editing {
            self.on_filter_key(key, raw);
        } else {
            self.on_browse_key(key);
        }
    }

    fn on_filter_key(&mut self, key: KeyEvent, raw: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                self.filter.editing = false;
                self.focus = Pane::Providers;
            }
            KeyCode::Esc => {
                self.filter.clear();
                self.say(Tone::Info, "filter cleared");
            }
            KeyCode::Backspace => {
                self.filter.query.pop();
            }
            KeyCode::Char(_) => {
                if let KeyCode::Char(typed) = raw.code {
                    self.filter.query.push(typed);
                }
            }
            _ => return,
        }
        // The list changed shape under the cursor; keep it on a real row.
        self.provider_cursor.clamp(self.provider_count());
    }

    fn on_browse_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('f') => return self.dispatch(Action::Filter),
                KeyCode::Char('p') | KeyCode::Char('k') => return self.dispatch(Action::Palette),
                _ => {}
            }
        }

        match key.code {
            KeyCode::Char('q') => self.dispatch(Action::Quit),
            // Esc walks back one layer at a time rather than always quitting.
            KeyCode::Esc if self.filter.active() => {
                self.filter.clear();
                self.provider_cursor.clamp(self.provider_count());
                self.say(Tone::Info, "filter cleared");
            }
            KeyCode::Esc => self.dispatch(Action::Quit),
            KeyCode::Up | KeyCode::Char('k') => self.move_focused(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_focused(1),
            KeyCode::Tab | KeyCode::BackTab => self.focus = self.focus.other(),
            KeyCode::Left => self.focus = Pane::Agents,
            KeyCode::Right => self.focus = Pane::Providers,
            KeyCode::Enter => self.dispatch(Action::Detail),
            KeyCode::Char('/') => self.dispatch(Action::Filter),
            KeyCode::Char('?') => self.dispatch(Action::Help),
            KeyCode::Char('u') => self.dispatch(Action::Use(None)),
            KeyCode::Char('m') => self.dispatch(Action::Models),
            KeyCode::Char('e') => self.dispatch(Action::Edit),
            KeyCode::Char('a') => self.dispatch(Action::Add),
            KeyCode::Char('d') => self.dispatch(Action::Delete),
            KeyCode::Char('c') => self.dispatch(Action::Check),
            KeyCode::Char('C') => self.dispatch(Action::CheckAll),
            KeyCode::Char('s') => self.dispatch(Action::Sync { prune: false }),
            // Shift widens the same action, as it does for delete in most file managers.
            KeyCode::Char('S') => self.dispatch(Action::Sync { prune: true }),
            KeyCode::Char('p') => self.dispatch(Action::Preset(None)),
            KeyCode::Char('r') => self.dispatch(Action::Reload),
            _ => {}
        }
    }

    fn on_mouse(&mut self, mouse: MouseEvent) {
        let at = Position::new(mouse.column, mouse.row);
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => self.on_click(at),
            MouseEventKind::ScrollUp => self.on_scroll(at, -1),
            MouseEventKind::ScrollDown => self.on_scroll(at, 1),
            _ => {}
        }
    }

    fn on_click(&mut self, at: Position) {
        if self.overlay.is_some() {
            self.click_overlay(at);
            return;
        }

        let hint = self.hits.hints.iter().find(|(rect, _)| rect.contains(at));
        if let Some((_, action)) = hint {
            let action = action.clone();
            self.dispatch(action);
            return;
        }

        if let Some(index) = self.hits.agent_rows.row(at, self.agents.len()) {
            self.focus = Pane::Agents;
            if index != self.agent_cursor.index() {
                self.agent_cursor = Cursor { index };
                self.provider_cursor = Cursor::default();
            }
            return;
        }

        if let Some(index) = self.hits.provider_rows.row(at, self.provider_count()) {
            // Click to select, click the selected row again to open it.
            let reopen = self.focus == Pane::Providers && index == self.provider_cursor.index();
            self.focus = Pane::Providers;
            self.provider_cursor = Cursor { index };
            if reopen {
                self.dispatch(Action::Detail);
            }
            return;
        }

        if self.hits.agents.contains(at) {
            self.focus = Pane::Agents;
        } else if self.hits.providers.contains(at) {
            self.focus = Pane::Providers;
        }
    }

    fn click_overlay(&mut self, at: Position) {
        if !self.hits.overlay.is_some_and(|rect| rect.contains(at)) {
            self.overlay = None;
            return;
        }

        let rows = self.hits.overlay_rows;
        match self.overlay.as_mut() {
            Some(Overlay::Presets(picker)) => {
                if let Some(index) = rows.row(at, picker.presets.len()) {
                    picker.cursor = Cursor { index };
                }
            }
            Some(Overlay::Palette(palette)) => {
                if let Some(index) = rows.row(at, palette.matches.len()) {
                    palette.cursor = Cursor { index };
                }
            }
            Some(Overlay::Models(picker)) => {
                if let Some(index) = rows.row(at, picker.matches.len()) {
                    picker.cursor = Cursor { index };
                }
            }
            _ => {}
        }
    }

    fn on_scroll(&mut self, at: Position, delta: isize) {
        if let Some(overlay) = self.overlay.as_mut() {
            let scrolled = |scroll: u16| {
                if delta < 0 {
                    scroll.saturating_sub(1)
                } else {
                    scroll.saturating_add(1)
                }
            };
            match overlay {
                Overlay::About { scroll } => *scroll = scrolled(*scroll),
                Overlay::Detail(detail) => detail.scroll = scrolled(detail.scroll),
                Overlay::Presets(picker) => picker.cursor.step(delta, picker.presets.len()),
                Overlay::Palette(palette) => palette.cursor.step(delta, palette.matches.len()),
                Overlay::Models(picker) => picker.cursor.step(delta, picker.matches.len()),
                Overlay::Form(_) | Overlay::Confirm(_) => {}
            }
            return;
        }

        // The wheel reads what is under the pointer without stealing the focus.
        if self.hits.agents.contains(at) {
            self.agent_cursor.step(delta, self.agents.len());
            self.provider_cursor = Cursor::default();
        } else if self.hits.providers.contains(at) {
            self.provider_cursor.step(delta, self.provider_count());
        }
    }

    fn move_focused(&mut self, delta: isize) {
        match self.focus {
            Pane::Agents => {
                self.agent_cursor.step(delta, self.agents.len());
                self.provider_cursor = Cursor::default();
            }
            Pane::Providers => self.provider_cursor.step(delta, self.provider_count()),
        }
    }

    fn schedule(&mut self, job: Job, note: &str) {
        self.say(Tone::Info, note);
        self.pending = Some(job);
    }

    fn run_job(&mut self, terminal: &mut DefaultTerminal, job: Job) {
        match job {
            Job::Check => self.check_selected(),
            Job::CheckAll => self.check_all(terminal),
            Job::Models => self.open_models(),
            Job::Sync { prune } => self.sync_selected(prune),
        }
    }

    /// Probe one provider and remember the verdict for the health column.
    fn probe_provider(&mut self, agent_id: &str, provider: &Provider) -> Option<Health> {
        let base_url = provider.base_url.clone()?;
        let result = net::probe::probe(
            &base_url,
            provider.api_key.as_deref(),
            provider.wire_api,
            net::DEFAULT_TIMEOUT,
        );
        let health = Health { alive: result.alive(), millis: result.latency.as_millis() };
        self.health.record(agent_id, &provider.id, health);
        let tone = if health.alive { Tone::Good } else { Tone::Bad };
        self.say(tone, format!("{}: {}", provider.id, result.summary()));
        Some(health)
    }

    fn check_selected(&mut self) {
        let (Some(agent_id), Some(provider)) =
            (self.agent().map(|a| a.id.clone()), self.provider().cloned())
        else {
            return;
        };
        if self.probe_provider(&agent_id, &provider).is_none() {
            self.say(Tone::Bad, format!("{} has no base URL", provider.id));
        }
    }

    fn check_all(&mut self, terminal: &mut DefaultTerminal) {
        let (Some(agent_id), Some(providers)) =
            (self.agent().map(|a| a.id.clone()), self.agent().map(|a| a.providers.clone()))
        else {
            return;
        };

        let total = providers.len();
        let (mut up, mut down, mut skipped) = (0usize, 0usize, 0usize);
        for (index, provider) in providers.iter().enumerate() {
            self.say(Tone::Info, format!("checking {} ({}/{total})…", provider.id, index + 1));
            // Each probe blocks, so the frame announcing it has to land first.
            self.draw(terminal);

            match self.probe_provider(&agent_id, provider) {
                Some(health) if health.alive => up += 1,
                Some(_) => down += 1,
                None => skipped += 1,
            }
        }

        let mut summary = format!("{up} up · {down} down");
        if skipped > 0 {
            summary.push_str(&format!(" · {skipped} without a base URL"));
        }
        let tone = if down == 0 && skipped == 0 { Tone::Good } else { Tone::Info };
        self.say(tone, summary);
    }

    /// Ask the endpoint what it serves, recording its health and explaining any
    /// failure in the status line.
    ///
    /// `None` means nothing usable came back, and the caller should open nothing.
    fn discover(&mut self, provider: &Provider, agent_id: &str) -> Option<Vec<Model>> {
        let discovery = net::discover_models(provider, net::DEFAULT_TIMEOUT, false);
        let probe = discovery.probe.as_ref().or_else(|| {
            self.say(Tone::Bad, format!("{} has no base URL", provider.id));
            None
        })?;

        self.health.record(
            agent_id,
            &provider.id,
            Health { alive: probe.alive(), millis: probe.latency.as_millis() },
        );
        if !probe.alive() {
            self.say(Tone::Bad, format!("{} did not answer: {}", probe.url, probe.summary()));
            return None;
        }
        if discovery.models.is_empty() {
            self.say(Tone::Bad, format!("{} answered but listed no models", probe.url));
            return None;
        }
        Some(discovery.models)
    }

    fn open_models(&mut self) {
        let (Some(provider), Some(agent_id), current) = (
            self.provider().cloned(),
            self.agent().map(|a| a.id.clone()),
            self.agent().and_then(|a| a.model.clone()),
        ) else {
            return;
        };

        let Some(models) = self.discover(&provider, &agent_id) else { return };
        let count = models.len();
        self.overlay =
            Some(Overlay::Models(ModelPicker::new(provider.id.clone(), models, current)));
        self.say(Tone::Info, format!("{} serves {count} model(s)", provider.id));
    }

    fn sync_selected(&mut self, prune: bool) {
        let (Some(provider), Some(agent_id)) =
            (self.provider().cloned(), self.agent().map(|a| a.id.clone()))
        else {
            return;
        };

        let Some(models) = self.discover(&provider, &agent_id) else { return };
        let id = provider.id.clone();
        self.apply("sync", move |config| {
            let served: Vec<String> = models.iter().map(|model| model.id.clone()).collect();
            let count = models.len();

            let mut patch = Provider::new(&id);
            patch.models = models;
            config.upsert_provider(&patch)?;

            if !prune {
                return Ok(format!("synced {count} model(s) into {id}"));
            }
            let dropped = config.prune_models(&id, &served)?;
            Ok(format!("synced {count} model(s) into {id}, dropped {dropped} stale"))
        });
    }

    /// Make a provider active; `None` means whichever the cursor is on.
    fn use_provider(&mut self, id: Option<String>) {
        let Some(id) = id.or_else(|| self.provider().map(|p| p.id.clone())) else { return };
        let reported = id.clone();
        self.apply("select", move |config| {
            config.set_active_provider(&id)?;
            Ok(format!("{reported} is now active"))
        });
    }

    fn open_detail(&mut self) {
        if self.focus == Pane::Agents {
            self.focus = Pane::Providers;
            return;
        }
        self.overlay =
            Some(Overlay::Detail(Detail { scroll: 0, catalog: catalog::Catalog::cached_only() }));
    }

    fn open_edit_form(&mut self) {
        let (Some(agent_id), Some(provider)) =
            (self.agent().map(|a| a.id.clone()), self.provider().cloned())
        else {
            return;
        };
        self.overlay = Some(Overlay::Form(Form::edit(agent_id, &provider)));
    }

    fn open_add_form(&mut self) {
        let Some(agent_id) = self.agent().map(|a| a.id.clone()) else { return };
        self.overlay = Some(Overlay::Form(Form::add(agent_id)));
    }

    fn open_palette(&mut self) {
        self.overlay = Some(Overlay::Palette(CommandPalette::new(self)));
    }

    fn ask_delete(&mut self) {
        let (Some(agent), Some(provider)) = (self.agent(), self.provider()) else { return };
        self.overlay = Some(Overlay::Confirm(Confirm {
            prompt: format!("Delete {} from {}?", provider.id, agent.name),
            agent_id: agent.id.clone(),
            provider_id: provider.id.clone(),
        }));
    }

    fn open_presets(&mut self) {
        match preset::all() {
            Ok(presets) if presets.is_empty() => self.say(Tone::Info, "no presets available"),
            Ok(presets) => {
                self.overlay =
                    Some(Overlay::Presets(PresetPicker { presets, cursor: Cursor::default() }))
            }
            Err(err) => self.say(Tone::Bad, format!("presets unreadable: {err:#}")),
        }
    }

    fn on_overlay_key(&mut self, key: KeyEvent, raw: KeyEvent) {
        let Some(overlay) = self.overlay.take() else { return };
        let next = match overlay {
            Overlay::About { scroll } => {
                scroll_key(key, scroll).map(|scroll| Overlay::About { scroll })
            }
            Overlay::Detail(detail) => scroll_key(key, detail.scroll)
                .map(|scroll| Overlay::Detail(Detail { scroll, ..detail })),
            Overlay::Form(form) => self.form_key(key, raw, form),
            Overlay::Confirm(confirm) => self.confirm_key(key, confirm),
            Overlay::Presets(picker) => self.preset_key(key, picker),
            Overlay::Palette(palette) => self.palette_key(key, raw, palette),
            Overlay::Models(picker) => self.models_key(key, raw, picker),
        };
        // A handler that opened its own overlay — a form, the preset picker —
        // wins over the one it replaced.
        if self.overlay.is_none() {
            self.overlay = next;
        }
    }

    fn palette_key(
        &mut self,
        key: KeyEvent,
        raw: KeyEvent,
        mut palette: CommandPalette,
    ) -> Option<Overlay> {
        match key.code {
            KeyCode::Esc => return None,
            KeyCode::Enter => {
                if let Some(action) = palette.selected().cloned() {
                    self.dispatch(action);
                }
                return None;
            }
            KeyCode::Up => palette.cursor.step(-1, palette.matches.len()),
            KeyCode::Down => palette.cursor.step(1, palette.matches.len()),
            KeyCode::Backspace => {
                palette.query.pop();
                palette.rebuild();
            }
            KeyCode::Char(_) => {
                if let KeyCode::Char(typed) = raw.code {
                    palette.query.push(typed);
                    palette.rebuild();
                }
            }
            _ => {}
        }
        Some(Overlay::Palette(palette))
    }

    fn models_key(
        &mut self,
        key: KeyEvent,
        raw: KeyEvent,
        mut picker: ModelPicker,
    ) -> Option<Overlay> {
        match key.code {
            KeyCode::Esc => return None,
            KeyCode::Enter => {
                if let Some(model) = picker.selected() {
                    let provider_id = picker.provider_id.clone();
                    let model_id = model.id.clone();
                    let reported = model_id.clone();
                    self.apply("set model", move |config| {
                        // Attributed to the endpoint it came from, so agents that
                        // qualify the name resolve it against the right provider.
                        config.set_model_for(&provider_id, &model_id)?;
                        Ok(format!("model is now {reported}"))
                    });
                }
                return None;
            }
            KeyCode::Up => picker.cursor.step(-1, picker.matches.len()),
            KeyCode::Down => picker.cursor.step(1, picker.matches.len()),
            KeyCode::Backspace => {
                picker.query.pop();
                picker.rebuild();
            }
            KeyCode::Char(_) => {
                if let KeyCode::Char(typed) = raw.code {
                    picker.query.push(typed);
                    picker.rebuild();
                }
            }
            _ => {}
        }
        Some(Overlay::Models(picker))
    }

    fn form_key(&mut self, key: KeyEvent, raw: KeyEvent, mut form: Form) -> Option<Overlay> {
        let save = matches!(key.code, KeyCode::F(2))
            || (key.modifiers.contains(KeyModifiers::CONTROL)
                && matches!(key.code, KeyCode::Char('s') | KeyCode::Char('S')));

        if save {
            form.commit();
            return self.save_form(&form);
        }

        if form.editing {
            match key.code {
                KeyCode::Enter => form.commit(),
                KeyCode::Esc => form.cancel_entry(),
                KeyCode::Backspace => form.backspace(),
                KeyCode::Char(_) => {
                    if let KeyCode::Char(typed) = raw.code {
                        form.push(typed);
                    }
                }
                _ => {}
            }
            return Some(Overlay::Form(form));
        }

        match key.code {
            KeyCode::Esc => return None,
            KeyCode::Up | KeyCode::Char('k') => form.cursor.step(-1, form.fields.len()),
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                form.cursor.step(1, form.fields.len())
            }
            KeyCode::Enter => form.activate(),
            _ => {}
        }
        Some(Overlay::Form(form))
    }

    /// Returns the overlay to keep showing: the form stays open when it is invalid.
    fn save_form(&mut self, form: &Form) -> Option<Overlay> {
        let provider = match form.provider() {
            Ok(provider) => provider,
            Err(err) => {
                self.say(Tone::Bad, format!("{err:#}"));
                return Some(Overlay::Form(form.clone()));
            }
        };

        // The form targets the agent it was opened on, even if the selection
        // moved in the meantime.
        self.select_agent(&form.agent_id);

        let id = provider.id.clone();
        let reported = id.clone();
        self.apply("save", move |config| {
            config.upsert_provider(&provider)?;
            Ok(format!("saved {reported}"))
        });

        if self.status.tone == Tone::Bad {
            return Some(Overlay::Form(form.clone()));
        }
        // The endpoint may now be somewhere else entirely, so the old verdict lies.
        self.health.forget(&form.agent_id, &id);
        self.select_provider(&id);
        None
    }

    fn select_agent(&mut self, id: &str) {
        if let Some(index) = self.agents.iter().position(|entry| entry.id == id) {
            self.agent_cursor = Cursor { index };
        }
    }

    fn select_provider(&mut self, id: &str) {
        // A provider the live filter hides cannot be selected; dropping the
        // filter is less surprising than the row the user just saved vanishing.
        let visible = self.visible_providers().iter().any(|p| p.id == id);
        if !visible {
            self.filter.clear();
        }
        let index = self.visible_providers().iter().position(|p| p.id == id);
        if let Some(index) = index {
            self.provider_cursor = Cursor { index };
            self.focus = Pane::Providers;
        }
    }

    fn confirm_key(&mut self, key: KeyEvent, confirm: Confirm) -> Option<Overlay> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let Confirm { agent_id, provider_id, .. } = confirm;
                self.select_agent(&agent_id);
                self.health.forget(&agent_id, &provider_id);
                let id = provider_id;
                self.apply("delete", move |config| {
                    if config.remove_provider(&id)? {
                        Ok(format!("removed {id}"))
                    } else {
                        anyhow::bail!("no provider called {id:?}")
                    }
                });
                self.provider_cursor.clamp(self.provider_count());
                None
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.say(Tone::Info, "cancelled");
                None
            }
            _ => Some(Overlay::Confirm(confirm)),
        }
    }

    fn preset_key(&mut self, key: KeyEvent, mut picker: PresetPicker) -> Option<Overlay> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => None,
            KeyCode::Up | KeyCode::Char('k') => {
                picker.cursor.step(-1, picker.presets.len());
                Some(Overlay::Presets(picker))
            }
            KeyCode::Down | KeyCode::Char('j') => {
                picker.cursor.step(1, picker.presets.len());
                Some(Overlay::Presets(picker))
            }
            KeyCode::Enter => {
                if let Some(entry) = picker.presets.get(picker.cursor.index()) {
                    let entry = entry.clone();
                    self.apply_preset(&entry);
                }
                None
            }
            _ => Some(Overlay::Presets(picker)),
        }
    }

    fn apply_preset_by_id(&mut self, id: &str) {
        match preset::find(id) {
            Ok(entry) => self.apply_preset(&entry),
            Err(err) => self.say(Tone::Bad, format!("{err:#}")),
        }
    }

    fn apply_preset(&mut self, entry: &Preset) {
        let provider = match entry.provider(None) {
            Ok(provider) => provider,
            Err(err) => {
                self.say(Tone::Bad, format!("preset {}: {err:#}", entry.id));
                return;
            }
        };
        let warn_key = entry.missing_key(None);
        let preset_id = entry.id.clone();
        let provider_id = provider.id.clone();
        let default_model = entry.default_model.clone();
        let model_provider = provider.id.clone();
        let agent_id = self.agent().map(|a| a.id.clone()).unwrap_or_default();

        self.apply("preset", move |config| {
            config.upsert_provider(&provider)?;
            if let Some(model) = &default_model {
                // Attributed to the preset's own endpoint rather than the active
                // one, for the reason `set_model_for` exists. Not every agent
                // tracks a model, and failing to set one must not undo the
                // endpoint that was just written.
                let _ = config.set_model_for(&model_provider, model);
            }
            Ok(format!("applied {preset_id}"))
        });

        self.health.forget(&agent_id, &provider_id);
        if warn_key && self.status.tone == Tone::Good {
            self.say(
                Tone::Info,
                format!("applied {provider_id}, but it has no API key; edit it with 'e'"),
            );
        }
        self.select_provider(&provider_id);
    }

    fn render(&self, frame: &mut Frame) -> Hits {
        let area = frame.area();
        let compact = area.height < COMPACT_HEIGHT;
        let header_height = if compact { 1 } else { 3 };

        let [header, rule, body, status, hints] = Layout::vertical([
            Constraint::Length(header_height),
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);

        self.render_header(frame, header, compact);
        frame.render_widget(
            Paragraph::new("─".repeat(rule.width as usize))
                .style(Style::default().fg(palette::ACCENT_MUTED)),
            rule,
        );

        let [left, right] =
            Layout::horizontal([Constraint::Length(AGENT_PANE_WIDTH), Constraint::Min(24)])
                .areas(body);

        let mut hits = Hits {
            agents: left,
            providers: right,
            agent_rows: self.agent_pane(left).render(frame, left),
            provider_rows: self.provider_pane(right).render(frame, right),
            ..Hits::default()
        };

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!(" {} ", self.status.tone.glyph()), self.status.tone.style()),
                Span::styled(self.status.text.clone(), Style::default().fg(palette::TEXT)),
            ])),
            status,
        );
        hits.hints = self.render_hints(frame, hints);

        match &self.overlay {
            Some(Overlay::About { scroll }) => hits.overlay = Some(render_about(frame, *scroll)),
            Some(Overlay::Detail(detail)) => hits.overlay = Some(self.render_detail(frame, detail)),
            Some(Overlay::Form(form)) => hits.overlay = Some(render_form(frame, form)),
            Some(Overlay::Confirm(confirm)) => hits.overlay = Some(render_confirm(frame, confirm)),
            Some(Overlay::Presets(picker)) => {
                let (area, rows) = render_presets(frame, picker);
                hits.overlay = Some(area);
                hits.overlay_rows = rows;
            }
            Some(Overlay::Palette(palette)) => {
                let (area, rows) = self.render_palette(frame, palette);
                hits.overlay = Some(area);
                hits.overlay_rows = rows;
            }
            Some(Overlay::Models(picker)) => {
                let (area, rows) = render_models(frame, picker);
                hits.overlay = Some(area);
                hits.overlay_rows = rows;
            }
            None => {}
        }
        hits
    }

    fn render_header(&self, frame: &mut Frame, area: Rect, compact: bool) {
        let mark = Style::default().fg(palette::ACCENT);
        let name = Style::default().fg(palette::TEXT).add_modifier(Modifier::BOLD);
        let muted = Style::default().fg(palette::MUTED);
        let faint = Style::default().fg(palette::FAINT);

        if compact {
            let [left, right] =
                Layout::horizontal([Constraint::Min(10), Constraint::Length(24)]).areas(area);
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(format!(" {} ", brand::MARK), mark),
                    Span::styled(brand::NAME, name),
                    Span::styled(format!(" v{}", brand::VERSION), faint),
                ])),
                left,
            );
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(format!("{} ", brand::VENDOR), faint)))
                    .alignment(Alignment::Right),
                right,
            );
            return;
        }

        // The mark alone, not a drawn wordmark: box-drawing art mixes weights
        // differently across terminals and fonts, and came out looking broken.
        let [logo_area, words, links] = Layout::horizontal([
            Constraint::Length(4),
            Constraint::Min(10),
            Constraint::Length(brand::REPOSITORY_SHORT.chars().count() as u16 + 2),
        ])
        .areas(area);

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" {} ", brand::MARK),
                mark.add_modifier(Modifier::BOLD),
            ))),
            logo_area,
        );

        let mut word_lines = vec![Line::from(Span::styled(brand::NAME, name))];
        if area.width >= TAGLINE_WIDTH {
            word_lines.push(Line::from(Span::styled(brand::TAGLINE, muted)));
        }
        frame.render_widget(Paragraph::new(word_lines), words);

        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(format!("v{}", brand::VERSION), muted)),
                Line::from(Span::styled(brand::VENDOR, faint)),
                Line::from(Span::styled(brand::REPOSITORY_SHORT, faint)),
            ])
            .alignment(Alignment::Right),
            links,
        );
    }

    /// Draw the hint bar, returning where each clickable hint landed.
    fn render_hints(&self, frame: &mut Frame, area: Rect) -> Vec<(Rect, Action)> {
        // The header already carries the vendor and the repository; repeating
        // the vendor down here only crowded the keys off the end of the bar.
        let keys = area;

        let mut spans = vec![Span::raw(" ")];
        let mut boxes = Vec::new();
        let mut x = keys.x + 1;

        for (index, hint) in self.hints().into_iter().enumerate() {
            let separator = if index > 0 { 3 } else { 0 };
            let width = (hint.key.chars().count() + 1 + hint.label.chars().count()) as u16;
            // Drop a hint that does not fit rather than letting it be sliced in
            // half: a truncated key is worse than an absent one, and the palette
            // and help screen both still list it.
            if x + separator + width > keys.right() {
                break;
            }

            if separator > 0 {
                spans.push(Span::styled(" · ", Style::default().fg(palette::FAINT)));
                x += separator;
            }
            if let Some(action) = hint.action {
                boxes.push((Rect { x, y: keys.y, width, height: 1 }, action));
            }
            spans.push(Span::styled(hint.key, Style::default().fg(palette::ACCENT)));
            spans.push(Span::styled(
                format!(" {}", hint.label),
                Style::default().fg(palette::MUTED),
            ));
            x += width;
        }

        frame.render_widget(Paragraph::new(Line::from(spans)), keys);
        boxes
    }

    fn hints(&self) -> Vec<Hint> {
        match &self.overlay {
            Some(Overlay::About { .. }) | Some(Overlay::Detail(_)) => {
                vec![Hint::plain("↑↓", "scroll"), Hint::plain("esc", "close")]
            }
            Some(Overlay::Form(form)) if form.editing => vec![
                Hint::plain("type", "value"),
                Hint::plain("enter", "accept"),
                Hint::plain("esc", "discard"),
                Hint::plain("ctrl+s", "save"),
            ],
            Some(Overlay::Form(_)) => vec![
                Hint::plain("↑↓", "field"),
                Hint::plain("enter", "edit/cycle"),
                Hint::plain("ctrl+s or F2", "save"),
                Hint::plain("esc", "cancel"),
            ],
            Some(Overlay::Confirm(_)) => {
                vec![Hint::plain("y", "delete"), Hint::plain("n", "cancel")]
            }
            Some(Overlay::Presets(_)) => vec![
                Hint::plain("↑↓", "move"),
                Hint::plain("enter", "apply"),
                Hint::plain("esc", "cancel"),
            ],
            Some(Overlay::Palette(_)) => vec![
                Hint::plain("type", "search"),
                Hint::plain("↑↓", "move"),
                Hint::plain("enter", "run"),
                Hint::plain("esc", "close"),
            ],
            Some(Overlay::Models(_)) => vec![
                Hint::plain("type", "filter"),
                Hint::plain("↑↓", "move"),
                Hint::plain("enter", "use this model"),
                Hint::plain("esc", "cancel"),
            ],
            None if self.filter.editing => vec![
                Hint::plain("type", "filter"),
                Hint::plain("enter", "accept"),
                Hint::plain("esc", "clear"),
            ],
            None => {
                let mut hints = vec![Hint::plain("↑↓", "move")];
                hints.extend(match self.focus {
                    Pane::Agents => {
                        Hint::bar([Action::Palette, Action::Reload, Action::Help, Action::Quit])
                    }
                    Pane::Providers => Hint::bar([
                        Action::Detail,
                        Action::Filter,
                        Action::Use(None),
                        Action::Models,
                        Action::Add,
                        Action::Edit,
                        Action::Delete,
                        Action::Check,
                        Action::Sync { prune: false },
                        Action::Preset(None),
                        Action::Palette,
                        Action::Help,
                    ]),
                });
                hints
            }
        }
    }

    fn agent_pane(&self, area: Rect) -> ListPane {
        let inner = area.width.saturating_sub(2) as usize;
        // The name absorbs the slack; the count and the two glyphs are fixed.
        let columns = Columns::flexible(inner.saturating_sub(1), &[1, 3, 1], 1, 8);

        let cursor_agent = self.agent().map(|a| a.id.as_str());
        let rows = self
            .agents
            .iter()
            .map(|entry| {
                let here = cursor_agent == Some(entry.id.as_str());
                let (glyph, glyph_style) = entry.detection_mark();
                let count = match &entry.error {
                    Some(_) => "err".to_string(),
                    None => entry.providers.len().to_string(),
                };

                let mut row = Row::default();
                row.cell(
                    &columns,
                    if here { brand::MARK } else { " " },
                    Style::default().fg(palette::ACCENT),
                );
                row.cell(&columns, &entry.name, Style::default().fg(palette::TEXT));
                row.cell(&columns, &count, Style::default().fg(palette::MUTED));
                row.cell(&columns, glyph, glyph_style);
                row.dim = !entry.detection.installed();
                row
            })
            .collect();

        ListPane {
            title: format!("agents {}", self.agents.len()),
            focused: self.focus == Pane::Agents,
            header: Some(columns.header(&["", "agent", "prv", ""])),
            rows,
            selected: self.agent_cursor.index(),
            empty: vec!["no agents known".to_string()],
        }
    }

    fn provider_pane(&self, area: Rect) -> ListPane {
        let focused = self.focus == Pane::Providers;
        let Some(agent) = self.agent() else {
            return ListPane::notice("providers", focused, vec!["no agent selected".to_string()]);
        };
        if let Some(err) = &agent.error {
            return ListPane::notice(
                &format!("{} · providers", agent.name),
                focused,
                vec![err.clone()],
            );
        }

        let total = agent.providers.len();
        let visible = self.visible_providers();
        let title = if self.filter.active() || self.filter.editing {
            format!(
                "{} · providers {}/{} · /{}{}",
                agent.name,
                visible.len(),
                total,
                self.filter.query,
                if self.filter.editing { CURSOR_BAR } else { "" }
            )
        } else {
            format!("{} · providers {}", agent.name, total)
        };

        if total == 0 {
            return ListPane::notice(
                &title,
                focused,
                vec![
                    format!("{} knows no providers yet", agent.name),
                    String::new(),
                    "press 'a' to add one, or 'p' to apply a preset".to_string(),
                ],
            );
        }
        if visible.is_empty() {
            return ListPane::notice(
                &title,
                focused,
                vec![
                    format!("nothing matches \"{}\"", self.filter.query.trim()),
                    String::new(),
                    "press esc to clear the filter".to_string(),
                ],
            );
        }

        let inner = area.width.saturating_sub(2) as usize;
        let columns = Columns::flexible(inner.saturating_sub(1), &[1, 16, 11, 9, 4, 9], 2, 10);

        let rows = visible
            .iter()
            .map(|provider| {
                let mut row = Row::default();
                row.cell(
                    &columns,
                    if agent.is_active(&provider.id) { brand::MARK } else { " " },
                    Style::default().fg(palette::ACCENT),
                );
                row.cell(&columns, &provider.id, Style::default().fg(palette::TEXT));
                row.cell(
                    &columns,
                    provider.host().unwrap_or(ABSENT),
                    Style::default().fg(palette::MUTED),
                );
                row.cell(
                    &columns,
                    &provider.api_key.as_deref().map(mask).unwrap_or_else(|| ABSENT.to_string()),
                    Style::default().fg(palette::FAINT),
                );
                row.cell(
                    &columns,
                    provider.wire_api.map(WireApi::as_str).unwrap_or(ABSENT),
                    Style::default().fg(palette::MUTED),
                );
                row.cell(
                    &columns,
                    &if agent.capabilities.per_provider_models {
                        provider.models.len().to_string()
                    } else {
                        ABSENT.to_string()
                    },
                    Style::default().fg(palette::MUTED),
                );

                let (health_text, health_style) = match self.health.get(&agent.id, &provider.id) {
                    Some(health) => (
                        format!("{DOT_FILLED} {}ms", health.millis),
                        Style::default().fg(if health.alive {
                            palette::GOOD
                        } else {
                            palette::BAD
                        }),
                    ),
                    None => (DOT_HOLLOW.to_string(), Style::default().fg(palette::FAINT)),
                };
                row.cell(&columns, &health_text, health_style);
                row
            })
            .collect();

        ListPane {
            title,
            focused,
            header: Some(columns.header(&["", "id", "host", "key", "wire", "mdl", "health"])),
            rows,
            selected: self.provider_cursor.index(),
            empty: Vec::new(),
        }
    }

    fn render_detail(&self, frame: &mut Frame, detail: &Detail) -> Rect {
        let area = centered(frame.area(), 76, 78);
        let (Some(agent), Some(provider)) = (self.agent(), self.provider()) else { return area };

        let mut lines = vec![
            labelled("agent", &agent.name),
            labelled("config", &agent.config_path.display().to_string()),
            labelled("id", &provider.id),
            labelled("display name", provider.display_name.as_deref().unwrap_or(ABSENT)),
            labelled("base url", provider.base_url.as_deref().unwrap_or(ABSENT)),
            labelled(
                "api key",
                &provider.api_key.as_deref().map(mask).unwrap_or_else(|| ABSENT.to_string()),
            ),
            labelled("wire api", provider.wire_api.map(WireApi::as_str).unwrap_or(ABSENT)),
            labelled("active", if agent.is_active(&provider.id) { "yes" } else { "no" }),
            labelled("agent model", agent.model.as_deref().unwrap_or(ABSENT)),
            labelled(
                "health",
                &match self.health.get(&agent.id, &provider.id) {
                    Some(health) if health.alive => format!("up · {}ms", health.millis),
                    Some(health) => format!("down · {}ms", health.millis),
                    None => "not checked this session".to_string(),
                },
            ),
        ];

        for (key, value) in &provider.extras {
            lines.push(labelled(key, value));
        }

        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            format!("models ({})", provider.models.len()),
            Style::default().fg(palette::ACCENT).add_modifier(Modifier::BOLD),
        )));
        if provider.models.is_empty() {
            lines.push(Line::from(Span::styled(
                "  none recorded; press 's' to sync",
                Style::default().fg(palette::FAINT),
            )));
        }
        for model in &provider.models {
            let facts = detail.catalog.as_ref().and_then(|c| c.lookup(&model.id));
            let limits = format_limits(
                model.context_limit.or_else(|| facts.and_then(|f| f.context)),
                model.output_limit.or_else(|| facts.and_then(|f| f.output)),
            );

            let mut spans = vec![
                Span::raw("  "),
                Span::styled(model.label().to_string(), Style::default().fg(palette::TEXT)),
            ];
            if let Some(limits) = limits {
                spans
                    .push(Span::styled(format!("  {limits}"), Style::default().fg(palette::MUTED)));
            }
            if let Some(price) = facts.and_then(catalog::Facts::price) {
                spans.push(Span::styled(format!("  {price}"), Style::default().fg(palette::FAINT)));
            }
            lines.push(Line::from(spans));
        }

        let inner = overlay_frame(frame, area, &format!("provider · {}", provider.id));
        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }).scroll((detail.scroll, 0)),
            inner,
        );
        area
    }

    fn render_palette(&self, frame: &mut Frame, palette: &CommandPalette) -> (Rect, RowsAt) {
        let search = Search {
            title: "commands",
            query: &palette.query,
            placeholder: "type to search every action",
            empty: "no action matches that",
            count: palette.matches.len(),
            selected: palette.cursor.index(),
        };

        render_search_overlay(frame, search, |width| {
            let columns = Columns::flexible(width, &[26, 18], 1, 16);
            palette
                .matches
                .iter()
                .filter_map(|index| palette.entries.get(*index))
                .map(|action| {
                    let mut row = Row::default();
                    row.cell(&columns, &action.label(), Style::default().fg(palette::TEXT));
                    row.cell(&columns, &action.description(), Style::default().fg(palette::MUTED));
                    row.cell(
                        &columns,
                        &format!("{:>18}", action.binding().unwrap_or("")),
                        Style::default().fg(palette::FAINT),
                    );
                    // Blocked actions are shown, not hidden, so the palette stays
                    // a stable map of what the program can do.
                    row.dim = action.unavailable(self).is_some();
                    row
                })
                .collect()
        })
    }
}

fn render_models(frame: &mut Frame, picker: &ModelPicker) -> (Rect, RowsAt) {
    let search = Search {
        title: &format!("models · {}", picker.provider_id),
        query: &picker.query,
        placeholder: "type to filter",
        empty: "no model matches that",
        count: picker.matches.len(),
        selected: picker.cursor.index(),
    };

    render_search_overlay(frame, search, |width| {
        let columns = Columns::flexible(width, &[1, 7, 7, 22], 1, 20);
        picker
            .matches
            .iter()
            .filter_map(|index| picker.models.get(*index))
            .map(|model| {
                let facts = picker.catalog.as_ref().and_then(|c| c.lookup(&model.id));
                let limit = |value: Option<u64>| {
                    value.map(ui::tokens).unwrap_or_else(|| ABSENT.to_string())
                };

                let mut row = Row::default();
                row.cell(
                    &columns,
                    if picker.is_current(model) { brand::MARK } else { " " },
                    Style::default().fg(palette::ACCENT),
                );
                row.cell(&columns, &model.id, Style::default().fg(palette::TEXT));
                row.cell(
                    &columns,
                    &limit(model.context_limit.or_else(|| facts.and_then(|f| f.context))),
                    Style::default().fg(palette::MUTED),
                );
                row.cell(
                    &columns,
                    &limit(model.output_limit.or_else(|| facts.and_then(|f| f.output))),
                    Style::default().fg(palette::MUTED),
                );
                row.cell(
                    &columns,
                    &facts.and_then(catalog::Facts::price).unwrap_or_else(|| ABSENT.to_string()),
                    Style::default().fg(palette::FAINT),
                );
                row
            })
            .collect()
    })
}

/// The parts of a search overlay that do not depend on what is being searched.
struct Search<'a> {
    title: &'a str,
    query: &'a str,
    placeholder: &'a str,
    empty: &'a str,
    count: usize,
    selected: usize,
}

/// A query line above a list: the shape both the command palette and the model
/// picker take.
///
/// `rows` is handed the width its columns have to fit into, because that is not
/// known until the panel has been laid out.
fn render_search_overlay(
    frame: &mut Frame,
    search: Search<'_>,
    rows: impl FnOnce(usize) -> Vec<Row>,
) -> (Rect, RowsAt) {
    dim_behind(frame);

    let screen = frame.area();
    let height = (search.count as u16 + 4).clamp(5, screen.height.saturating_sub(4).max(5));
    let area = centered_fixed(screen, 76, height);
    let inner = overlay_frame(frame, area, search.title);

    let [input, list] = Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).areas(inner);

    let mut prompt = vec![
        Span::styled(format!("{} ", brand::MARK), Style::default().fg(palette::ACCENT)),
        Span::styled(search.query.to_string(), Style::default().fg(palette::TEXT)),
        Span::styled(CURSOR_BAR, Style::default().fg(palette::ACCENT)),
    ];
    if search.query.is_empty() {
        prompt.push(Span::styled(
            format!(" {}", search.placeholder),
            Style::default().fg(palette::FAINT),
        ));
    }
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(prompt),
            Line::from(Span::styled(
                "─".repeat(input.width as usize),
                Style::default().fg(palette::ACCENT_MUTED),
            )),
        ]),
        input,
    );

    if search.count == 0 {
        render_centered(frame, list, &[search.empty.to_string()]);
        return (area, RowsAt::default());
    }

    let rows = rows(list.width.saturating_sub(1) as usize);
    let rows_at = render_rows(frame, list, rows, search.selected, true);
    (area, rows_at)
}

/// Scroll-or-close, shared by every read-only overlay.
fn scroll_key(key: KeyEvent, scroll: u16) -> Option<u16> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => None,
        KeyCode::Up | KeyCode::Char('k') => Some(scroll.saturating_sub(1)),
        KeyCode::Down | KeyCode::Char('j') => Some(scroll.saturating_add(1)),
        _ => Some(scroll),
    }
}

fn labelled<'a>(label: &str, value: &str) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{label:>14}  "), Style::default().fg(palette::MUTED)),
        Span::styled(value.to_string(), Style::default().fg(palette::TEXT)),
    ])
}

/// Push what is already drawn back a step, so a modal reads as the live surface.
///
/// Terminals cannot blend, so this dims what is there rather than washing over it.
fn dim_behind(frame: &mut Frame) {
    let area = frame.area();
    frame.buffer_mut().set_style(area, Style::default().add_modifier(Modifier::DIM));
}

/// Chrome every overlay shares: cleared ground, rounded accent border, its own
/// background so it reads as floating. Returns the area to draw into.
fn overlay_frame(frame: &mut Frame, area: Rect, title: &str) -> Rect {
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(palette::ACCENT))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(palette::ACCENT).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(palette::OVERLAY_BG).fg(palette::TEXT));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

fn render_about(frame: &mut Frame, scroll: u16) -> Rect {
    let mut lines: Vec<Line> = brand::logo_lines()
        .map(|line| {
            Line::from(Span::styled(line.to_string(), Style::default().fg(palette::ACCENT)))
                .centered()
        })
        .collect();

    lines.push(Line::default());
    for (text, style) in [
        (brand::TAGLINE.to_string(), Style::default().fg(palette::TEXT)),
        (brand::signature(), Style::default().fg(palette::MUTED)),
        (brand::WEBSITE.to_string(), Style::default().fg(palette::FAINT)),
        (brand::REPOSITORY.to_string(), Style::default().fg(palette::FAINT)),
    ] {
        lines.push(Line::from(Span::styled(text, style)).centered());
    }
    lines.push(Line::default());

    let rows: Vec<(String, String)> = NAVIGATION
        .iter()
        .map(|(key, what)| ((*key).to_string(), (*what).to_string()))
        .chain(menu().iter().filter_map(|action| {
            action.binding().map(|key| (key.to_string(), action.description()))
        }))
        .collect();

    let key_width = rows.iter().map(|(key, _)| key.chars().count()).max().unwrap_or(0);
    for (key, what) in rows {
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {}", fit(&key, key_width)),
                Style::default().fg(palette::ACCENT),
            ),
            Span::styled(format!("   {what}"), Style::default().fg(palette::MUTED)),
        ]));
    }

    let area = centered(frame.area(), 74, 90);
    let inner = overlay_frame(frame, area, &format!("about {}", brand::NAME));
    frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), inner);
    area
}

fn render_form(frame: &mut Frame, form: &Form) -> Rect {
    let rows: Vec<Line> = form
        .fields
        .iter()
        .enumerate()
        .map(|(index, (kind, value))| {
            let selected = index == form.cursor.index();
            let mut spans = vec![Span::styled(
                format!("{:>14}  ", kind.label()),
                Style::default().fg(if selected { palette::ACCENT } else { palette::MUTED }),
            )];

            if selected && form.editing {
                spans.push(Span::styled(form.buffer.clone(), Style::default().fg(palette::TEXT)));
                spans.push(Span::styled(
                    CURSOR_BAR,
                    Style::default().fg(palette::ACCENT).add_modifier(Modifier::BOLD),
                ));
            } else {
                // A key is only ever legible while it is the field being typed into.
                let shown = if value.is_empty() {
                    ABSENT.to_string()
                } else if *kind == FieldKind::ApiKey {
                    mask(value)
                } else {
                    value.clone()
                };
                let style = if value.is_empty() {
                    Style::default().fg(palette::FAINT)
                } else {
                    Style::default().fg(palette::TEXT)
                };
                spans.push(Span::styled(shown, style));
            }

            let mut line = Line::from(spans);
            if selected {
                line = line.style(Style::default().bg(palette::SELECTION_BG));
            }
            line
        })
        .collect();

    let height = (rows.len() as u16) + 2;
    let area = centered_fixed(frame.area(), 68, height);
    let inner = overlay_frame(frame, area, &form.title());
    frame.render_widget(Paragraph::new(rows), inner);
    area
}

fn render_confirm(frame: &mut Frame, confirm: &Confirm) -> Rect {
    let area = centered_fixed(frame.area(), 60, 5);
    let inner = overlay_frame(frame, area, "confirm");
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(confirm.prompt.clone(), Style::default().fg(palette::TEXT))),
            Line::default(),
            Line::from(vec![
                Span::styled("y", Style::default().fg(palette::BAD)),
                Span::styled(" delete", Style::default().fg(palette::MUTED)),
                Span::styled(" · ", Style::default().fg(palette::FAINT)),
                Span::styled("n", Style::default().fg(palette::ACCENT)),
                Span::styled(" cancel", Style::default().fg(palette::MUTED)),
            ]),
        ])
        .wrap(Wrap { trim: false }),
        inner,
    );
    area
}

fn render_presets(frame: &mut Frame, picker: &PresetPicker) -> (Rect, RowsAt) {
    let area = centered(frame.area(), 74, 74);
    let inner = area.width.saturating_sub(2) as usize;
    let columns = Columns::flexible(inner.saturating_sub(1), &[18, 22], 2, 16);

    let rows = picker
        .presets
        .iter()
        .map(|entry| {
            let url = entry
                .provider(None)
                .ok()
                .and_then(|p| p.base_url)
                .unwrap_or_else(|| ABSENT.to_string());
            let mut row = Row::default();
            row.cell(&columns, &entry.id, Style::default().fg(palette::TEXT));
            row.cell(&columns, &entry.name, Style::default().fg(palette::MUTED));
            row.cell(&columns, &url, Style::default().fg(palette::FAINT));
            row
        })
        .collect();

    let rows_at = ListPane {
        title: format!("presets {}", picker.presets.len()),
        focused: true,
        header: Some(columns.header(&["preset", "name", "base url"])),
        rows,
        selected: picker.cursor.index(),
        empty: vec!["no presets available".to_string()],
    }
    .render_overlay(frame, area);
    (area, rows_at)
}

/// One row of already-fitted, individually styled cells.
#[derive(Default)]
struct Row {
    cells: Vec<Span<'static>>,
    /// Drawn faded, for something that exists but cannot be acted on.
    dim: bool,
}

impl Row {
    /// Append the next column's cell, padded and clipped by `columns`.
    fn cell(&mut self, columns: &Columns, text: &str, style: Style) {
        let index = self.cells.len();
        self.cells.push(Span::styled(columns.cell(index, text), style));
    }

    fn into_line(self, selected: bool, focused: bool, width: usize) -> Line<'static> {
        let bar = if selected {
            Span::styled(CURSOR_BAR, Style::default().fg(palette::ACCENT))
        } else {
            Span::raw(" ")
        };

        let drawn: usize =
            1 + self.cells.iter().map(|span| span.content.chars().count()).sum::<usize>();
        let mut spans = vec![bar];
        spans.extend(self.cells);
        // Pad to the full width so a selected row's background reaches the edge.
        spans.push(Span::raw(" ".repeat(width.saturating_sub(drawn))));

        let mut style = Style::default();
        if selected {
            style = style.add_modifier(Modifier::BOLD);
            // The bar alone marks the selection in an unfocused pane, so you can
            // still see where tabbing back will land you.
            if focused {
                style = style.bg(palette::SELECTION_BG);
            }
        }
        if self.dim {
            style = style.add_modifier(Modifier::DIM);
        }
        Line::from(spans).style(style)
    }
}

/// Draw rows with the shared selection styling and a scrollbar when they
/// overflow, reporting where they landed so a click can be resolved to a row.
fn render_rows(
    frame: &mut Frame,
    area: Rect,
    rows: Vec<Row>,
    selected: usize,
    focused: bool,
) -> RowsAt {
    let width = area.width as usize;
    let count = rows.len();
    let items: Vec<ListItem> = rows
        .into_iter()
        .enumerate()
        .map(|(index, row)| ListItem::new(row.into_line(index == selected, focused, width)))
        .collect();

    let mut state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(List::new(items), area, &mut state);

    if count > area.height as usize {
        let mut scroll = ScrollbarState::new(count).position(selected);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .thumb_style(Style::default().fg(palette::ACCENT))
                .track_style(Style::default().fg(palette::FAINT)),
            area,
            &mut scroll,
        );
    }

    RowsAt { area, offset: state.offset() }
}

/// A bordered, scrolling list with a column header and an empty state. Every
/// list in the app is this shape.
struct ListPane {
    title: String,
    focused: bool,
    header: Option<Line<'static>>,
    rows: Vec<Row>,
    selected: usize,
    /// Centred lines to show instead of rows when there are none.
    empty: Vec<String>,
}

impl ListPane {
    fn notice(title: &str, focused: bool, empty: Vec<String>) -> Self {
        Self {
            title: title.to_string(),
            focused,
            header: None,
            rows: Vec::new(),
            selected: 0,
            empty,
        }
    }

    fn render(self, frame: &mut Frame, area: Rect) -> RowsAt {
        let (border, title) = if self.focused {
            (palette::ACCENT, Style::default().fg(palette::ACCENT).add_modifier(Modifier::BOLD))
        } else {
            (palette::FAINT, Style::default().fg(palette::MUTED))
        };
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border))
            .title(Span::styled(format!(" {} ", self.title), title));
        self.fill(frame, area, block)
    }

    /// The same pane wearing an overlay's chrome rather than a pane border.
    fn render_overlay(self, frame: &mut Frame, area: Rect) -> RowsAt {
        frame.render_widget(Clear, area);
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(palette::ACCENT))
            .title(Span::styled(
                format!(" {} ", self.title),
                Style::default().fg(palette::ACCENT).add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(palette::OVERLAY_BG).fg(palette::TEXT));
        self.fill(frame, area, block)
    }

    fn fill(self, frame: &mut Frame, area: Rect, block: Block<'static>) -> RowsAt {
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.rows.is_empty() {
            render_centered(frame, inner, &self.empty);
            return RowsAt::default();
        }

        let body = match self.header {
            Some(header) => {
                let [head, rest] =
                    Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);
                frame.render_widget(Paragraph::new(header), head);
                rest
            }
            None => inner,
        };
        render_rows(frame, body, self.rows, self.selected, self.focused)
    }
}

/// Muted text parked in the middle of an otherwise empty area.
fn render_centered(frame: &mut Frame, area: Rect, lines: &[String]) {
    if area.height == 0 || lines.is_empty() {
        return;
    }
    let top = area.height.saturating_sub(lines.len() as u16) / 2;
    let box_area = Rect { y: area.y + top, height: area.height - top, ..area };
    let text: Vec<Line> = lines
        .iter()
        .map(|line| {
            Line::from(Span::styled(line.clone(), Style::default().fg(palette::MUTED))).centered()
        })
        .collect();
    frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), box_area);
}

/// Fixed-width text columns, so headers and rows line up without a table widget.
struct Columns {
    widths: Vec<usize>,
}

impl Columns {
    /// Columns for `total` cells of space where the column at `flex_at` absorbs
    /// whatever the fixed ones leave, never falling below `min`.
    ///
    /// `fixed` lists every column except the flexible one, in order.
    fn flexible(total: usize, fixed: &[usize], flex_at: usize, min: usize) -> Self {
        let gutters = fixed.len() + 1;
        let taken = fixed.iter().sum::<usize>() + gutters;
        let mut widths = fixed.to_vec();
        widths.insert(flex_at.min(widths.len()), total.saturating_sub(taken).max(min));
        Self { widths }
    }

    /// One cell: clipped to its column, padded to it, plus the gutter space.
    fn cell(&self, index: usize, text: &str) -> String {
        let width = self.widths.get(index).copied().unwrap_or_else(|| text.chars().count());
        format!("{} ", fit(text, width))
    }

    fn row(&self, cells: &[&str]) -> String {
        cells.iter().enumerate().map(|(index, cell)| self.cell(index, cell)).collect()
    }

    fn header(&self, cells: &[&str]) -> Line<'static> {
        // The header sits under the cursor-bar column, like every row does.
        Line::from(Span::styled(
            format!(" {}", self.row(cells)),
            Style::default().fg(palette::FAINT).add_modifier(Modifier::BOLD),
        ))
    }
}

/// Truncate to `width` characters, marking the cut, then pad to exactly `width`.
fn fit(text: &str, width: usize) -> String {
    let count = text.chars().count();
    if count <= width {
        let mut out = text.to_string();
        out.push_str(&" ".repeat(width - count));
        return out;
    }
    match width {
        0 => String::new(),
        1 => "…".to_string(),
        _ => {
            let kept: String = text.chars().take(width - 1).collect();
            format!("{kept}…")
        }
    }
}

/// `1M ctx · 128K out`, or whichever half is known.
///
/// The rounding is [`ui::tokens`], shared with the CLI's model table so the same
/// window is never described two ways.
fn format_limits(context: Option<u64>, output: Option<u64>) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(context) = context {
        parts.push(format!("{} ctx", ui::tokens(context)));
    }
    if let Some(output) = output {
        parts.push(format!("{} out", ui::tokens(output)));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn centered(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let [_, middle, _] = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .areas(area);
    let [_, centre, _] = Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .areas(middle);
    centre
}

/// A centred box of a known size, clamped so it always fits the terminal.
fn centered_fixed(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod render_tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// A fixed two-agent scene, so a render test does not depend on what happens
    /// to be installed on the machine running it.
    fn scene() -> App {
        let mut codex = AgentEntry {
            id: "codex".into(),
            name: "Codex".into(),
            detection: Detection { binary_on_path: true, config_exists: true },
            capabilities: Capabilities {
                named_providers: true,
                selectable_provider: true,
                per_provider_models: false,
                inline_api_key: true,
                mcp: true,
            },
            config_path: PathBuf::from("/home/u/.codex/config.toml"),
            providers: Vec::new(),
            active: Some("primary".into()),
            model: Some("gpt-5.5".into()),
            error: None,
        };
        for (id, url, key) in [
            ("primary", "http://192.0.2.10:8080/v1", Some("sk-example-key-0001")),
            ("backup", "https://backup.example/v1", Some("tok-example-0002")),
            ("byesu", "https://byesu.com/v1", None),
        ] {
            let mut provider = Provider::new(id);
            provider.base_url = Some(url.into());
            provider.api_key = key.map(str::to_owned);
            provider.wire_api = Some(WireApi::Responses);
            codex.providers.push(provider);
        }

        let opencode = AgentEntry {
            id: "opencode".into(),
            name: "opencode".into(),
            detection: Detection { binary_on_path: false, config_exists: true },
            capabilities: Capabilities {
                named_providers: true,
                selectable_provider: true,
                per_provider_models: true,
                inline_api_key: true,
                mcp: true,
            },
            config_path: PathBuf::from("/home/u/.config/opencode/opencode.json"),
            providers: Vec::new(),
            active: None,
            model: Some("codexsale/gpt-5.5".into()),
            error: None,
        };

        App {
            agents: vec![codex, opencode],
            agent_cursor: Cursor::default(),
            provider_cursor: Cursor::default(),
            focus: Pane::Providers,
            filter: Filter::default(),
            health: HealthCache::default(),
            overlay: None,
            status: Status::default(),
            pending: None,
            hits: Hits::default(),
            quit: false,
        }
    }

    /// A picker over what an endpoint answered with, as `Job::Models` builds it.
    fn served_models() -> ModelPicker {
        let mut big = Model::new("gpt-5.5");
        big.context_limit = Some(400_000);
        big.output_limit = Some(128_000);

        ModelPicker::new(
            "primary".into(),
            vec![big, Model::new("gpt-4o-mini"), Model::new("claude-opus-4-8")],
            Some("gpt-5.5".into()),
        )
    }

    /// Render one frame and return it as text, so a failure shows the screen.
    fn screen(app: &mut App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend");
        terminal
            .draw(|frame| {
                app.render(frame);
            })
            .expect("draw");

        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_main_view_draws_the_brand_and_both_panes() {
        let text = screen(&mut scene(), 120, 30);

        assert!(text.contains(brand::VENDOR), "no vendor mark:\n{text}");
        assert!(text.contains("primary"), "providers missing:\n{text}");
        assert!(text.contains("Codex") && text.contains("opencode"), "agents missing:\n{text}");
        // A masked key, never the key itself.
        assert!(!text.contains("sk-example-key-0001"), "secret rendered in full:\n{text}");
    }

    #[test]
    fn every_overlay_draws_without_panicking() {
        for overlay in [
            Overlay::Detail(Detail { scroll: 0, catalog: None }),
            Overlay::About { scroll: 0 },
            Overlay::Palette(CommandPalette::new(&scene())),
            Overlay::Models(served_models()),
            Overlay::Confirm(Confirm {
                prompt: "delete primary?".into(),
                agent_id: "codex".into(),
                provider_id: "primary".into(),
            }),
        ] {
            let mut app = scene();
            app.overlay = Some(overlay);
            let text = screen(&mut app, 120, 30);
            assert!(!text.trim().is_empty(), "overlay drew nothing");
        }
    }

    #[test]
    fn the_model_picker_shows_limits_and_marks_the_one_in_use() {
        let mut app = scene();
        app.overlay = Some(Overlay::Models(served_models()));
        let text = screen(&mut app, 120, 30);

        assert!(text.contains("gpt-5.5"), "models missing:\n{text}");
        assert!(text.contains("primary"), "endpoint not named in the title:\n{text}");
        // Limits rounded the way the CLI's model table rounds them.
        assert!(text.contains("400K") && text.contains("128K"), "limits missing:\n{text}");
        // The model already in use carries the mark.
        let marked = text
            .lines()
            .find(|line| line.contains("gpt-5.5") && !line.contains("mini"))
            .expect("no row for the current model");
        assert!(marked.contains(brand::MARK), "current model unmarked: {marked:?}");
    }

    #[test]
    fn the_provider_detail_names_the_agents_current_model() {
        let mut app = scene();
        app.overlay = Some(Overlay::Detail(Detail { scroll: 0, catalog: None }));
        let text = screen(&mut app, 120, 30);

        assert!(text.contains("agent model"), "no model row in the detail:\n{text}");
        assert!(text.contains("gpt-5.5"), "the model itself is missing:\n{text}");
    }

    #[test]
    fn typing_in_the_model_picker_narrows_it_to_what_matches() {
        let mut picker = served_models();
        picker.query = "opus".into();
        picker.rebuild();

        let mut app = scene();
        app.overlay = Some(Overlay::Models(picker));
        let text = screen(&mut app, 120, 30);

        assert!(text.contains("claude-opus-4-8"), "the match is missing:\n{text}");
        assert!(!text.contains("gpt-4o-mini"), "a non-match survived the filter:\n{text}");
    }

    #[test]
    fn the_hint_bar_drops_whole_hints_rather_than_slicing_one() {
        for width in [50, 70, 90, 118, 160] {
            let text = screen(&mut scene(), width, 24);
            let hints = text.lines().last().expect("no hint bar");

            assert!(
                !hints.trim_end().ends_with('·'),
                "hint bar ends on a dangling separator at {width} columns: {hints:?}"
            );
            assert!(
                hints.chars().count() <= width as usize,
                "hint bar overflows at {width} columns: {hints:?}"
            );
            // A key sliced in half would leave its label orphaned or truncated.
            assert!(
                !hints.ends_with('…') && !hints.ends_with(' '),
                "hint bar looks cut at {width} columns: {hints:?}"
            );
        }
    }

    #[test]
    fn a_cramped_terminal_still_draws() {
        // Narrow and short enough to force every compaction path.
        for (width, height) in [(60, 12), (80, 20), (200, 60)] {
            let text = screen(&mut scene(), width, height);
            assert!(!text.trim().is_empty(), "{width}x{height} drew nothing");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Model;

    fn provider(id: &str, url: Option<&str>, models: &[&str]) -> Provider {
        let mut provider = Provider::new(id);
        provider.base_url = url.map(str::to_owned);
        provider.models = models.iter().map(|id| Model::new(*id)).collect();
        provider
    }

    fn filtered(providers: &[Provider], query: &str) -> Vec<String> {
        let filter = Filter { query: query.to_string(), editing: false };
        providers.iter().filter(|p| filter.matches(p)).map(|p| p.id.clone()).collect()
    }

    fn sample() -> Vec<Provider> {
        vec![
            provider("byesu", Some("https://byesu.com/v1"), &["gpt-5.5"]),
            provider("local", Some("http://127.0.0.1:1337/v1"), &["qwen3-coder"]),
            provider("bare", None, &[]),
        ]
    }

    #[test]
    fn cursor_stops_at_both_ends() {
        let mut cursor = Cursor::default();
        cursor.step(-1, 3);
        assert_eq!(cursor.index(), 0);

        cursor.step(1, 3);
        cursor.step(1, 3);
        cursor.step(1, 3);
        assert_eq!(cursor.index(), 2);
    }

    #[test]
    fn cursor_on_an_empty_list_stays_at_zero() {
        let mut cursor = Cursor { index: 5 };
        cursor.step(1, 0);
        assert_eq!(cursor.index(), 0);

        let mut cursor = Cursor { index: 5 };
        cursor.clamp(0);
        assert_eq!(cursor.index(), 0);
    }

    #[test]
    fn cursor_follows_a_list_that_shrank() {
        let mut cursor = Cursor { index: 7 };
        cursor.clamp(3);
        assert_eq!(cursor.index(), 2);

        cursor.clamp(9);
        assert_eq!(cursor.index(), 2);
    }

    #[test]
    fn panes_toggle() {
        assert_eq!(Pane::Agents.other(), Pane::Providers);
        assert_eq!(Pane::Providers.other().other(), Pane::Providers);
    }

    #[test]
    fn an_empty_filter_hides_nothing() {
        let providers = sample();
        assert_eq!(filtered(&providers, "").len(), 3);
        assert_eq!(filtered(&providers, "   ").len(), 3);
        assert!(!Filter::default().active());
    }

    #[test]
    fn a_filter_matches_the_id_the_host_and_the_models() {
        let providers = sample();
        assert_eq!(filtered(&providers, "byesu"), vec!["byesu"]);
        assert_eq!(filtered(&providers, "127.0.0.1"), vec!["local"]);
        assert_eq!(filtered(&providers, "1337"), vec!["local"]);
        assert_eq!(filtered(&providers, "qwen"), vec!["local"]);
    }

    #[test]
    fn a_filter_ignores_case_on_both_sides() {
        let providers = vec![provider("ByEsU", Some("https://BYESU.com/v1"), &["GPT-5.5"])];
        assert_eq!(filtered(&providers, "byesu").len(), 1);
        assert_eq!(filtered(&providers, "BYESU").len(), 1);
        assert_eq!(filtered(&providers, "gpt-5.5").len(), 1);
    }

    #[test]
    fn a_filter_that_matches_nothing_returns_nothing() {
        assert!(filtered(&sample(), "no-such-thing").is_empty());
    }

    #[test]
    fn a_filter_that_shrinks_the_list_leaves_the_cursor_on_a_real_row() {
        let providers = sample();
        let mut cursor = Cursor { index: 2 };

        let visible = filtered(&providers, "byesu");
        cursor.clamp(visible.len());
        assert_eq!(cursor.index(), 0);
        assert!(cursor.index() < visible.len());

        cursor.clamp(filtered(&providers, "").len());
        assert_eq!(cursor.index(), 0);
    }

    #[test]
    fn a_filter_that_empties_the_list_parks_the_cursor_at_zero() {
        let mut cursor = Cursor { index: 2 };
        cursor.clamp(filtered(&sample(), "nope").len());
        assert_eq!(cursor.index(), 0);
    }

    #[test]
    fn health_is_keyed_by_agent_as_well_as_provider() {
        let mut cache = HealthCache::default();
        cache.record("codex", "byesu", Health { alive: true, millis: 42 });
        cache.record("claude", "byesu", Health { alive: false, millis: 900 });

        assert_eq!(cache.get("codex", "byesu").unwrap().millis, 42);
        assert!(!cache.get("claude", "byesu").unwrap().alive);
        assert!(cache.get("opencode", "byesu").is_none());
        assert!(cache.get("codex", "other").is_none());
    }

    #[test]
    fn editing_a_provider_forgets_only_its_own_verdict() {
        let mut cache = HealthCache::default();
        cache.record("codex", "byesu", Health { alive: true, millis: 42 });
        cache.record("codex", "local", Health { alive: true, millis: 3 });
        cache.record("claude", "byesu", Health { alive: true, millis: 7 });

        cache.forget("codex", "byesu");
        assert!(cache.get("codex", "byesu").is_none());
        assert!(cache.get("codex", "local").is_some());
        assert!(cache.get("claude", "byesu").is_some());
    }

    #[test]
    fn fit_pads_and_marks_truncation() {
        assert_eq!(fit("ab", 5), "ab   ");
        assert_eq!(fit("abcdef", 4), "abc…");
        assert_eq!(fit("abc", 3), "abc");
        assert_eq!(fit("abc", 1), "…");
        assert_eq!(fit("abc", 0), "");
    }

    #[test]
    fn fit_counts_characters_rather_than_bytes() {
        assert_eq!(fit("héllo", 5), "héllo");
        assert_eq!(fit("héllo", 3), "hé…");
    }

    #[test]
    fn columns_line_up_to_the_declared_widths() {
        let columns = Columns { widths: vec![3, 4] };
        assert_eq!(columns.row(&["a", "bb"]), "a   bb   ");
    }

    #[test]
    fn a_flexible_column_absorbs_the_slack_and_the_row_fills_the_width() {
        let columns = Columns::flexible(40, &[1, 6, 4], 2, 5);
        assert_eq!(columns.widths, vec![1, 6, 25, 4]);
        assert_eq!(columns.row(&["a", "b", "c", "d"]).chars().count(), 40);
    }

    #[test]
    fn a_flexible_column_stops_shrinking_at_its_minimum() {
        let columns = Columns::flexible(8, &[1, 6, 4], 2, 5);
        assert_eq!(columns.widths, vec![1, 6, 5, 4]);
    }

    #[test]
    fn long_cells_are_cut_with_one_ellipsis_and_never_wrap() {
        let columns = Columns::flexible(24, &[1, 4], 1, 6);
        let row = columns.row(&["*", "a-very-long-hostname", "x"]);
        assert_eq!(row.chars().count(), 24);
        assert_eq!(row.matches('…').count(), 1);
        assert!(!row.contains('\n'));
    }

    #[test]
    fn limits_read_the_way_people_say_them() {
        assert_eq!(format_limits(Some(1_000_000), Some(128_000)).unwrap(), "1M ctx · 128K out");
        assert_eq!(format_limits(Some(200_000), None).unwrap(), "200K ctx");
        assert_eq!(format_limits(None, Some(8_192)).unwrap(), "8K out");
        assert_eq!(format_limits(Some(1_100_000), None).unwrap(), "1.1M ctx");
        assert_eq!(format_limits(Some(512), None).unwrap(), "512 ctx");
        assert_eq!(format_limits(None, None), None);
    }

    #[test]
    fn the_current_model_is_recognised_bare_or_qualified() {
        // Most agents name the model on its own.
        assert!(is_current_model(Some("gpt-5.5"), "gpt-5.5"));
        // opencode qualifies it with the provider it came from.
        assert!(is_current_model(Some("codexsale/gpt-5.5"), "gpt-5.5"));
        // A model id that itself contains a slash still matches exactly.
        assert!(is_current_model(Some("xiaomi/mimo-v2.5-pro"), "xiaomi/mimo-v2.5-pro"));
    }

    #[test]
    fn a_different_model_is_never_mistaken_for_the_current_one() {
        assert!(!is_current_model(Some("gpt-5.5"), "gpt-4"));
        assert!(!is_current_model(None, "gpt-5.5"));
        // A suffix that is not a whole path segment must not count.
        assert!(!is_current_model(Some("codexsale/not-gpt-5.5"), "gpt-5.5"));
        // Nor may a bare id match a longer one that merely ends the same way.
        assert!(!is_current_model(Some("mini-gpt-5.5"), "gpt-5.5"));
    }

    #[test]
    fn filtering_models_ranks_the_closest_id_first() {
        let models: Vec<Model> = ["gpt-5.5", "gpt-4o-mini", "claude-opus-4-8", "grok-3"]
            .iter()
            .map(|id| Model::new(*id))
            .collect();
        let ids = |query: &str| -> Vec<String> {
            rank_by(&models, query, |model| model.id.clone())
                .iter()
                .map(|index| models[*index].id.clone())
                .collect()
        };

        assert_eq!(ids("gpt5").first().unwrap(), "gpt-5.5");
        assert_eq!(ids("opus"), vec!["claude-opus-4-8"]);
        // A subsequence, not a substring: scattered letters still find it.
        assert!(ids("gk").contains(&"grok-3".to_string()));
        assert!(ids("zzz").is_empty());
        // An empty query keeps every model, in the order the endpoint listed them.
        assert_eq!(ids("").len(), 4);
    }

    #[test]
    fn cyrillic_keys_reach_the_binding_on_the_same_physical_key() {
        assert_eq!(normalise_key('й'), 'q');
        assert_eq!(normalise_key('ы'), 's');
        assert_eq!(normalise_key('ф'), 'a');
        assert_eq!(normalise_key('ю'), '.');
    }

    #[test]
    fn a_shifted_cyrillic_key_still_means_the_shifted_binding() {
        assert_eq!(normalise_key('Ы'), 'S');
        assert_eq!(normalise_key('С'), 'C');
        assert_eq!(normalise_key('Й'), 'Q');
        // A Latin target with no upper case keeps the character it has.
        assert_eq!(normalise_key('Ж'), ';');
    }

    #[test]
    fn latin_and_unmapped_characters_pass_through_untouched() {
        for ch in ['q', 'S', '/', '?', '1', 'ß', '你'] {
            assert_eq!(normalise_key(ch), ch, "{ch} must not be rewritten");
        }
    }

    #[test]
    fn normalising_leaves_non_character_keys_alone() {
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(normalise_event(enter).code, KeyCode::Enter);

        let chord = KeyEvent::new(KeyCode::Char('а'), KeyModifiers::CONTROL);
        let folded = normalise_event(chord);
        assert_eq!(folded.code, KeyCode::Char('f'));
        assert_eq!(folded.modifiers, KeyModifiers::CONTROL);
    }

    #[test]
    fn a_subsequence_matches_even_when_scattered() {
        // Scattered across word boundaries: u-se p-r-imar-y.
        assert!(subsequence_score("use primary", "upy").is_some());
        assert!(subsequence_score("add provider", "ad").is_some());
        assert_eq!(subsequence_score("add provider", ""), Some(0));
    }

    #[test]
    fn a_subsequence_out_of_order_or_missing_does_not_match() {
        assert_eq!(subsequence_score("use primary", "zz"), None);
        assert_eq!(subsequence_score("add", "da"), None);
    }

    #[test]
    fn subsequence_matching_ignores_case() {
        assert_eq!(
            subsequence_score("Use Primary", "upy"),
            subsequence_score("use primary", "UPY")
        );
        assert!(subsequence_score("QUIT", "qt").is_some());
    }

    #[test]
    fn earlier_and_tighter_matches_score_better() {
        let tight = subsequence_score("add provider", "ad").unwrap();
        let scattered = subsequence_score("apply a preset delete", "ad").unwrap();
        assert!(tight < scattered, "{tight} should beat {scattered}");

        let early = subsequence_score("check all", "ch").unwrap();
        let late = subsequence_score("reload from disk", "ch").unwrap_or(u32::MAX);
        assert!(early < late);
    }

    #[test]
    fn ranking_puts_the_best_match_first_and_keeps_ties_in_order() {
        let entries = vec![
            Action::Reload,
            Action::Delete,
            Action::Detail,
            Action::Use(Some("vendor".into())),
        ];

        let ranked: Vec<String> =
            rank_by(&entries, "de", Action::label).iter().map(|i| entries[*i].label()).collect();
        assert_eq!(ranked.first().unwrap(), "delete provider");
        assert!(ranked.contains(&"provider detail".to_string()));
        assert!(!ranked.contains(&"reload from disk".to_string()));

        // With no query every entry survives, in catalogue order.
        assert_eq!(rank_by(&entries, "", Action::label), vec![0, 1, 2, 3]);
    }

    #[test]
    fn a_click_resolves_to_the_row_under_it() {
        let rows = RowsAt { area: Rect { x: 2, y: 5, width: 20, height: 4 }, offset: 0 };
        assert_eq!(rows.row(Position::new(3, 5), 10), Some(0));
        assert_eq!(rows.row(Position::new(3, 7), 10), Some(2));

        // Outside the area entirely.
        assert_eq!(rows.row(Position::new(1, 5), 10), None);
        assert_eq!(rows.row(Position::new(3, 4), 10), None);
        assert_eq!(rows.row(Position::new(3, 9), 10), None);

        // Inside the area but past the end of the list.
        assert_eq!(rows.row(Position::new(3, 7), 2), None);
    }

    #[test]
    fn a_click_on_a_scrolled_list_accounts_for_the_offset() {
        let rows = RowsAt { area: Rect { x: 0, y: 0, width: 10, height: 3 }, offset: 7 };
        assert_eq!(rows.row(Position::new(0, 0), 20), Some(7));
        assert_eq!(rows.row(Position::new(0, 2), 20), Some(9));
        assert_eq!(rows.row(Position::new(0, 2), 8), None);
    }

    #[test]
    fn an_empty_hitbox_swallows_nothing() {
        let rows = RowsAt::default();
        assert_eq!(rows.row(Position::new(0, 0), 5), None);
    }

    #[test]
    fn every_direct_binding_is_documented_on_the_help_screen() {
        let bindings: Vec<&str> = menu().iter().filter_map(|action| action.binding()).collect();
        for expected in [
            "enter",
            "/ or ctrl+f",
            "u",
            "m",
            "a",
            "e",
            "d",
            "c",
            "C",
            "s",
            "S",
            "p",
            "r",
            "ctrl+p or ctrl+k",
            "?",
            "q",
        ] {
            assert!(bindings.contains(&expected), "{expected} is bound but undocumented");
        }
    }

    #[test]
    fn every_action_has_something_to_show_in_the_palette() {
        for action in menu() {
            assert!(!action.label().is_empty());
            assert!(!action.description().is_empty());
            assert!(!action.hint().is_empty());
        }
    }

    #[test]
    fn adding_needs_a_usable_id() {
        let mut form = Form::add("codex");
        assert!(form.provider().is_err(), "an empty id must be rejected");

        form.activate();
        form.push('b');
        form.push('a');
        form.push('d');
        form.push(' ');
        form.push('i');
        form.commit();
        let err = form.provider().unwrap_err().to_string();
        assert!(err.contains("may only contain"), "{err}");
    }

    #[test]
    fn a_completed_add_form_becomes_a_provider() {
        let mut form = Form::add("codex");
        form.activate();
        form.buffer = "byesu".into();
        form.commit();

        form.cursor.step(1, form.fields.len());
        form.activate();
        form.buffer = " https://byesu.com/v1 ".into();
        form.commit();

        let provider = form.provider().unwrap();
        assert_eq!(provider.id, "byesu");
        assert_eq!(provider.base_url.as_deref(), Some("https://byesu.com/v1"));
        assert_eq!(provider.api_key, None);
        assert_eq!(provider.wire_api, None);
    }

    #[test]
    fn editing_keeps_the_original_id_out_of_the_fields() {
        let mut source = Provider::new("byesu");
        source.base_url = Some("https://byesu.com/v1".into());
        source.api_key = Some("sk-secret-value".into());
        source.wire_api = Some(WireApi::Responses);

        let form = Form::edit("codex", &source);
        assert!(form.fields.iter().all(|(kind, _)| *kind != FieldKind::Id));
        assert_eq!(form.value(FieldKind::WireApi), "responses");

        let provider = form.provider().unwrap();
        assert_eq!(provider.id, "byesu");
        assert_eq!(provider.api_key.as_deref(), Some("sk-secret-value"));
        assert_eq!(provider.wire_api, Some(WireApi::Responses));
    }

    #[test]
    fn the_wire_api_field_cycles_rather_than_taking_text() {
        let mut form = Form::add("codex");
        form.cursor = Cursor { index: form.fields.len() - 1 };
        assert_eq!(form.fields[form.cursor.index()].0, FieldKind::WireApi);

        for expected in ["chat", "responses", "anthropic", ""] {
            form.activate();
            assert!(!form.editing, "a choice field must never start text entry");
            assert_eq!(form.value(FieldKind::WireApi), expected);
        }
    }

    #[test]
    fn cancelling_text_entry_leaves_the_field_untouched() {
        let mut source = Provider::new("byesu");
        source.base_url = Some("https://old/v1".into());
        let mut form = Form::edit("codex", &source);

        form.activate();
        assert!(form.editing);
        form.buffer = "https://new/v1".into();
        form.cancel_entry();

        assert!(!form.editing);
        assert_eq!(form.value(FieldKind::BaseUrl), "https://old/v1");
    }

    #[test]
    fn blank_fields_are_absent_rather_than_empty_strings() {
        assert_eq!(optional("  "), None);
        assert_eq!(optional(" x "), Some("x".to_string()));
    }

    #[test]
    fn centred_boxes_never_leave_the_screen() {
        let screen = Rect { x: 0, y: 0, width: 20, height: 6 };
        let small = centered_fixed(screen, 64, 40);
        assert_eq!(small.width, 20);
        assert_eq!(small.height, 6);

        let inside = centered(screen, 70, 70);
        assert!(inside.x + inside.width <= screen.width);
        assert!(inside.y + inside.height <= screen.height);
    }
}
