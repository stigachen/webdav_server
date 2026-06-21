use std::io::{self, Read, Write};
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::time::Duration;

use crate::core::events::{Metrics, ServerEvent};
use crate::core::server::ServerInfo;

pub struct ConsoleUi {
    info: ServerInfo,
}

impl ConsoleUi {
    pub fn new(info: ServerInfo) -> Self {
        Self { info }
    }

    pub fn run(&self, events: Receiver<ServerEvent>) {
        if !self.info.tui_enabled {
            self.render_plain();
            wait_for_enter();
            return;
        }

        let quit = spawn_enter_listener();
        let mut metrics = Metrics::new();

        loop {
            while let Ok(event) = events.try_recv() {
                metrics.apply(event);
            }
            self.render_dashboard(&metrics);
            if quit.try_recv().is_ok() {
                break;
            }
            thread::sleep(Duration::from_millis(250));
        }
        clear_screen();
    }

    fn render_plain(&self) {
        let url = format!("http://{}:{}/", self.info.display_host, self.info.port);
        println!("davbox online: {url}");
        println!("Press Enter or Ctrl+C to stop.");
    }

    fn render_dashboard(&self, metrics: &Metrics) {
        clear_screen();
        let url = format!("http://{}:{}/", self.info.display_host, self.info.port);
        let (upload_rate, download_rate) = metrics.transfer_rates();

        println!(
            r#"
╭────────────────────────────────────────────────────────────╮
│  DAVBOX  // local WebDAV uplink active                     │
├────────────────────────────────────────────────────────────┤
│  Name       {name:<47}│
│  Folder     {folder:<47}│
│  WebDAV     {url:<47}│
│  Bind       {bind:<47}│
│  Mode       {mode:<47}│
│  Auth       {auth:<47}│
├────────────────────────────────────────────────────────────┤
│  Uptime     {uptime:<47}│
│  Clients    active {active:<5} total {total_clients:<27}│
│  Requests   {requests:<47}│
│  Traffic    up {upload:<16} down {download:<20}│
│  Totals     in {total_in:<16} out {total_out:<20}│
├────────────────────────────────────────────────────────────┤
│  Recent Activity                                           │
{activity}
╰────────────────────────────────────────────────────────────╯

Press Enter or Ctrl+C to stop.
"#,
            name = truncate(&self.info.name, 47),
            folder = truncate(&self.info.folder, 47),
            url = truncate(&url, 47),
            bind = truncate(&format!("{}:{}", self.info.bind_host, self.info.port), 47),
            mode = if self.info.read_only {
                "read-only"
            } else {
                "read-write"
            },
            auth = truncate(&self.auth_line(), 47),
            uptime = format_duration(metrics.uptime()),
            active = metrics.active_clients(),
            total_clients = metrics.total_clients(),
            requests = metrics.total_requests(),
            upload = format!("{}/s", format_bytes(upload_rate as u64)),
            download = format!("{}/s", format_bytes(download_rate as u64)),
            total_in = format_bytes(metrics.total_bytes_in()),
            total_out = format_bytes(metrics.total_bytes_out()),
            activity = format_activity(metrics),
        );
        let _ = io::stdout().flush();
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

fn spawn_enter_listener() -> Receiver<()> {
    let (sender, receiver) = channel();
    thread::spawn(move || {
        wait_for_enter();
        let _ = sender.send(());
    });
    receiver
}

fn wait_for_enter() {
    let mut buffer = [0u8; 1];
    let _ = io::stdin().read(&mut buffer);
}

fn clear_screen() {
    print!("\x1b[2J\x1b[H");
}

fn format_activity(metrics: &Metrics) -> String {
    let mut lines = Vec::new();
    for entry in metrics.recent() {
        lines.push(format!(
            "│  {:<8} {:<4} {:<24} {:>8} {:>6}ms │",
            truncate(&entry.method, 8),
            entry.status,
            truncate(&entry.path, 24),
            format_bytes(entry.bytes_in + entry.bytes_out),
            entry.duration.as_millis()
        ));
    }
    while lines.len() < 8 {
        lines.push("│                                                            │".to_string());
    }
    lines.join("\n")
}

fn format_duration(duration: Duration) -> String {
    let total = duration.as_secs();
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}h {minutes}m {seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
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
