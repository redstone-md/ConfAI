<p align="center">
  <img src="../assets/logo.svg" alt="ConfAI — un solo editor para la configuración de todos los agentes de IA" width="720">
</p>

<p align="center">
  <a href="https://github.com/redstone-md/ConfAI/actions/workflows/ci.yml"><img src="https://github.com/redstone-md/ConfAI/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <img src="https://img.shields.io/badge/rust-1.88%2B-b8352d" alt="Rust 1.88+">
  <img src="https://img.shields.io/badge/licence-MIT-b8352d" alt="MIT">
</p>

<p align="center">
  <a href="https://redstone.md">redstone.md</a> ·
  <a href="https://github.com/redstone-md/ConfAI">código</a> ·
  <a href="../CONTRIBUTING.md">contribuir</a> ·
  <a href="../LICENSE">MIT</a>
</p>

<p align="center">
  <a href="../README.md">English</a> ·
  <a href="README.ru.md">Русский</a> ·
  <a href="README.zh-CN.md">简体中文</a> ·
  <b>Español</b> ·
  <a href="README.de.md">Deutsch</a> ·
  <a href="README.ja.md">日本語</a>
</p>

---

Codex, Claude Code y opencode guardan sus endpoints cada uno en un archivo
distinto, en un formato distinto y con un nombre distinto para la misma idea.
Añadir un proveedor o cambiar entre dos de ellos significa abrir tres archivos a
mano. ConfAI lo hace con un solo comando, y nunca reformatea lo que no ha
cambiado.

## Instalación

Linux y macOS:

```sh
curl -fsSL https://github.com/redstone-md/ConfAI/releases/latest/download/install.sh | sh
```

Windows:

```powershell
irm https://github.com/redstone-md/ConfAI/releases/latest/download/install.ps1 | iex
```

Ambos scripts averiguan tu plataforma, descargan el archivo de la release
correspondiente y `SHA256SUMS`, verifican la suma de comprobación y solo entonces
colocan el binario. Canalizar un script de internet hacia una shell es una
decisión de confianza; en [INSTALL.md](../INSTALL.md) se explica cómo leerlo
antes.

Con cargo, una vez que el crate esté publicado en crates.io — a fecha de la
v0.0.1 no lo está, así que estos dos comandos todavía no funcionan:

```sh
cargo install confai --locked    # compila desde el código, necesita Rust 1.88+
cargo binstall confai            # descarga el archivo de la release en su lugar
```

