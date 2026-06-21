use std::io::{self, Read};

use crate::core::server::ServerInfo;

pub struct ConsoleUi {
    info: ServerInfo,
}

impl ConsoleUi {
    pub fn new(info: ServerInfo) -> Self {
        Self { info }
    }

    pub fn render_started(&self) {
        let url = format!("http://{}:{}/", self.info.display_host, self.info.port);
        if !self.info.tui_enabled {
            println!("davbox online: {url}");
            return;
        }

        println!(
            r#"
╭────────────────────────────────────────────╮
│  DAVBOX  // local WebDAV uplink active     │
├────────────────────────────────────────────┤
│  Name       {name:<31}│
│  Folder     {folder:<31}│
│  WebDAV     {url:<31}│
│  Bind       {bind:<31}│
│  Mode       {mode:<31}│
│  Auth       {auth:<31}│
╰────────────────────────────────────────────╯

Press Enter or Ctrl+C to stop.
"#,
            name = truncate(&self.info.name, 31),
            folder = truncate(&self.info.folder, 31),
            url = truncate(&url, 31),
            bind = truncate(&format!("{}:{}", self.info.bind_host, self.info.port), 31),
            mode = if self.info.read_only {
                "read-only"
            } else {
                "read-write"
            },
            auth = truncate(&self.auth_line(), 31),
        );
    }

    pub fn wait_for_shutdown(&self) {
        let mut buffer = [0u8; 1];
        let _ = io::stdin().read(&mut buffer);
    }

    fn auth_line(&self) -> String {
        if !self.info.auth_enabled {
            return "disabled".to_string();
        }
        format!(
            "{} / {}",
            self.info.username,
            self.info.password.as_deref().unwrap_or("<hidden>")
        )
    }
}

fn truncate(input: &str, max: usize) -> String {
    if input.chars().count() <= max {
        return input.to_string();
    }
    let mut out = input
        .chars()
        .take(max.saturating_sub(1))
        .collect::<String>();
    out.push('…');
    out
}
