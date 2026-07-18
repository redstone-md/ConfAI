# Installing ConfAI

Five ways in, in rough order of how much you have to think about them. Pick one.

| | Command | Needs |
|---|---|---|
| Prebuilt, scripted | `curl -fsSL .../install.sh \| sh` | curl, tar |
| Prebuilt, scripted (Windows) | `irm .../install.ps1 \| iex` | PowerShell 5.1+ |
| Prebuilt, no script | Download an archive from the releases page | nothing |
| Prebuilt, via cargo | `cargo binstall confai` | `cargo-binstall` |
| From source | `cargo install confai` | Rust 1.88+ |

---

## cargo install

The route with no trust question attached: cargo builds the crate on your
machine from the source published to crates.io.

```sh
cargo install confai
```

This needs a Rust toolchain at or above the `rust-version` in `Cargo.toml`
(1.88 at the time of writing) and takes a few minutes, since the release
profile uses fat LTO. The binary lands in `~/.cargo/bin`, which cargo already
puts on your PATH.

Upgrade with the same command; add `--force` if cargo thinks the installed
version is current.

## cargo binstall

`cargo-binstall` fetches the release archive built by CI instead of compiling,
so it takes seconds rather than minutes:

```sh
cargo install cargo-binstall   # once
cargo binstall confai
```

It resolves the version through crates.io, then downloads the matching archive
and checksum from the GitHub release.

## Release archives

Every release publishes one archive per platform, plus a `SHA256SUMS` file
covering all of them.

| Platform | Target | Archive |
|---|---|---|
| Linux x86-64, glibc | `x86_64-unknown-linux-gnu` | `.tar.gz` |
| Linux x86-64, static | `x86_64-unknown-linux-musl` | `.tar.gz` |
| Linux ARM64, glibc | `aarch64-unknown-linux-gnu` | `.tar.gz` |
| Linux ARM64, static | `aarch64-unknown-linux-musl` | `.tar.gz` |
| Windows x86-64 | `x86_64-pc-windows-msvc` | `.zip` |
| Windows ARM64 | `aarch64-pc-windows-msvc` | `.zip` |
| macOS Intel | `x86_64-apple-darwin` | `.tar.gz` |
| macOS Apple silicon | `aarch64-apple-darwin` | `.tar.gz` |

The musl builds are statically linked, so they run on any Linux with a
compatible kernel, including Alpine and distroless containers. If you do not
know which libc you have, take musl.

Each archive contains the binary, `README.md`, `LICENSE` and `CHANGELOG.md`, in
a directory named after the archive.

```sh
version=0.0.1
target=x86_64-unknown-linux-musl
base="https://github.com/redstone-md/ConfAI/releases/download/v$version"

curl -fLO "$base/confai-$version-$target.tar.gz"
curl -fLO "$base/SHA256SUMS"

# Check it before you run it. This must print "OK".
sha256sum --ignore-missing -c SHA256SUMS

tar -xzf "confai-$version-$target.tar.gz"
install -m 755 "confai-$version-$target/confai" ~/.local/bin/
```

On macOS, `shasum -a 256 -c SHA256SUMS --ignore-missing` does the same job.

On Windows, from PowerShell:

```powershell
$version = '0.0.1'
$target  = 'x86_64-pc-windows-msvc'
$base    = "https://github.com/redstone-md/ConfAI/releases/download/v$version"

Invoke-WebRequest "$base/confai-$version-$target.zip" -OutFile confai.zip
Invoke-WebRequest "$base/SHA256SUMS" -OutFile SHA256SUMS

# Compare this hash against the matching line in SHA256SUMS.
(Get-FileHash confai.zip -Algorithm SHA256).Hash.ToLower()
Get-Content SHA256SUMS | Select-String "$target"

Expand-Archive confai.zip -DestinationPath .
```

---

## The install scripts

`install.sh` and `install.ps1` do exactly what the manual steps above do:
work out your platform, resolve the latest release, download the archive and
`SHA256SUMS`, **verify the checksum**, and only then put the binary in place. A
mismatch aborts with a loud error and installs nothing.

### A note on piping

`curl … | sh` runs code from the internet on your machine before you have seen
it. That is a trust decision, and it is yours to make, not ours to assume. Both
one-liners below are offered because they are convenient, not because piping is
safe in general.

If you would rather look first — and that is the better habit — download,
read, then run:

```sh
curl -fsSL -o install.sh https://raw.githubusercontent.com/redstone-md/ConfAI/main/install.sh
less install.sh
sh install.sh
```

