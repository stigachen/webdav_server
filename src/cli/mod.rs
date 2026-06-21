use std::env;
use std::ffi::OsString;
use std::path::PathBuf;

use crate::core::config::{Config, EffectiveConfig, default_config_path, write_default_config};
use crate::core::server::DavServer;
use crate::tui::ConsoleUi;

pub fn run<I, T>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let args = args
        .into_iter()
        .map(|arg| arg.into().to_string_lossy().to_string())
        .collect::<Vec<_>>();

    match args.first().map(String::as_str) {
        None => print_help(),
        Some("-h" | "--help" | "help") => print_help(),
        Some("-V" | "--version") => {
            println!("davbox {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("config") => run_config(&args[1..]),
        Some("serve") => run_serve(&args[1..]),
        Some(other) => Err(format!("Unknown command: {other}\n\nTry: davbox --help")),
    }
}

fn run_config(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        None | Some("-h" | "--help" | "help") => print_config_help(),
        Some("path") => {
            println!("{}", default_config_path().display());
            Ok(())
        }
        Some("init") => {
            let path = option_value(args, "--config")
                .map(PathBuf::from)
                .unwrap_or_else(default_config_path);
            let written = write_default_config(&path).map_err(|err| err.to_string())?;
            println!("Created {}", written.display());
            Ok(())
        }
        Some("show") => {
            let path = option_value(args, "--config")
                .map(PathBuf::from)
                .unwrap_or_else(default_config_path);
            println!(
                "{}",
                std::fs::read_to_string(&path)
                    .map_err(|err| format!("Cannot read {}: {err}", path.display()))?
            );
            Ok(())
        }
        _ => Err(
            "Usage: davbox config <init|path|show> [--config FILE]\n\nTry: davbox config --help"
                .to_string(),
        ),
    }
}

fn run_serve(args: &[String]) -> Result<(), String> {
    if matches!(
        args.first().map(String::as_str),
        None | Some("-h" | "--help" | "help")
    ) {
        return print_serve_help();
    }
    let parsed = ServeArgs::parse(args)?;
    let config_path = parsed
        .config_path
        .clone()
        .unwrap_or_else(default_config_path);
    let config = Config::load_optional(&config_path).map_err(|err| err.to_string())?;
    let effective = EffectiveConfig::from_inputs(config, parsed, &env::vars().collect::<Vec<_>>())?;

    let mut server = DavServer::new(effective).map_err(|err| err.to_string())?;
    let events = server.subscribe();
    server.start().map_err(|err| err.to_string())?;

    let ui = ConsoleUi::new(server.info());
    ui.run(events);

    server.stop().map_err(|err| err.to_string())
}

fn print_help() -> Result<(), String> {
    println!("{}", main_help());
    Ok(())
}

fn print_serve_help() -> Result<(), String> {
    println!("{}", serve_help());
    Ok(())
}

fn print_config_help() -> Result<(), String> {
    println!("{}", config_help());
    Ok(())
}

fn main_help() -> &'static str {
    r#"DAVBOX // local WebDAV uplink

Usage:
  davbox serve <folder-or-profile> [options]
  davbox config <command> [options]

Commands:
  serve      Share a folder as a WebDAV server
  config     Create, inspect, or locate the config file
  help       Print this message

Examples:
  davbox serve ~/Movies
  davbox serve ~/Movies --read-only
  davbox config init

Run 'davbox serve --help' or 'davbox config --help' for command details.
"#
}

fn serve_help() -> &'static str {
    r#"DAVBOX SERVE // share a local folder

Usage:
  davbox serve <folder-or-profile> [options]

Options:
  --host HOST             Bind address, default 0.0.0.0
  --port PORT             Bind port, default 8080. Use 0 for a random free port
  --name NAME             Display/server name
  --read-only             Reject write methods
  --user USER             Basic auth username
  --password PASSWORD     Basic auth password
  --no-auth               Disable authentication
  --no-tui                Plain startup output
  --config FILE           Use an explicit config file

Examples:
  davbox serve ~/Movies
  davbox serve ~/Movies --read-only
  davbox serve movies --port 9000
  DAVBOX_PASSWORD=secret davbox serve movies
"#
}

fn config_help() -> &'static str {
    r#"DAVBOX CONFIG // manage configuration

Usage:
  davbox config init [--config FILE]
  davbox config path
  davbox config show [--config FILE]

Commands:
  init       Create a starter config file
  path       Print the default config file path
  show       Print the current config file

Default path:
  ~/.davbox/config.toml

Examples:
  davbox config init
  davbox config show
  davbox config init --config ./davbox.toml
"#
}

#[derive(Debug, Clone, Default)]
pub struct ServeArgs {
    pub target: String,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub name: Option<String>,
    pub read_only: Option<bool>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub no_auth: bool,
    pub tui: Option<bool>,
    pub config_path: Option<PathBuf>,
}

impl ServeArgs {
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let mut out = ServeArgs::default();
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--host" => out.host = Some(take_value(args, &mut i, "--host")?),
                "--port" => {
                    out.port = Some(
                        take_value(args, &mut i, "--port")?
                            .parse()
                            .map_err(|_| "Invalid --port".to_string())?,
                    )
                }
                "--name" => out.name = Some(take_value(args, &mut i, "--name")?),
                "--read-only" => out.read_only = Some(true),
                "--user" => out.user = Some(take_value(args, &mut i, "--user")?),
                "--password" => out.password = Some(take_value(args, &mut i, "--password")?),
                "--no-auth" => out.no_auth = true,
                "--no-tui" => out.tui = Some(false),
                "--config" => {
                    out.config_path = Some(PathBuf::from(take_value(args, &mut i, "--config")?))
                }
                value if value.starts_with('-') => return Err(format!("Unknown option: {value}")),
                value => {
                    if !out.target.is_empty() {
                        return Err(
                            "Only one folder or profile can be served at a time".to_string()
                        );
                    }
                    out.target = value.to_string();
                }
            }
            i += 1;
        }
        if out.target.is_empty() {
            return Err("Missing folder or profile. Try: davbox serve ~/Movies".to_string());
        }
        Ok(out)
    }
}

fn take_value(args: &[String], i: &mut usize, name: &str) -> Result<String, String> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or_else(|| format!("Missing value for {name}"))
}

fn option_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

#[cfg(test)]
mod tests {
    use super::{ServeArgs, config_help, main_help, serve_help};

    #[test]
    fn parses_serve_args() {
        let args = ["~/Movies", "--port", "9000", "--read-only", "--no-auth"].map(String::from);
        let parsed = ServeArgs::parse(&args).unwrap();
        assert_eq!(parsed.target, "~/Movies");
        assert_eq!(parsed.port, Some(9000));
        assert_eq!(parsed.read_only, Some(true));
        assert!(parsed.no_auth);
    }

    #[test]
    fn help_texts_include_subcommand_usage() {
        assert!(main_help().contains("davbox serve --help"));
        assert!(serve_help().contains("davbox serve <folder-or-profile>"));
        assert!(config_help().contains("davbox config init"));
    }
}
