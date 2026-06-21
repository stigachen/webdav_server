use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::cli::ServeArgs;
use crate::core::auth::AuthConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub name: String,
    pub read_only: bool,
    pub hide_dotfiles: bool,
    pub follow_symlinks: bool,
    pub enable_mdns: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            name: "Davbox".to_string(),
            read_only: false,
            hide_dotfiles: true,
            follow_symlinks: false,
            enable_mdns: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuiConfig {
    pub enabled: bool,
    pub theme: String,
    pub refresh_ms: u64,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            theme: "matrix".to_string(),
            refresh_ms: 250,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileConfig {
    pub path: Option<String>,
    pub name: Option<String>,
    pub port: Option<u16>,
    pub read_only: Option<bool>,
    pub hide_dotfiles: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub server: ServerConfig,
    pub auth: AuthConfig,
    pub tui: TuiConfig,
    pub profiles: HashMap<String, ProfileConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            auth: AuthConfig::default(),
            tui: TuiConfig::default(),
            profiles: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveConfig {
    pub folder: PathBuf,
    pub server: ServerConfig,
    pub auth: AuthConfig,
    pub tui: TuiConfig,
}

impl Config {
    pub fn load_optional(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        Self::parse(&fs::read_to_string(path)?)
    }

    pub fn parse(source: &str) -> Result<Self, ConfigError> {
        let mut config = Config::default();
        let mut section = Section::Root;

        for (index, raw_line) in source.lines().enumerate() {
            let line = raw_line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                let name = &line[1..line.len() - 1];
                section = match name {
                    "server" => Section::Server,
                    "auth" => Section::Auth,
                    "tui" => Section::Tui,
                    value if value.starts_with("profiles.") => {
                        Section::Profile(value["profiles.".len()..].to_string())
                    }
                    _ => {
                        return Err(ConfigError::Parse(format!(
                            "Unsupported section [{name}] at line {}",
                            index + 1
                        )));
                    }
                };
                continue;
            }

            let Some((raw_key, raw_value)) = line.split_once('=') else {
                return Err(ConfigError::Parse(format!(
                    "Invalid line {}: {raw_line}",
                    index + 1
                )));
            };
            let key = raw_key.trim();
            let value = Value::parse(raw_value.trim())
                .map_err(|err| ConfigError::Parse(format!("{err} at line {}", index + 1)))?;
            apply_value(&mut config, &section, key, value)?;
        }
        Ok(config)
    }
}

impl EffectiveConfig {
    pub fn from_inputs(
        config: Config,
        args: ServeArgs,
        env: &[(String, String)],
    ) -> Result<Self, String> {
        let profile = config.profiles.get(&args.target);
        let target = profile
            .and_then(|item| item.path.clone())
            .unwrap_or_else(|| args.target.clone());

        let mut server = config.server;
        if let Some(profile) = profile {
            if let Some(name) = &profile.name {
                server.name = name.clone();
            }
            if let Some(port) = profile.port {
                server.port = port;
            }
            if let Some(read_only) = profile.read_only {
                server.read_only = read_only;
            }
            if let Some(hide_dotfiles) = profile.hide_dotfiles {
                server.hide_dotfiles = hide_dotfiles;
            }
        }
        if let Some(host) = args.host {
            server.host = host;
        }
        if let Some(port) = args.port {
            server.port = port;
        }
        if let Some(name) = args.name {
            server.name = name;
        }
        if let Some(read_only) = args.read_only {
            server.read_only = read_only;
        }

        let mut auth = config.auth;
        if let Some(user) = args.user {
            auth.username = user;
        }
        if let Some(password) = args.password {
            auth.password = Some(password);
        }
        if args.no_auth {
            auth.enabled = false;
        }
        auth = auth.with_runtime_password(env);

        let mut tui = config.tui;
        if let Some(enabled) = args.tui {
            tui.enabled = enabled;
        }

        Ok(Self {
            folder: expand_home(&target),
            server,
            auth,
            tui,
        })
    }
}

pub fn default_config_path() -> PathBuf {
    if cfg!(target_os = "macos") {
        home_dir()
            .join("Library")
            .join("Application Support")
            .join("davbox")
            .join("config.toml")
    } else if cfg!(target_os = "windows") {
        env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().join("AppData").join("Roaming"))
            .join("davbox")
            .join("config.toml")
    } else {
        env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().join(".config"))
            .join("davbox")
            .join("config.toml")
    }
}

pub fn write_default_config(path: &Path) -> Result<PathBuf, ConfigError> {
    if path.exists() {
        return Err(ConfigError::Parse(format!(
            "Config already exists: {}",
            path.display()
        )));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, EXAMPLE_CONFIG)?;
    Ok(path.to_path_buf())
}

pub const EXAMPLE_CONFIG: &str = r#"[server]
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
refresh_ms = 250

[profiles.movies]
path = "~/Movies"
name = "Movies"
port = 8080
read_only = true
"#;

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Parse(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(err) => write!(f, "{err}"),
            ConfigError::Parse(err) => write!(f, "{err}"),
        }
    }
}

