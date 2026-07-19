<p align="center">
  <img src="../assets/logo.svg" alt="ConfAI — ein Editor für die Konfiguration jedes KI-Agenten" width="720">
</p>

<p align="center">
  <a href="https://github.com/redstone-md/ConfAI/actions/workflows/ci.yml"><img src="https://github.com/redstone-md/ConfAI/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <img src="https://img.shields.io/badge/rust-1.88%2B-b8352d" alt="Rust 1.88+">
  <img src="https://img.shields.io/badge/licence-MIT-b8352d" alt="MIT">
</p>

<p align="center">
  <a href="https://redstone.md">redstone.md</a> ·
  <a href="https://github.com/redstone-md/ConfAI">Quellcode</a> ·
  <a href="../CONTRIBUTING.md">mitmachen</a> ·
  <a href="../LICENSE">MIT</a>
</p>

<p align="center">
  <a href="../README.md">English</a> ·
  <a href="README.ru.md">Русский</a> ·
  <a href="README.zh-CN.md">简体中文</a> ·
  <a href="README.es.md">Español</a> ·
  <b>Deutsch</b> ·
  <a href="README.ja.md">日本語</a>
</p>

---

Codex, Claude Code und opencode legen ihre Endpunkte jeweils in einer anderen
Datei ab, in einem anderen Format und unter einem anderen Namen für dieselbe
Sache. Bei den MCP-Servern, die sie starten, und den Skills, die sie laden, ist
es genauso. Einen Anbieter hinzuzufügen oder zwischen zweien zu wechseln heißt,
drei Dateien von Hand zu öffnen. ConfAI erledigt das mit einem Befehl — und
formatiert nie um, was es nicht geändert hat.

## Installation

Linux und macOS:

```sh
curl -fsSL https://github.com/redstone-md/ConfAI/releases/latest/download/install.sh | sh
```

Windows:

```powershell
irm https://github.com/redstone-md/ConfAI/releases/latest/download/install.ps1 | iex
```

Beide Skripte ermitteln deine Plattform, laden das passende Release-Archiv und
`SHA256SUMS` herunter, prüfen die Prüfsumme und legen erst danach die Binärdatei
ab. Ein Skript aus dem Internet in eine Shell zu pipen ist eine
Vertrauensentscheidung; [INSTALL.md](../INSTALL.md) zeigt, wie man es vorher
liest.

Über cargo:

```sh
cargo install confai --locked    # baut aus dem Quellcode, braucht Rust 1.88+
cargo binstall confai            # lädt stattdessen das Release-Archiv
```

