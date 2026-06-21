# Davbox Design

This document is for the project author. It records the current architecture, product decisions, and code boundaries. Keep it synchronized with the implementation whenever behavior or structure changes.

## Product Goal

Davbox is a cross-platform command-line application that instantly transforms a local folder into a WebDAV server for the local network.

Primary use cases:

- Access files from phones and tablets.
- Sync documents through WebDAV clients.
- Stream large local media files to TV boxes and media players.
- Quickly expose a folder without installing a full server stack.

The first-run experience should remain:

```sh
davbox serve ~/Movies
```

No config file, daemon setup, database, or external runtime should be required.

## Current Implementation

The project is implemented in Rust and currently uses only the Rust standard library. That keeps the MVP easy to build in restricted environments and gives us a clear dependency baseline.

Implemented:

- CLI entrypoint and manual argument parsing.
- Single default config file with profile support.
- Effective config merge: defaults, config, profile, environment, CLI.
- WebDAV server over `TcpListener`.
- Basic auth.
- Runtime password generation.
- Shared-root file system backend.
- WebDAV methods: `OPTIONS`, `PROPFIND`, `GET`, `HEAD`, `PUT`, `DELETE`, `MKCOL`, `COPY`, `MOVE`.
- Byte range reads.
- Server event channel for client and request lifecycle events.
- Live terminal dashboard with active requests, connection count, traffic, totals, and recent activity.
- Unit and functional tests.

Not yet implemented:

- mDNS/Bonjour publishing.
- TLS.
- Real TOML parser with full TOML syntax.
- `davbox doctor`.
- Packaged installers.

## Module Layout

```text
src/
  main.rs
  lib.rs
  cli/
    mod.rs
  core/
    mod.rs
    auth.rs
    config.rs
    dav.rs
    events.rs
    fs_backend.rs
    http.rs
    network.rs
    server.rs
  tui.rs
docs/
  DESIGN.md
README.md
Cargo.toml
```

## Boundaries

The design intentionally separates UI/CLI from server core.

```text
CLI
  Parses user intent.
  Loads config.
  Builds EffectiveConfig.
  Starts/stops the server.

TUI
  Renders server state.
  Handles local user interaction.
  Must not implement WebDAV behavior.

Server Core
  Owns HTTP/WebDAV behavior.
  Owns auth checks.
  Owns file system sandboxing.
  Must not know where config came from.
```

The server receives an `EffectiveConfig`. It should not care whether settings came from CLI flags, profiles, environment variables, or future GUI controls.

## Config Design

Davbox has one default user-level config file:

```text
macOS     ~/.davbox/config.toml
Linux     ~/.davbox/config.toml
Windows   %USERPROFILE%\.davbox\config.toml
```

This intentionally follows the convention used by many CLI-first developer tools: a hidden home-directory folder that is easy to find, edit, back up, and discuss in documentation.

Users can override it:

```sh
davbox serve movies --config ./davbox.toml
```

The config file can contain global defaults and multiple profiles:

```toml
[server]
host = "0.0.0.0"
port = 8080
read_only = false

[auth]
enabled = true
username = "davbox"
password_env = "DAVBOX_PASSWORD"

[profiles.movies]
path = "~/Movies"
port = 8080
read_only = true
```

Merge priority:

```text
Built-in defaults
  < global config
  < selected profile
  < environment variables
  < CLI flags
```

Current parser intentionally supports only the subset we use: string, bool, integer, and flat sections. If config grows, replace the manual parser with `serde` plus a TOML crate.

## Server Core

`core::server::DavServer` owns the listener lifecycle. Each connection is handled on a thread. This is simple and adequate for the MVP.

The server emits events through `core::events::EventBus`. The TUI subscribes to those events and derives display metrics without inspecting server internals.

Current events:

- `ClientConnected`
- `RequestCompleted`
- `ServerStopped`

Future options:

- Move to an async runtime such as Tokio.
- Use Hyper/Axum for HTTP parsing.
- Keep `core::dav` and `core::fs_backend` independent enough to survive that migration.

Current WebDAV behavior:

- `OPTIONS`: advertises DAV support and allowed methods.
- `PROPFIND`: returns `207 Multi-Status` with basic resource properties.
- `GET`/`HEAD`: streams file content from shared root.
- `PUT`: writes uploaded file content.
- `DELETE`: removes files or folders.
- `MKCOL`: creates folders.
- `COPY`/`MOVE`: copies or renames inside the shared root.