impl From<std::io::Error> for ConfigError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug, Clone)]
enum Section {
    Root,
    Server,
    Auth,
    Tui,
    Profile(String),
}

#[derive(Debug, Clone)]
enum Value {
    String(String),
    Bool(bool),
    Integer(u16),
}

impl Value {
    fn parse(input: &str) -> Result<Self, String> {
        if input.starts_with('"') && input.ends_with('"') {
            return Ok(Value::String(input[1..input.len() - 1].to_string()));
        }
        if input == "true" {
            return Ok(Value::Bool(true));
        }
        if input == "false" {
            return Ok(Value::Bool(false));
        }
        input
            .parse::<u16>()
            .map(Value::Integer)
            .map_err(|_| format!("Unsupported value: {input}"))
    }
}

fn apply_value(
    config: &mut Config,
    section: &Section,
    key: &str,
    value: Value,
) -> Result<(), ConfigError> {
    match section {
        Section::Server => match key {
            "host" => config.server.host = expect_string(value, key)?,
            "port" => config.server.port = expect_integer(value, key)?,
            "name" => config.server.name = expect_string(value, key)?,
            "read_only" => config.server.read_only = expect_bool(value, key)?,
            "hide_dotfiles" => config.server.hide_dotfiles = expect_bool(value, key)?,
            "follow_symlinks" => config.server.follow_symlinks = expect_bool(value, key)?,
            "enable_mdns" => config.server.enable_mdns = expect_bool(value, key)?,
            _ => return Err(ConfigError::Parse(format!("Unsupported server key: {key}"))),
        },
        Section::Auth => match key {
            "enabled" => config.auth.enabled = expect_bool(value, key)?,
            "username" => config.auth.username = expect_string(value, key)?,
            "password" => config.auth.password = Some(expect_string(value, key)?),
            "password_env" => config.auth.password_env = Some(expect_string(value, key)?),
            _ => return Err(ConfigError::Parse(format!("Unsupported auth key: {key}"))),
        },
        Section::Tui => match key {
            "enabled" => config.tui.enabled = expect_bool(value, key)?,
            "theme" => config.tui.theme = expect_string(value, key)?,
            "refresh_ms" => config.tui.refresh_ms = expect_integer(value, key)? as u64,
            _ => return Err(ConfigError::Parse(format!("Unsupported tui key: {key}"))),
        },
        Section::Profile(name) => {
            let profile = config.profiles.entry(name.clone()).or_default();
            match key {
                "path" => profile.path = Some(expect_string(value, key)?),
                "name" => profile.name = Some(expect_string(value, key)?),
                "port" => profile.port = Some(expect_integer(value, key)?),
                "read_only" => profile.read_only = Some(expect_bool(value, key)?),
                "hide_dotfiles" => profile.hide_dotfiles = Some(expect_bool(value, key)?),
                _ => {
                    return Err(ConfigError::Parse(format!(
                        "Unsupported profile key: {key}"
                    )));
                }
            }
        }
        Section::Root => return Err(ConfigError::Parse(format!("Key outside section: {key}"))),
    }
    Ok(())
}

fn expect_string(value: Value, key: &str) -> Result<String, ConfigError> {
    match value {
        Value::String(value) => Ok(value),
        _ => Err(ConfigError::Parse(format!("{key} must be a string"))),
    }
}

fn expect_bool(value: Value, key: &str) -> Result<bool, ConfigError> {
    match value {
        Value::Bool(value) => Ok(value),
        _ => Err(ConfigError::Parse(format!("{key} must be a boolean"))),
    }
}

fn expect_integer(value: Value, key: &str) -> Result<u16, ConfigError> {
    match value {
        Value::Integer(value) => Ok(value),
        _ => Err(ConfigError::Parse(format!("{key} must be an integer"))),
    }
}

fn expand_home(value: &str) -> PathBuf {
    if value == "~" {
        return home_dir();
    }
    if let Some(rest) = value.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    PathBuf::from(value)
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use crate::cli::ServeArgs;

    use super::{Config, EffectiveConfig};

    #[test]
    fn parses_profile_config() {
        let source = r#"
[server]
port = 8080
read_only = false

[profiles.movies]
path = "~/Movies"
port = 9000
read_only = true
"#;
        let config = Config::parse(source).unwrap();
        let profile = config.profiles.get("movies").unwrap();
        assert_eq!(profile.port, Some(9000));
        assert_eq!(profile.read_only, Some(true));
    }

    #[test]
    fn cli_overrides_profile() {
        let config = Config::parse(
            r#"
[profiles.movies]
path = "~/Movies"
port = 9000
read_only = true
"#,
        )
        .unwrap();
        let args = ServeArgs {
            target: "movies".to_string(),
            port: Some(7000),
            read_only: Some(false),
            no_auth: true,
            ..ServeArgs::default()
        };
        let effective = EffectiveConfig::from_inputs(config, args, &[]).unwrap();
        assert_eq!(effective.server.port, 7000);
        assert!(!effective.server.read_only);
        assert!(!effective.auth.enabled);
    }
}