Oder von Hand: das Archiv für deine Plattform von der
[Release-Seite](https://github.com/redstone-md/ConfAI/releases/latest) holen,
gegen die daneben veröffentlichte `SHA256SUMS` prüfen und die Binärdatei in den
`PATH` legen.

Der Rest steht in [INSTALL.md](../INSTALL.md): alle Plattformen, die Optionen der
Installer, wie mit dem `PATH` umgegangen wird und wie man wieder deinstalliert.

## Was es tut

```
$ confai list
agent        detected         providers  active  model          config
Codex        binary + config  3          primary gpt-5.6-terra  ~/.codex/config.toml
Claude Code  binary + config  1          byesu   opus[1m]       ~/.claude/settings.json
opencode     binary + config  11         vendor              ~/.config/opencode/opencode.json
```

- Ein Befehl schaltet jeden Agenten um, der diesen Endpunkt hat:
  `confai provider use primary`.
- Ein Preset schreibt denselben Endpunkt in alle:
  `confai preset apply byesu --all --use`.
- `confai provider sync` trägt die Modelle nach, die ein Endpunkt tatsächlich
  ausliefert, samt Kontext- und Ausgabegrenzen.
- `confai mcp list` und `confai skill list` tun dasselbe für die MCP-Server, die
  ein Agent startet, und die Skills, die er lädt.
- Kommentare, Schlüsselreihenfolge und unbekannte Schlüssel überstehen eine
  Bearbeitung. Vor jedem Schreiben wird gesichert, und `confai undo` stellt es
  wieder her.

`confai` ohne Argumente öffnet einen Browser mit zwei Spalten: links die
Agenten, rechts die Endpunkte des gewählten Agenten.

<p align="center">
  <img src="../assets/screenshots/tui.png" alt="Die interaktive Ansicht von ConfAI: links Agenten, rechts Endpunkte" width="900">
</p>

<details>
<summary><b>Die Kommandozeile</b> — alle Unterbefehle und Optionen</summary>

```sh
confai                                    # interaktive Ansicht
confai list                               # was installiert ist, und wo
confai provider list --check              # alle Endpunkte, und ob sie antworten
confai provider add byesu \
    --agent codex \
    --base-url https://byesu.com/v1 \
    --api-key "$BYESU_API_KEY" \
    --wire-api chat --use
confai provider use primary               # jeden Agenten umschalten, der ihn hat
confai provider sync vendor --prune       # die Modellliste vom Endpunkt holen
confai preset apply byesu --all --use     # ein Endpunkt, alle Agenten
confai doctor                             # lässt sich noch alles parsen und auflösen
confai undo                               # zurück auf den vorherigen Stand
```

`--agent` zielt auf einen Agenten, `--all` auf jeden installierten. Ohne beides
decken lesende Befehle alles ab, und schreibende fragen nach.

| Befehl | |
|---|---|
| `list` | welche Agenten installiert sind und worauf sie zeigen |
| `provider list` | Endpunkte der gewählten Agenten; `--check` ruft jeden auf |
| `provider add <id>` | Endpunkt anlegen oder an einem vorhandenen die übergebenen Felder ändern |
| `provider remove <id>` | Endpunkt entfernen |
| `provider use <id>` | einen Agenten über einen seiner Endpunkte leiten |
| `provider check [id]` | Endpunkte fragen, ob sie erreichbar sind und was sie ausliefern |
| `provider models [id]` | was ein Endpunkt ausliefert, mit Grenzen und Preisen |
| `provider sync <id>` | die Modellliste in die Konfiguration schreiben |
| `preset list` · `preset show <id>` | welche Rezepte es gibt, und was eines schreiben würde |
| `preset apply <id>` | den Endpunkt eines Presets in die gewählten Agenten schreiben |
| `mcp list` · `mcp doctor` | die MCP-Server jedes Agenten · ob jeder starten könnte |
| `mcp add <name>` · `mcp remove <name>` | Server anlegen oder ändern · Server entfernen |
| `mcp toggle <name>` | einen Server abschalten, ohne ihn zu entfernen, wo der Agent das zulässt |
| `mcp preset list` · `mcp preset apply <id>` | fertige Server-Rezepte, und eines anwenden |
| `skill list` · `skill path` | die Skills jedes Agenten · wo er sie ablegt |
| `skill doctor` | Skills, die der Agent stillschweigend ignoriert, und warum |
| `skill copy <name>` · `skill remove <name>` | einen Skill zwischen Agenten kopieren · einen löschen |
| `model [model]` | das Modell eines Agenten anzeigen oder setzen |
| `path` · `edit` | Pfad der Konfiguration ausgeben · sie in `$EDITOR` öffnen |
| `doctor` | prüfen, ob jede Konfiguration parst und jeder referenzierte Anbieter auflösbar ist |
| `about` · `update` | Version und Ablageorte · ob ein neueres Release existiert |
| `undo` | die vor dem letzten Schreiben gesicherte Konfiguration zurückholen |

`provider add` nimmt `--base-url`, `--api-key`, `--wire-api` (`chat`,
`responses` oder `anthropic`), `--name`, ein wiederholbares
`--set SCHLÜSSEL=WERT` für backend-spezifische Schlüssel sowie `--use` /
`--sync`, um den Endpunkt nach dem Schreiben auszuwählen und seine Modelle zu
holen. `provider check` nimmt `--timeout` in Sekunden, standardmäßig 10.
`provider models` nimmt `--select <model>` und `--refresh`. `provider sync` nimmt
`--prune`, `--dry-run` und `--refresh`. `preset apply` nimmt `--api-key`,
`--use` und `--sync`.

`list`, `doctor`, `about`, `update`, `preset list` und `mcp preset list` nehmen
keine Agentenauswahl — sie decken immer alles ab. `skill copy` und `skill remove`
haben ihre eigene: `--from` / `--to` und ein verpflichtendes `--agent`.

</details>

<details>
<summary><b>MCP-Server</b> — eine Liste, drei verschiedene Konfigurationsformen</summary>

Jeder Agent startet seinen eigenen Satz MCP-Server, und alle drei halten sie
unterschiedlich und an unterschiedlichen Orten fest. Codex legt sie unter
`mcp_servers` in seinem TOML ab, als Kommando plus separate `args`-Liste. Claude
Code legt sie unter `mcpServers` in `~/.claude.json` ab — einer dritten Datei,
nicht `settings.json`. opencode legt sie unter `mcp` in seiner Konfiguration ab,
wo das Kommando eine einzige Liste ist statt Programm und Argumente, der
Umgebungsblock `environment` statt `env` heißt und ein Server abgeschaltet werden
kann, ohne gelöscht zu werden.

```sh
confai mcp list --all
confai mcp add context7 --agent claude --command npx --arg -y --arg @upstash/context7-mcp
confai mcp add sentry --agent opencode --url https://mcp.example.com/mcp
confai mcp toggle playwright --off
confai mcp remove playwright --agent codex
confai mcp doctor --all
confai mcp preset list
confai mcp preset apply github --all
```

`mcp add` nimmt `--command` zusammen mit einem wiederholbaren, die Reihenfolge
bewahrenden `--arg` für einen stdio-Server, oder `--url` für einen entfernten,
dazu ein wiederholbares `--env SCHLÜSSEL=WERT`. `mcp doctor` nimmt `--timeout` in
Sekunden, standardmäßig 10. `mcp preset apply` nimmt `--name`, um den Server
unter etwas anderem als der Preset-id einzutragen.

**`confai mcp doctor` startet nichts.** Bei einem stdio-Server löst es die
ausführbare Datei über den `PATH` auf, bei einem entfernten ruft es den Endpunkt
auf. Ein beliebiges konfiguriertes Kommando auszuführen, um zu sehen was
passiert, ist keine Diagnose, sondern das Ausführen dessen, was in der
Konfiguration steht. Ein Starter wie `npx` wird als das gemeldet, was er ist —
das Paket dahinter lässt sich nicht prüfen, ohne es zu holen.

`~/.claude.json` enthält den laufenden Sitzungszustand, und Claude Code schreibt
fortlaufend hinein. ConfAI schreibt die Datei deshalb nur dann neu, wenn eine
MCP-Änderung tatsächlich etwas geändert hat, statt ohne Grund mit dem Agenten um
sie zu konkurrieren.

`mcp toggle` funktioniert dort, wo der Agent diesen Zustand irgendwo festhalten
kann — heute heißt das opencode. Codex und Claude Code haben kein Kennzeichen
zum Abschalten; ConfAI sagt das und verweist darauf, den Server zu entfernen,
statt so zu tun als ginge es.

Die neun mitgelieferten Rezepte liegen in [`presets/mcp/`](../presets/mcp/):
continuum, context7, playwright, github, git, fetch, filesystem, memory und
sequential-thinking. Eigene kommen nach `~/.confai/presets/mcp/`.

</details>

<details>
<summary><b>Skills</b> — was jeder Agent hat, und was er stillschweigend übergeht</summary>

Ein Skill ist ein Verzeichnis mit einer `SKILL.md` darin, das der Agent findet,
indem er danach sucht. Claude Code und opencode arbeiten beide so, in einem
`skills/` neben ihrer Konfiguration. Codex hat überhaupt keine Skills — seine
Plugins sind ein eigener Mechanismus — und ConfAI sagt das, statt ihm ein
Verzeichnis anzudichten.

```sh
confai skill list --all
confai skill path --all
confai skill doctor --all
confai skill copy context7 --from claude --to opencode
confai skill remove context7 --agent opencode
```

`skill copy` braucht `--from`; lässt du `--to` weg, wird der Skill in jeden
anderen Agenten installiert, der Skills führt, und `--force` ersetzt einen, den
das Ziel bereits unter dem Namen hat. Es gibt den Befehl, weil derselbe Skill für
mehrere Agenten nützlich ist und es keinen gemeinsamen Ort dafür gibt.

`skill doctor` meldet, auf welche Weisen ein Skill am Ende von niemandem geladen
wird, ohne dass irgendwer etwas dazu sagt: ein Verzeichnis ohne lesbare
`SKILL.md`, ein Front Matter ohne `description`, an der der Agent erkennen
könnte, wann er den Skill braucht, oder ein `name` im Front Matter, der nicht zum
Verzeichnisnamen passt — Agenten sprechen einen Skill über sein Verzeichnis an.

`skill remove` verlangt `--agent` und löscht ein Verzeichnis. Für ein Verzeichnis
gibt es keine Sicherung wie für eine Konfigurationsdatei, **`confai undo` holt es
also nicht zurück.** Vor dem Löschen gibt der Befehl den Pfad aus, den er
entfernen wird.

</details>

<details>
<summary><b>Die interaktive Ansicht</b> — Befehlspalette, Detailansicht, vollständige Tastenbelegung</summary>

Die Befehlspalette auf `Ctrl+P` listet jede Aktion mit der Taste, die sie
ausführt — die Kürzel lernt man also im Gebrauch und nicht auf dieser Seite:

<p align="center">
  <img src="../assets/screenshots/palette.png" alt="Die Befehlspalette mit jeder Aktion und ihrer Tastenbelegung" width="900">
</p>

`Enter` auf einem Endpunkt zeigt alles, was über ihn festgehalten ist, samt der
Modellliste mit Kontext- und Ausgabegrenzen:

<p align="center">
  <img src="../assets/screenshots/detail.png" alt="Die Detailansicht eines Anbieters mit den Feldern des Endpunkts und seiner Modellliste" width="900">
</p>

`v` schaltet die rechte Spalte reihum durch drei Ansichten: Anbieter →
MCP-Server → Skills → Anbieter. Eine Ansicht, die der Agent nicht hat, wird
übersprungen; bei Codex geht es also nur zwischen Anbietern und MCP-Servern hin
und her, weil Codex keine Skills führt.

Diese gelten in jeder Ansicht:

| Taste | |
|---|---|
| `Ctrl+P` / `Ctrl+K` | Befehlspalette — jede Aktion, durchsuchbar |
| `↑` `↓` / `k` `j` | bewegen · `Tab` `←` `→` Spalte wechseln |
| `v` | die rechte Spalte durch die Ansichten schalten |
| `/` oder `Ctrl+F` | die Liste nach id, Host oder Modell filtern |
| `m` | wählen, welches Modell dieser Agent benutzt |
| `s` / `S` | Modelle synchronisieren · synchronisieren und veraltete entfernen |
| `?` | Info und die vollständige Tastenbelegung |
| `r` `q` | von der Platte neu laden · beenden |

Der Rest wirkt auf das, was die rechte Spalte gerade zeigt:

| Taste | Anbieter | MCP-Server | Skills |
|---|---|---|---|
| `Enter` | Details | Details | Details |
| `u` | den Agenten darüber leiten | ein- oder ausschalten | — |
| `a` | hinzufügen | hinzufügen | — |
| `e` | bearbeiten | bearbeiten | — |
| `d` | löschen | löschen | löschen, nach Rückfrage |
| `c` / `C` | prüfen · alle prüfen | prüfen · alle prüfen | — |
| `p` | ein Preset anwenden | einen MCP-Server finden | — |
| `g` | einen MCP-Server finden | einen MCP-Server finden | einen MCP-Server finden |
| `y` | — | — | in einen anderen Agenten kopieren |

**`p` und `g` öffnen dasselbe Panel.** Es durchsucht beide Quellen auf einmal:
zuerst die neun eingebauten Rezepte, mit Stern als empfohlen markiert, danach
das offizielle Registry. Tippen filtert das bereits Gelistete; `Strg+R` fragt
das Registry — die einzige Taste, die ins Netz geht.

**Für Skills gibt es kein Hinzufügen und kein Bearbeiten.** Einen Skill schreibt
man nicht in einer Listenansicht; dafür ist ein Texteditor da. Diese Tasten tun
in der Skills-Ansicht schlicht nichts, statt dir einen Fehler zu melden.

**Einen Skill zu löschen ist das einzige Unumkehrbare, das ConfAI tut.** Jedes
andere Löschen schreibt eine Konfigurationsdatei neu, und `confai undo` holt sie
aus der zuvor angelegten Sicherung zurück. Ein Skill ist ein Verzeichnis, und von
einem Verzeichnis gibt es keine Sicherung — die Rückfrage sagt genau das, bevor
irgendetwas entfernt wird.

Die Maus funktioniert: klicken wählt aus, nochmal klicken öffnet, das Rad
scrollt, ein Klick auf einen Hinweis führt ihn aus.

Tasten werden nach ihrer physischen Position erkannt und funktionieren deshalb
auch auf einem nicht-lateinischen Layout weiter — `й` ist `q`, `Ы` ist `S`. `/`
hat auf einem kyrillischen Layout keine entsprechende Position, weshalb auch
`Ctrl+F` den Filter öffnet.

Änderungen laufen über denselben Laden-Ändern-Speichern-Pfad wie in der CLI, es
gelten also dieselben Zusagen für deine Dateien.

</details>

<details>
<summary><b>Was es deinen Dateien nicht antut</b> — Kommentare, Schlüsselreihenfolge, Sicherungen</summary>

Konfigurationen werden von Hand geschrieben, und in von Hand geschriebenen
Dateien steht Zeug, das ein naiver Lese-Schreib-Durchlauf zerstört.

- **Kommentare bleiben.** Codex-Konfigurationen werden über `toml_edit`
  bearbeitet, ein auf einer auskommentierten `base_url` geparkter
  Ersatz-Endpunkt ist danach also immer noch da.
- **Nur was du geändert hast, ändert sich.** Schlüsselreihenfolge, Einrückung und
  unbekannte Schlüssel bleiben unangetastet, weil jedes Backend das geparste
  Dokument an Ort und Stelle bearbeitet, statt seine eigene Vorstellung der Datei
  neu zu serialisieren.
- **JSON mit Kommentaren wird abgelehnt, nicht verstümmelt.** ConfAI müsste sie
  wegwerfen, also hält es an und sagt das.
- **Vor jedem Schreiben wird gesichert**, neben dem Original als
  `<name>.confai.bak`, und die Datei wird atomar ersetzt. `confai undo` stellt
  sie wieder her.

</details>

<details>
<summary><b>Modelle und Erreichbarkeit</b> — woher die Modellliste kommt, und was --prune tut</summary>

opencode bietet kein Modell an, von dem man ihm nicht erzählt hat, und es will
die Kontextgrenze ausgeschrieben haben. `confai provider sync <id>` ruft
`/v1/models` des Endpunkts auf, schlägt jede id auf
[models.dev](https://models.dev) für Kontext- und Ausgabegrenzen nach und
schreibt das Ergebnis — `variants` und alles andere, was du konfiguriert hast,
bleibt unberührt. Der Katalog wird einen Tag zwischengespeichert; `--refresh`
lädt ihn neu.

Synchronisieren ist ein Zusammenführen, ein vom Gateway inzwischen abgeschaltetes
Modell bleibt also in deiner Konfiguration, bis du etwas anderes sagst.
`--prune` entfernt die, die der Endpunkt nicht mehr ausliefert, und verschiebt
deine Modellauswahl auf ein verbliebenes, falls das ausgewählte entfernt wurde:

```sh
confai provider sync vendor --prune --dry-run   # sehen, was wegfiele
confai provider sync vendor --prune
```

In der interaktiven Ansicht synchronisiert `s`, und `S` synchronisiert mit
Entfernen.

`confai provider models <id>` listet auf, was ein Endpunkt ausliefert, ohne etwas
zu schreiben, und `--select` macht eines davon zum Modell des Agenten. Das
funktioniert auch für Codex und Claude Code, die ein Modell festhalten, aber
keine Modellliste.

`confai provider check` ist derselbe Aufruf ohne das Schreiben: er meldet, ob
jeder Endpunkt läuft, wie schnell er geantwortet hat und wie viele Modelle er
ausliefert.

</details>

<details>
<summary><b>Presets</b> — ein Endpunkt-Rezept für jeden Agenten</summary>

Ein Preset ist ein Endpunkt, einmal in agentenneutralen Begriffen beschrieben,
damit dasselbe Rezept für jeden Agenten gilt:

```sh
confai preset list
confai preset show byesu
confai preset apply byesu --all --api-key sk-... --use --sync
```

Die sechsundzwanzig mitgelieferten Presets liegen in [`presets/`](../presets/) —
je eine TOML-Datei, beim Bauen fest in die Binärdatei eingebacken — und decken
OpenCode Zen, OpenRouter, OpenAI, Anthropic, Groq, xAI, Mistral, Cerebras,
Together, DeepSeek, DeepInfra, Fireworks, Moonshot, Z.ai, Chutes, Baseten,
Vercel AI Gateway, Venice, Novita, Byesu, Ollama und LM Studio ab. Eines
hinzuzufügen ist ein Pull Request, der genau eine neue Datei anfasst. Eigene
Presets kommen nach `~/.confai/presets/` und überschreiben ein mitgeliefertes mit
derselben id.

</details>

<details>
<summary><b>Agenten</b> — die drei Konfigurationsformen, und was ConfAI mit jeder macht</summary>

| Agent | Konfiguration | Schlüssel | Benannte Anbieter | Modellliste | Umschalten |
|---|---|---|---|---|---|
| Codex | `~/.codex/config.toml` | dieselbe Datei | ja | nein | `model_provider` |
| Claude Code | `~/.claude/settings.json` | `env`-Block | über ConfAI | nein | `ANTHROPIC_*` |
| opencode | `~/.config/opencode/opencode.json` | `~/.local/share/opencode/auth.json` | ja | ja | `provider/model` |

MCP-Server und Skills liegen wiederum woanders:

| Agent | MCP-Server | Abschaltbar | Skills |
|---|---|---|---|
| Codex | `mcp_servers` in `config.toml` | nein | keine — Plugins sind ein eigener Mechanismus |
| Claude Code | `mcpServers` in `~/.claude.json` | nein | `~/.claude/skills/` |
| opencode | `mcp` in `opencode.json` | ja | `skills/` neben der Konfiguration |

`CODEX_HOME`, `CLAUDE_CONFIG_DIR`, `OPENCODE_CONFIG` und `XDG_CONFIG_HOME` werden
beachtet, genau so, wie die Agenten selbst sie beachten.

Claude Code zeigt über Umgebungsvariablen in seinen Einstellungen auf genau einen
Endpunkt und hat keinen Platz für die Endpunkte, die du gerade nicht benutzt.
ConfAI hält diese Liste in `~/.confai/agents/claude.json` und schreibt nur den
ausgewählten Eintrag in die Datei, die Claude Code gehört.

opencode ist auf zwei Dateien verteilt: Anbieter in `opencode.json`, Schlüssel in
`~/.local/share/opencode/auth.json`, wohin `opencode auth login` sie legt. ConfAI
liest beide, damit eine Erreichbarkeitsprüfung mit der Anmeldeinformation
rausgeht, die opencode wirklich benutzen würde, statt einen falschen 401 zu
melden. Ein neuer Schlüssel wird nach `auth.json` geschrieben; ein Schlüssel, der
bereits direkt in `opencode.json` steht, wird dort aktualisiert, wo er steht —
weil ein Geheimnis stillschweigend von einer Datei in eine andere zu verschieben
seine eigene Art von Überraschung ist. Eine OAuth-Sitzung in `auth.json` wird
angezeigt, aber nie überschrieben: ConfAI sagt dir, dass du
`opencode auth logout` ausführen sollst, statt sie stillschweigend zu beenden.

Einen Agenten hinzuzufügen heißt: eine Datei in `src/agent/`, die `Agent` und
`AgentConfig` implementiert; nichts oberhalb dieser Schicht weiß, mit welchem
Agenten es spricht.

</details>

<details>
<summary><b>Aktuell bleiben</b> — die Update-Prüfung, und wie man sie abschaltet</summary>

`confai update` meldet, ob ein neueres Release existiert, fasst zusammen, was
sich geändert hat, und gibt aus, wie man aktualisiert.

Im Alltag musst du nicht fragen. Nach einem Befehl gibt ConfAI zwei Zeilen auf
stderr aus, wenn ein neueres Release draußen ist:

```
◆ 0.0.1 → 0.0.2 available
  · provider sync now prunes retired models
  · run `confai update` for the rest
```

Dieser Hinweis wird aus einem Cache gezeichnet, der höchstens einmal am Tag
geprüft wird — ein normaler Lauf kostet also nichts. Ist der Cache veraltet, hat
die Prüfung vierhundert Millisekunden Zeit zu antworten, bevor der Lauf aufgibt
und es morgen erneut versucht. Nach einer fehlgeschlagenen Prüfung wird eine
Stunde gewartet, statt bei jedem Aufruf neu zu versuchen.
`CONFAI_NO_UPDATE_CHECK` schaltet das Ganze ab.

ConfAI ersetzt seine eigene Binärdatei nicht. `cargo` und die Installer machen
das bereits ordentlich, und ein Werkzeug, das sich selbst überschreibt, während
es deine Zugangsdaten offen hält, ist ein schlechterer Handel als eine
ausgegebene Zeile.

</details>

<details>
<summary><b>Mitmachen</b> — ein Preset oder einen Agenten hinzufügen</summary>

Ein Preset ist eine neue Datei in `presets/`. Ein neuer Agent ist eine neue Datei
in `src/agent/`, die `Agent` und `AgentConfig` implementiert — die Schichten
darüber bleiben unangetastet. Führe `cargo test` und
`cargo clippy --lib --bins --tests` aus, bevor du einen Pull Request öffnest.
Siehe [CONTRIBUTING.md](../CONTRIBUTING.md).

</details>

## Lizenz

[MIT](../LICENSE) © [redstone.md](https://redstone.md)