Current implementation reads file responses into memory. This should become streaming before large media workloads are considered production-ready.

## File System Safety

`core::fs_backend::FileSystemBackend` is the security boundary around the shared directory.

Rules:

- Decode percent-encoded paths.
- Reject parent traversal.
- Resolve paths relative to the configured root.
- Ensure canonical parent paths remain inside the root.
- Hide dotfiles by default.
- Reject symlink traversal by default.

Any future file operation must go through `FileSystemBackend::resolve`.

## Authentication

Basic auth is enabled by default.

Password priority:

```text
CLI --password
  > config password
  > configured password_env
  > generated runtime password
```

The generated runtime password is printed in the terminal panel so the first run works without setup.

Future:

- macOS Keychain.
- Windows Credential Manager.
- Linux Secret Service.
- Optional per-profile credentials.

## Terminal UI

The current UI is a live dashboard. It prints:

- Server name.
- Shared folder.
- WebDAV URL.
- Bind address.
- Read/write mode.
- Auth info.
- Uptime.
- Active requests.
- Total connections.
- Request count.
- Upload and download rates over a short rolling window.
- Total bytes in and out.
- Recent request activity.

```text
██████╗   █████╗  ██╗   ██╗ ██████╗   ██████╗  ██╗  ██╗
██╔══██╗ ██╔══██╗ ██║   ██║ ██╔══██╗ ██╔═══██╗ ╚██╗██╔╝
██║  ██║ ███████║ ██║   ██║ ██████╔╝ ██║   ██║  ╚███╔╝

             local folder uplink // WebDAV over LAN
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

▸ UPLINK
  • WebDAV     http://192.168.1.8:8080/
  • Mode       read-only
  • Auth       davbox / 1234-5678

▸ TELEMETRY
  ◆ Active req 2
  ◆ Conn total 148
  ◆ Requests   148
  ◆ Traffic    up 3.2 MB/s   down 91.4 MB/s

▸ RECENT ACTIVITY
  › GET       206  /movie.mkv                         84.1 MB   12ms
  › PROPFIND  207  /                                  1.2 KB    0ms
```

The TUI uses ANSI color, a large terminal logo, a neon divider, and compact command-style sections. It consumes server events and maintains a `Metrics` model. Server core does not render UI and TUI does not implement WebDAV behavior.

The dashboard enters the terminal alternate screen and hides the cursor while running. On normal shutdown the terminal session guard restores the main screen, so periodic refreshes do not remain in shell scrollback.

## Testing Strategy

The project follows a test-driven style: every risky behavior should have a small focused test, and server behavior should have functional coverage.

Current tests cover:

- CLI argument parsing.
- Config profile parsing.
- CLI override priority.
- Basic auth.
- Percent decoding.
- Range parsing.
- Event metrics.
- Dotfile hiding.
- Parent traversal rejection.
- Real localhost server GET.
- Read-only write rejection.

Run:

```sh
cargo test
cargo fmt --check
```

Regression rule:

Any change to WebDAV behavior, path handling, auth, config merging, or startup defaults should add or update tests in the same change.

## Release Design

Target install experience:

```sh
brew install davbox
winget install davbox
curl -fsSL https://davbox.dev/install.sh | sh
```

Release artifacts:

```text
macOS arm64     davbox-vX.Y.Z-aarch64-apple-darwin.tar.gz
macOS x64       davbox-vX.Y.Z-x86_64-apple-darwin.tar.gz
Linux x64       davbox-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz
Windows x64     davbox-vX.Y.Z-x86_64-pc-windows-msvc.zip
```

GitHub Actions release flow:

```sh
git tag vX.Y.Z
git push origin vX.Y.Z
```

The workflow in `.github/workflows/release.yml` runs for `v*` tags. It builds and tests on macOS, Linux, and Windows, packages each platform artifact with README and DESIGN docs, generates SHA-256 checksum files, and uploads everything to the matching GitHub Release.

Each matrix job runs:

```sh
cargo fmt --check
cargo test
cargo build --release --target <target>
```

The release uses `GITHUB_TOKEN` with `contents: write` permission via `softprops/action-gh-release`.

## Near-Term Engineering Plan

1. Replace response buffering with streaming file transfer.
2. Add `davbox doctor`.
3. Add QR code rendering for mobile setup.
4. Add compatibility tests using real WebDAV clients where practical.
5. Add more release targets such as Linux aarch64.
