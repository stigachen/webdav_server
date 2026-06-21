# Davbox

Davbox instantly turns a folder on your computer into a WebDAV server for your local network.

It is designed for quick file access, document syncing, and media streaming from devices such as iPhone, iPad, Apple TV, Android TV boxes, macOS Finder, Windows Explorer, VLC, Kodi, and Infuse.

```sh
davbox serve ~/Movies
```

Davbox then prints a local WebDAV URL, username, and password.

## Status

This repository currently contains the first Rust MVP:

- Cross-platform Rust CLI structure.
- Local folder to WebDAV server.
- Basic authentication with generated runtime password.
- Single user config file with multiple profiles.
- Read-only mode.
- WebDAV methods: `OPTIONS`, `PROPFIND`, `GET`, `HEAD`, `PUT`, `DELETE`, `MKCOL`, `COPY`, `MOVE`.
- Byte range reads for media clients.
- Streaming file responses for large downloads and media playback.
- Live terminal dashboard with active requests, connections, traffic, and recent activity.
- Unit and functional tests.

## Install From Release

Download the package for your platform from GitHub Releases, then unpack it and run `davbox`.

macOS Apple Silicon:

```text
davbox-v0.1.2-aarch64-apple-darwin.tar.gz
```

Linux x64:

```text
davbox-v0.1.2-x86_64-unknown-linux-gnu.tar.gz
```

Windows x64:

```text
davbox-v0.1.2-x86_64-pc-windows-msvc.zip
```

### macOS Gatekeeper

Current macOS release binaries are not signed with an Apple Developer ID and are not notarized yet. If you download Davbox from GitHub, macOS may show:

```text
Apple could not verify "davbox" is free of malware
```

This happens because browser-downloaded files receive the `com.apple.quarantine` attribute. Locally built binaries under `target/` usually do not have that attribute.

After unpacking the release archive, you can remove the quarantine attribute:

```sh
xattr -dr com.apple.quarantine ./davbox
chmod +x ./davbox
./davbox --version
```

If you want to clear the whole unpacked folder:

```sh
xattr -dr com.apple.quarantine ./davbox-v0.1.2-aarch64-apple-darwin
```

Long term, macOS release artifacts should be signed and notarized before broad public distribution.

## Install From Source

You need Rust installed.

```sh
cargo build --release
```

The binary will be available at:

```sh
target/release/davbox
```

You can also run it directly during development:

```sh
cargo run -- serve ~/Movies
```

## Quick Start

Share a folder:

```sh
davbox serve ~/Movies
```

Share as read-only:

```sh
davbox serve ~/Movies --read-only
```

Use a custom port:

```sh
davbox serve ~/Movies --port 8090
```

Disable authentication for a trusted temporary LAN session:

```sh
davbox serve ~/Movies --no-auth
```

Use an explicit username and password:

```sh
davbox serve ~/Movies --user media --password secret
```

Use an environment variable for the password:

```sh
DAVBOX_PASSWORD=secret davbox serve ~/Movies
```

## Connect From Devices

After startup, Davbox prints a URL like:

```text
http://192.168.1.23:8080/
```

Use that URL in your WebDAV client.

Default username:

```text
davbox
```

If you do not provide a password, Davbox generates a temporary password and prints it at startup.

## Terminal Dashboard

By default Davbox shows a live terminal dashboard:

```text
██████╗   █████╗  ██╗   ██╗ ██████╗   ██████╗  ██╗  ██╗
██╔══██╗ ██╔══██╗ ██║   ██║ ██╔══██╗ ██╔═══██╗ ╚██╗██╔╝
██║  ██║ ███████║ ██║   ██║ ██████╔╝ ██║   ██║  ╚███╔╝

             local folder uplink // WebDAV over LAN
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

▸ UPLINK
  • Name       Davbox
  • Folder     /Users/alice/Movies
  • WebDAV     http://192.168.1.23:8080/

▸ TELEMETRY
  ◆ Uptime     3m 12s
  ◆ Active req 1
  ◆ Conn total 4
  ◆ Traffic    up 20.4 KB/s   down 84.1 MB/s

▸ RECENT ACTIVITY
  › GET       206  /movie.mkv                         84.1 MB   12ms
  › PROPFIND  207  /                                  1.2 KB    0ms
```

Use plain startup output instead:

```sh
davbox serve ~/Movies --no-tui
```

The dashboard uses the terminal alternate screen, so live refreshes do not fill your shell history with repeated UI frames. The top logo is drawn once and the live status area refreshes below it, which keeps Windows terminals much calmer.

## Config File

Davbox works without a config file. For repeated use, create one:

```sh
davbox config init
```

Show the config file path:

```sh
davbox config path
```

Print the current config file:

```sh
davbox config show
```

Default config locations:

```text
macOS     ~/.davbox/config.toml
Linux     ~/.davbox/config.toml
Windows   %USERPROFILE%\.davbox\config.toml
```

Example:

```toml
[server]
host = "0.0.0.0"
port = 8080
name = "Davbox"
read_only = false
hide_dotfiles = true
follow_symlinks = false
enable_mdns = false

[auth]
enabled = true
username = "davbox"
password_env = "DAVBOX_PASSWORD"

[tui]
enabled = true
theme = "matrix"
refresh_ms = 750

[profiles.movies]
path = "~/Movies"
name = "Movies"
port = 8080
read_only = true
```

Start a profile:

```sh
davbox serve movies
```

Use another config file:

```sh
davbox serve movies --config ./davbox.toml
```

## CLI Reference

```text
davbox serve <folder-or-profile> [options]
davbox config <command> [options]
```

Help:

```text
davbox --help
davbox serve --help
davbox config --help
```

Config commands:

```text
davbox config init [--config FILE]
davbox config path
davbox config show [--config FILE]
```

Serve options:

```text
--host HOST             Bind address, default 0.0.0.0
--port PORT             Bind port, default 8080. Use 0 for a random free port
--name NAME             Display/server name
--read-only             Reject write methods
--user USER             Basic auth username
--password PASSWORD     Basic auth password
--no-auth               Disable authentication
--no-tui                Plain startup output
--config FILE           Use an explicit config file
```

## Security Defaults

Davbox is built around a shared-root sandbox:

- Requests cannot escape the shared folder with `..`.
- Dotfiles are hidden by default.
- Symlinks are blocked by default.
- Authentication is enabled by default.
- `--read-only` rejects write methods.

For best results, keep authentication enabled unless you are doing a short trusted LAN test.

## Development

Run tests:

```sh
cargo test
```

Check formatting:

```sh
cargo fmt --check
```

The test suite includes:

- CLI parsing tests.
- Config parsing and merge tests.
- Authentication tests.
- Path sandbox tests.
- Range request tests.
- Streaming byte-range response tests.
- Server event and metrics tests.
- Functional HTTP/WebDAV server tests over localhost.

## Release

Releases are built by GitHub Actions when a version tag is pushed:

```sh
git tag v0.1.2
git push origin v0.1.2
```

The release workflow builds and uploads:

```text
macOS Apple Silicon   davbox-v0.1.2-aarch64-apple-darwin.tar.gz
Linux x64             davbox-v0.1.2-x86_64-unknown-linux-gnu.tar.gz
Windows x64           davbox-v0.1.2-x86_64-pc-windows-msvc.zip
```

Each artifact includes a `.sha256` checksum file.

## Roadmap

Near-term:

- `davbox doctor` for firewall, port, and network diagnostics.
- QR code display for quick mobile setup.
- More compatibility testing with Finder, Windows Explorer, iOS Files, VLC, Kodi, and Infuse.