```powershell
Invoke-WebRequest https://raw.githubusercontent.com/redstone-md/ConfAI/main/install.ps1 -OutFile install.ps1
Get-Content install.ps1 | more
.\install.ps1
```

Both scripts are also attached to every release, so you can take the copy that
shipped with the version you are installing rather than whatever is on `main`.

### Linux and macOS

```sh
curl -fsSL https://raw.githubusercontent.com/redstone-md/ConfAI/main/install.sh | sh
```

Options, passed after `-s --` when piping:

```sh
curl -fsSL https://raw.githubusercontent.com/redstone-md/ConfAI/main/install.sh | sh -s -- --prefix ~/bin
```

| Flag | Effect |
|---|---|
| `--version <vX.Y.Z>` | Install a specific release instead of the latest. |
| `--prefix <dir>` | Install into `<dir>`. |
| `--no-modify-path` | Never touch a shell profile. |
| `--force` | Reinstall even if that version is already present. |
| `--quiet` | Errors only. |
| `--uninstall` | Remove the binary and the PATH line the script added. |
| `--help` | Everything above. |

Without `--prefix`, it installs into `$XDG_BIN_HOME`, then `~/.local/bin`, then
`/usr/local/bin` — and it asks before using `sudo` for that last one. If the
chosen directory is not on your `PATH`, it appends one line to the profile for
your shell (`~/.bashrc`, `~/.zshrc`, `~/.config/fish/config.fish` or
`~/.profile`), prints exactly what it wrote, and never writes the same line
twice. Re-running the script to upgrade is safe.

The script is POSIX `sh`; it does not need bash.

### Windows

```powershell
irm https://raw.githubusercontent.com/redstone-md/ConfAI/main/install.ps1 | iex
```

To pass options through a pipe, PowerShell needs the script as a scriptblock:

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/redstone-md/ConfAI/main/install.ps1))) -Prefix C:\tools\confai
```

| Flag | Effect |
|---|---|
| `-Version <vX.Y.Z>` | Install a specific release. |
| `-Prefix <dir>` | Install into `<dir>`. |
| `-NoModifyPath` | Never touch PATH. |
| `-Force` | Reinstall even if that version is already present. |
| `-Quiet` | Errors only. |
| `-Uninstall` | Remove the binary and the PATH entry. |

The default location is `%LOCALAPPDATA%\Programs\confai`. PATH changes go to
your **user** PATH, never the machine PATH, so no elevation is involved and
nobody else's environment changes. Terminals that are already open keep the old
PATH until you restart them.

---

## Uninstalling

```sh
sh install.sh --uninstall        # or: confai-installed-via-cargo -> cargo uninstall confai
```

```powershell
.\install.ps1 -Uninstall
```

```sh
cargo uninstall confai           # if you used cargo install or cargo binstall
```

None of these touch `~/.confai`, where your presets and agent rosters live, nor
any agent config ConfAI has edited. Remove `~/.confai` yourself if you want it
gone. Backups ConfAI made alongside agent configs are named `<name>.confai.bak`
and are also left in place.

---

## Where releases live

Both install scripts, and anything else that wants to find a build, use these
URLs. They are stable and will not change shape between releases.

| What | URL |
|---|---|
| Latest release, as JSON | `https://api.github.com/repos/redstone-md/ConfAI/releases/latest` |
| Latest release, in a browser | `https://github.com/redstone-md/ConfAI/releases/latest` |
| One asset | `https://github.com/redstone-md/ConfAI/releases/download/v<version>/confai-<version>-<target>.<ext>` |
| Checksums for a release | `https://github.com/redstone-md/ConfAI/releases/download/v<version>/SHA256SUMS` |

The tag carries a `v` prefix; the file names do not. `<ext>` is `zip` for the
Windows targets and `tar.gz` everywhere else.

GitHub does not serve a fixed "latest binary" URL, so the version has to be
resolved first — the JSON above has it in `tag_name` — and then substituted
into the asset URL. Both scripts do exactly that, and anything checking for
updates should read `tag_name` from the same endpoint.

## Building from source

```sh
git clone https://github.com/redstone-md/ConfAI
cd ConfAI
cargo build --release
```

The binary is `target/release/confai`. `build.rs` bakes everything in `presets/`
into it, so a preset dropped into that directory is picked up by the next build.
See [CONTRIBUTING.md](CONTRIBUTING.md).