O a mano: coge el archivo de tu plataforma en la
[página de releases](https://github.com/redstone-md/ConfAI/releases/latest),
compruébalo contra el `SHA256SUMS` publicado junto a él y pon el binario en tu
`PATH`.

El resto está en [INSTALL.md](../INSTALL.md): todas las plataformas, las opciones
de los instaladores, cómo se gestiona el `PATH` y cómo desinstalar.

## Qué hace

```
$ confai list
agent        detected         providers  active  model          config
Codex        binary + config  3          primary gpt-5.6-terra  ~/.codex/config.toml
Claude Code  binary + config  1          byesu   opus[1m]       ~/.claude/settings.json
opencode     binary + config  11         vendor              ~/.config/opencode/opencode.json
```

- Un comando cambia todos los agentes que tengan ese endpoint:
  `confai provider use primary`.
- Un preset escribe el mismo endpoint en todos ellos:
  `confai preset apply byesu --all --use`.
- `confai provider sync` rellena la lista de modelos que el endpoint sirve de
  verdad, con sus límites de contexto y de salida.
- Los comentarios, el orden de las claves y las claves desconocidas sobreviven a
  una edición. Cada escritura se respalda antes, y `confai undo` la deshace.

Ejecutar `confai` sin argumentos abre un navegador de dos paneles: los agentes a
la izquierda, los endpoints de ese agente a la derecha.

<p align="center">
  <img src="../assets/screenshots/tui.png" alt="La vista interactiva de ConfAI: agentes a la izquierda, endpoints a la derecha" width="900">
</p>

<details>
<summary><b>La línea de comandos</b> — todos los subcomandos y opciones</summary>

```sh
confai                                    # vista interactiva
confai list                               # qué está instalado, y dónde
confai provider list --check              # todos los endpoints, y si responden
confai provider add byesu \
    --agent codex \
    --base-url https://byesu.com/v1 \
    --api-key "$BYESU_API_KEY" \
    --wire-api chat --use
confai provider use primary               # cambia todo agente que lo tenga
confai provider sync vendor --prune       # trae la lista de modelos del endpoint
confai preset apply byesu --all --use     # un endpoint, todos los agentes
confai doctor                             # ¿sigue todo parseando y resolviendo?
confai undo                               # devuelve lo que había antes
```

`--agent` apunta a un agente, `--all` a todos los instalados. Sin ninguno de los
dos, los comandos de lectura cubren todo y los de escritura te piden elegir.

| Comando | |
|---|---|
| `list` | qué agentes hay instalados y hacia dónde apuntan |
| `provider list` | endpoints de los agentes seleccionados; `--check` llama a cada uno |
| `provider add <id>` | añade un endpoint, o edita en uno existente los campos que pases |
| `provider remove <id>` | elimina un endpoint |
| `provider use <id>` | enruta un agente por uno de sus endpoints |
| `provider check [id]` | pregunta a los endpoints si están vivos y qué sirven |
| `provider models [id]` | qué sirve un endpoint, con límites y precios |
| `provider sync <id>` | escribe la lista de modelos en la configuración |
| `preset list` · `preset show <id>` | qué recetas existen, y qué escribiría una de ellas |
| `preset apply <id>` | escribe el endpoint de un preset en los agentes seleccionados |
| `model [model]` | muestra o fija el modelo que usa un agente |
| `path` · `edit` | imprime la ruta del config · lo abre en `$EDITOR` |
| `doctor` | comprueba que todo config parsea y que cada proveedor referenciado resuelve |
| `about` · `update` | versión y rutas de estado · si existe una release más nueva |
| `undo` | restaura el config respaldado antes de la última escritura |

`provider add` acepta `--base-url`, `--api-key`, `--wire-api` (`chat`,
`responses` o `anthropic`), `--name`, un `--set CLAVE=VALOR` repetible para
claves propias de cada backend, y `--use` / `--sync` para seleccionar el endpoint
y traer sus modelos tras escribirlo. `provider check` acepta `--timeout` en
segundos, 10 por defecto. `provider models` acepta `--select <model>` y
`--refresh`. `provider sync` acepta `--prune`, `--dry-run` y `--refresh`.
`preset apply` acepta `--api-key`, `--use` y `--sync`.

`list`, `doctor`, `about`, `update` y `preset list` no aceptan selector de
agente: siempre lo cubren todo.

</details>

<details>
<summary><b>La vista interactiva</b> — la paleta de comandos, el detalle, el mapa de teclas completo</summary>

La paleta de comandos en `Ctrl+P` lista cada acción con la tecla que la ejecuta,
así que los atajos se aprenden usándolos y no leyendo esta página:

<p align="center">
  <img src="../assets/screenshots/palette.png" alt="La paleta de comandos, con cada acción y su tecla" width="900">
</p>

`Enter` sobre un endpoint muestra todo lo que hay registrado sobre él, incluida
la lista de modelos con sus límites de contexto y de salida:

<p align="center">
  <img src="../assets/screenshots/detail.png" alt="La vista de detalle del proveedor, con los campos del endpoint y su lista de modelos" width="900">
</p>

| tecla | |
|---|---|
| `Ctrl+P` / `Ctrl+K` | paleta de comandos — todas las acciones, con búsqueda |
| `↑` `↓` / `k` `j` | moverse · `Tab` `←` `→` cambiar de panel |
| `Enter` | detalle del endpoint, con su lista de modelos |
| `/` o `Ctrl+F` | filtrar endpoints por id, host o modelo |
| `u` | enrutar este agente por el endpoint seleccionado |
| `m` | elegir qué modelo usa este agente |
| `a` `e` `d` | añadir · editar · eliminar |
| `c` / `C` | comprobar este endpoint · comprobarlos todos |
| `s` / `S` | sincronizar modelos · sincronizar y podar los obsoletos |
| `p` | aplicar un preset |
| `?` | acerca de, y el mapa de teclas completo |
| `r` `q` | recargar desde disco · salir |

El ratón funciona: clic para seleccionar, otro clic para abrir, rueda para
desplazarse, clic en una pista para ejecutarla.

Las teclas se reconocen por su posición física, así que siguen funcionando en una
distribución no latina: `й` es `q`, `Ы` es `S`. `/` no tiene posición equivalente
en una distribución cirílica, y por eso `Ctrl+F` también abre el filtro.

Las ediciones pasan por el mismo camino de cargar-editar-guardar que la CLI, así
que valen las mismas garantías sobre tus archivos.

</details>

<details>
<summary><b>Lo que no le hará a tus archivos</b> — comentarios, orden de claves, copias de seguridad</summary>

Los configs se escriben a mano, y los archivos escritos a mano tienen cosas
dentro que una ida y vuelta ingenua destruye.

- **Los comentarios sobreviven.** Los configs de Codex se editan con `toml_edit`,
  así que un endpoint de repuesto aparcado en un `base_url` comentado sigue ahí
  después.
- **Solo cambia lo que hayas cambiado.** El orden de las claves, la indentación y
  las claves desconocidas se dejan en paz, porque cada backend edita el documento
  parseado en su sitio en vez de reserializar su propia idea del archivo.
- **El JSON con comentarios se rechaza, no se estropea.** ConfAI tendría que
  descartarlos, así que se detiene y lo dice.
- **Cada escritura se respalda** junto al original, como `<name>.confai.bak`, y
  reemplaza el archivo de forma atómica. `confai undo` lo restaura.

</details>

<details>
<summary><b>Modelos y estado</b> — de dónde sale la lista de modelos, y el podado</summary>

opencode no ofrecerá un modelo del que no le hayan hablado, y quiere el límite de
contexto escrito explícitamente. `confai provider sync <id>` llama a `/v1/models`
del endpoint, busca cada id en [models.dev](https://models.dev) para obtener sus
límites de contexto y salida, y escribe el resultado — dejando intactos
`variants` y cualquier otra cosa que hubieras configurado. El catálogo se cachea
un día; `--refresh` lo vuelve a descargar.

Sincronizar es fusionar, así que un modelo que la pasarela ya ha retirado
permanece en tu config hasta que digas lo contrario. `--prune` quita los que el
endpoint ya no sirve, y mueve tu selección de modelo a uno superviviente si podó
el que estaba seleccionado:

```sh
confai provider sync vendor --prune --dry-run   # ver qué se iría
confai provider sync vendor --prune
```

En la vista interactiva, `s` sincroniza y `S` sincroniza podando.

`confai provider models <id>` lista lo que sirve un endpoint sin escribir nada, y
`--select` convierte uno de ellos en el modelo del agente. Esto funciona también
para Codex y Claude Code, que registran un modelo pero no una lista de modelos.

`confai provider check` es la misma llamada sin la escritura: informa de si cada
endpoint está en pie, cuánto tardó en responder y cuántos modelos sirve.

</details>

<details>
<summary><b>Presets</b> — una receta de endpoint, cualquier agente</summary>

Un preset es un endpoint descrito una sola vez, en términos neutrales respecto al
agente, para que la misma receta se aplique a cualquiera de ellos:

```sh
confai preset list
confai preset show byesu
confai preset apply byesu --all --api-key sk-... --use --sync
```

Los veintiséis presets integrados viven en [`presets/`](../presets/) — un archivo
TOML cada uno, incrustados en el binario al compilar — y cubren OpenCode Zen,
OpenRouter, OpenAI, Anthropic, Groq, xAI, Mistral, Cerebras, Together, DeepSeek,
DeepInfra, Fireworks, Moonshot, Z.ai, Chutes, Baseten, Vercel AI Gateway, Venice,
Novita, Byesu, Ollama y LM Studio. Añadir uno es un pull request que toca un
único archivo nuevo. Tus propios presets van en `~/.confai/presets/`, y
sobrescriben a un integrado con el mismo id.

</details>

<details>
<summary><b>Agentes</b> — las tres formas de config, y qué hace ConfAI con cada una</summary>

| Agente | Config | Claves | Proveedores con nombre | Lista de modelos | Cambio |
|---|---|---|---|---|---|
| Codex | `~/.codex/config.toml` | mismo archivo | sí | no | `model_provider` |
| Claude Code | `~/.claude/settings.json` | bloque `env` | vía ConfAI | no | `ANTHROPIC_*` |
| opencode | `~/.config/opencode/opencode.json` | `~/.local/share/opencode/auth.json` | sí | sí | `provider/model` |

`CODEX_HOME`, `CLAUDE_CONFIG_DIR`, `OPENCODE_CONFIG` y `XDG_CONFIG_HOME` se
respetan, igual que los respetan los propios agentes.

Claude Code apunta a un endpoint cada vez, mediante variables de entorno en sus
ajustes, y no tiene dónde guardar los endpoints que no estás usando. ConfAI
mantiene ese registro en `~/.confai/agents/claude.json` y escribe solo la entrada
seleccionada en el archivo que Claude Code posee.

opencode está repartido en dos archivos: los proveedores en `opencode.json`, las
claves en `~/.local/share/opencode/auth.json`, donde las deja
`opencode auth login`. ConfAI lee ambos, así que una comprobación de estado sale
con la credencial que opencode usaría de verdad en vez de devolver un 401 falso.
Una clave nueva se escribe en `auth.json`; una clave que ya está en línea dentro
de `opencode.json` se actualiza donde está, porque mover un secreto de un archivo
a otro sin avisar es su propia clase de sorpresa. Una sesión OAuth en `auth.json`
se muestra pero nunca se sobrescribe — ConfAI te dice que ejecutes
`opencode auth logout` en vez de terminarla en silencio.

Añadir un agente es un archivo en `src/agent/` que implemente `Agent` y
`AgentConfig`; nada por encima de esa capa sabe con qué agente está hablando.

</details>

<details>
<summary><b>Mantenerse al día</b> — la comprobación de actualizaciones y cómo desactivarla</summary>

`confai update` informa de si existe una release más nueva, resume qué ha
cambiado e imprime cómo actualizar.

En el día a día no hace falta preguntar. Después de un comando, ConfAI imprime un
aviso de dos líneas en stderr si hay una release más nueva:

```
◆ 0.0.1 → 0.0.2 available
  · provider sync now prunes retired models
  · run `confai update` for the rest
```

Ese aviso se dibuja desde una caché que se comprueba como mucho una vez al día,
así que una ejecución normal no cuesta nada; y cuando la caché está caducada, la
comprobación tiene cuatrocientos milisegundos para responder antes de que la
ejecución lo deje estar y lo intente mañana. Una comprobación fallida espera una
hora en lugar de reintentar en cada invocación. Define
`CONFAI_NO_UPDATE_CHECK` para desactivarlo del todo.

ConfAI no reemplaza su propio binario. `cargo` y los instaladores ya hacen eso
correctamente, y una herramienta que se reescribe a sí misma mientras tiene tus
credenciales abiertas es peor trato que imprimir una línea.

</details>

<details>
<summary><b>Contribuir</b> — añadir un preset o un agente</summary>

Un preset es un archivo nuevo en `presets/`. Un agente nuevo es un archivo nuevo
en `src/agent/` que implemente `Agent` y `AgentConfig` — las capas por encima
quedan intactas. Ejecuta `cargo test` y `cargo clippy --lib --bins --tests` antes
de abrir un pull request. Ver [CONTRIBUTING.md](../CONTRIBUTING.md).

</details>

## Licencia

[MIT](../LICENSE) © [redstone.md](https://redstone.md)
