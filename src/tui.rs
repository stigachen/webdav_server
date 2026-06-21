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
        let _terminal = TerminalSession::enter();

        loop {
            while let Ok(event) = events.try_recv() {
                metrics.apply(event);
            }
            self.render_dashboard(&metrics);
            if quit.try_recv().is_ok() {
                break;
            }
            thread::sleep(Duration::from_millis(750));
        }
    }

    fn render_plain(&self) {
        let url = format!("http://{}:{}/", self.info.display_host, self.info.port);
        println!("davbox online: {url}");
        println!("Press Enter or Ctrl+C to stop.");
    }

    fn render_dashboard(&self, metrics: &Metrics) {
        move_home();
        let url = format!("http://{}:{}/", self.info.display_host, self.info.port);
        let (upload_rate, download_rate) = metrics.transfer_rates();
        let mode = if self.info.read_only {
            "read-only"
        } else {
            "read-write"
        };

        println!(
            "{}{}{}{}{}{}{}{}{}{}",
            render_logo(),
            line(&magenta(
                "             local folder uplink // WebDAV over LAN"
            )),
            line(&rainbow_rule(96)),
            blank_line(),
            line(&section("UPLINK")),
            block(&render_endpoint_block(
                &self.info.name,
                &self.info.folder,
                &url,
                &format!("{}:{}", self.info.bind_host, self.info.port),
                mode,
                &self.auth_line()
            )),
            line(&section("TELEMETRY")),
            block(&render_metrics_block(metrics, upload_rate, download_rate)),
            line(&section("RECENT ACTIVITY")),
            block(&format!(
                "{}\n\n{}",
                format_activity(metrics),
                dim("Press Enter or Ctrl+C to stop.  Use --no-tui for plain output.")
            )),
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

struct TerminalSession;

impl TerminalSession {
    fn enter() -> Self {
        print!("\x1b[?1049h\x1b[?25l\x1b[2J\x1b[H");
        let _ = io::stdout().flush();
        Self
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        print!("\x1b[?25h\x1b[?1049l");
        let _ = io::stdout().flush();
    }
}

fn move_home() {
    print!("\x1b[H");
}

fn render_logo() -> String {
    let lines = [
        "██████╗   █████╗  ██╗   ██╗ ██████╗   ██████╗  ██╗  ██╗",
        "██╔══██╗ ██╔══██╗ ██║   ██║ ██╔══██╗ ██╔═══██╗ ╚██╗██╔╝",
        "██║  ██║ ███████║ ██║   ██║ ██████╔╝ ██║   ██║  ╚███╔╝ ",
        "██║  ██║ ██╔══██║ ╚██╗ ██╔╝ ██╔══██╗ ██║   ██║  ██╔██╗ ",
        "██████╔╝ ██║  ██║  ╚████╔╝  ██████╔╝ ╚██████╔╝ ██╔╝ ██╗",
        "╚═════╝  ╚═╝  ╚═╝   ╚═══╝   ╚═════╝   ╚═════╝  ╚═╝  ╚═╝",
    ];
    let colors = [36, 36, 32, 33, 35, 35];
    lines
        .iter()
        .zip(colors)
        .map(|(logo_line, color_code)| line(&color(logo_line, color_code)))
        .collect::<Vec<_>>()
        .join("")
}

fn render_endpoint_block(
    name: &str,
    folder: &str,
    url: &str,
    bind: &str,
    mode: &str,
    auth: &str,
) -> String {
    [
        kv("Name", name),
        kv("Folder", folder),
        kv("WebDAV", url),
        kv("Bind", bind),
        kv("Mode", mode),
        kv("Auth", auth),
    ]
    .join("\n")
}

fn render_metrics_block(metrics: &Metrics, upload_rate: f64, download_rate: f64) -> String {
    [
        metric("Uptime", &format_duration(metrics.uptime())),
        metric("Active req", &metrics.active_requests().to_string()),
        metric("Conn total", &metrics.total_connections().to_string()),
        metric("Requests", &metrics.total_requests().to_string()),
        metric(
            "Traffic",
            &format!(
                "up {}/s   down {}/s",
                format_bytes(upload_rate as u64),
                format_bytes(download_rate as u64)
            ),
        ),
        metric(
            "Totals",
            &format!(
                "in {}   out {}",
                format_bytes(metrics.total_bytes_in()),
                format_bytes(metrics.total_bytes_out())
            ),
        ),
    ]
    .join("\n")
}

fn format_activity(metrics: &Metrics) -> String {
    let mut lines = Vec::new();
    for entry in metrics.recent() {
        let status = if entry.status >= 500 {
            red(&entry.status.to_string())
        } else if entry.status >= 400 {
            yellow(&entry.status.to_string())
        } else if entry.status >= 300 {
            cyan(&entry.status.to_string())
        } else {
            green(&entry.status.to_string())
        };
        lines.push(format!(
            "  {}  {:<8} {}  {:<34} {:>9} {:>6}ms",
            dim("›"),
            magenta(&truncate(&entry.method, 8)),
            status,
            truncate(&entry.path, 34),
            format_bytes(entry.bytes_in + entry.bytes_out),
            entry.duration.as_millis()
        ));
    }
    while lines.len() < 8 {
        lines.push(format!("  {}", dim("·")));
    }
    lines.join("\n")
}

fn line(value: &str) -> String {
    format!("{value}\x1b[K\n")
}

fn block(value: &str) -> String {
    value.lines().map(line).collect::<String>() + &blank_line()
}

fn blank_line() -> String {
    "\x1b[K\n".to_string()
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

fn section(value: &str) -> String {
    format!("{} {}", yellow("▸"), bold(value))
}

fn kv(label: &str, value: &str) -> String {
    format!("  {} {:<10} {}", cyan("•"), dim(label), truncate(value, 72))
}

fn metric(label: &str, value: &str) -> String {
    format!("  {} {:<10} {}", green("◆"), dim(label), value)
}

fn rainbow_rule(width: usize) -> String {
    let palette = [36, 32, 33, 35, 31];
    (0..width)
        .map(|index| color("━", palette[index * palette.len() / width]))
        .collect::<String>()
}

fn bold(value: &str) -> String {
    format!("\x1b[1m{value}\x1b[0m")
}

fn dim(value: &str) -> String {
    color(value, 90)
}

fn cyan(value: &str) -> String {
    color(value, 36)
}

fn green(value: &str) -> String {
    color(value, 32)
}

fn yellow(value: &str) -> String {
    color(value, 33)
}

fn magenta(value: &str) -> String {
    color(value, 35)
}

fn red(value: &str) -> String {
    color(value, 31)
}

fn color(value: &str, code: u8) -> String {
    format!("\x1b[{code}m{value}\x1b[0m")
}
