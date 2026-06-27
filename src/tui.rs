use std::io::{self, Read, Write};
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::time::Duration;

use crate::core::events::{Metrics, ServerEvent};
use crate::core::server::ServerInfo;

const AUTHOR_ID: &str = "stigachen";
const COPYRIGHT_YEAR: &str = "2026";

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
        if !redraw_static_header_each_frame() {
            self.render_static_header();
        }

        loop {
            while let Ok(event) = events.try_recv() {
                metrics.apply(event);
            }
            self.render_dynamic_dashboard(&metrics);
            if quit.try_recv().is_ok() {
                break;
            }
            thread::sleep(Duration::from_millis(self.info.tui_refresh_ms));
        }
    }

    fn render_plain(&self) {
        let url = format!("http://{}:{}/", self.info.display_host, self.info.port);
        println!("davbox online: {url}");
        println!("{}", product_meta_line());
        println!("Press Enter or Ctrl+C to stop.");
    }

    fn render_static_header(&self) {
        print!("{}", render_static_header(HeaderStyle::Full));
        let _ = io::stdout().flush();
    }

    fn render_dynamic_dashboard(&self, metrics: &Metrics) {
        if redraw_static_header_each_frame() {
            move_to_screen_start();
            print!("{}", self.render_dashboard_frame(metrics));
            print!("{}", clear_to_screen_end_sequence());
            let _ = io::stdout().flush();
            return;
        } else {
            move_to_dynamic_start();
        }
        print!(
            "{}{}",
            self.render_dynamic_dashboard_content(metrics, DashboardDensity::Full),
            clear_to_screen_end_sequence(),
        );
        let _ = io::stdout().flush();
    }

    fn render_dashboard_frame(&self, metrics: &Metrics) -> String {
        let header_style = windows_header_style();
        let density = windows_dashboard_density(header_style);
        format!(
            "{}{}",
            render_static_header(header_style),
            self.render_dynamic_dashboard_content(metrics, density),
        )
    }

    fn render_dynamic_dashboard_content(
        &self,
        metrics: &Metrics,
        density: DashboardDensity,
    ) -> String {
        let url = format!("http://{}:{}/", self.info.display_host, self.info.port);
        let (upload_rate, download_rate) = metrics.transfer_rates();
        let mode = if self.info.read_only {
            "read-only"
        } else {
            "read-write"
        };

        let endpoint = render_endpoint_block(
            &self.info.name,
            &self.info.folder,
            &url,
            &format!("{}:{}", self.info.bind_host, self.info.port),
            mode,
            &self.auth_line(),
        );
        let telemetry = render_metrics_block(metrics, upload_rate, download_rate);
        let help = render_help_line(density);
        let activity = match density {
            DashboardDensity::Full => format!("{}\n\n{}", format_activity(metrics, 8), help),
            DashboardDensity::Compact => format!("{}\n{}", format_activity(metrics, 1), help),
        };

        match density {
            DashboardDensity::Full => format!(
                "{}{}{}{}{}{}",
                line(&section("UPLINK")),
                block(&endpoint),
                line(&section("TELEMETRY")),
                block(&telemetry),
                line(&section("RECENT ACTIVITY")),
                block(&activity),
            ),
            DashboardDensity::Compact => format!(
                "{}{}{}{}{}{}",
                line(&section("UPLINK")),
                compact_block(&endpoint),
                line(&section("TELEMETRY")),
                compact_block(&telemetry),
                line(&section("RECENT ACTIVITY")),
                compact_block(&activity),
            ),
        }
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

fn move_to_screen_start() {
    print!("\x1b[H");
}

fn move_to_dynamic_start() {
    print!("\x1b[{};1H", dynamic_start_row());
}

fn clear_to_screen_end_sequence() -> &'static str {
    "\x1b[J"
}

#[cfg(not(windows))]
fn render_logo_for_style(style: HeaderStyle) -> String {
    render_text_logo_lines(render_logo_lines(style))
}

fn render_text_logo_lines(lines: &[&str]) -> String {
    lines
        .iter()
        .zip([36, 36, 32, 33, 35, 35].into_iter().cycle())
        .map(|(logo_line, color_code)| line(&color(logo_line, color_code)))
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(windows)]
fn render_logo_for_style(style: HeaderStyle) -> String {
    match style {
        HeaderStyle::Full => render_text_logo_lines(full_logo_lines()),
        HeaderStyle::Compact => render_windows_pixel_logo(),
    }
}

#[cfg(windows)]
fn render_windows_pixel_logo() -> String {
    let rows = [
        "####   ###  #   # ####   ###  #   #",
        "#   # #   # #   # #   # #   # #   #",
        "#   # ##### #   # ####  #   #  # # ",
        "#   # #   #  # #  #   # #   # #   #",
        "####  #   #   #   ####   ###  #   #",
    ];
    rows.iter()
        .enumerate()
        .map(|(row_index, row)| render_pixel_logo_row(row, row_index))
        .collect::<String>()
}

#[cfg(windows)]
fn render_pixel_logo_row(row: &str, row_index: usize) -> String {
    let mut out = String::new();
    let previous = if row_index == 0 {
        None
    } else {
        Some(WINDOWS_PIXEL_LOGO_ROWS[row_index - 1])
    };

    for (column, pixel) in row.chars().enumerate() {
        if pixel == '#' {
            let color_code = windows_logo_color(column);
            out.push_str(&format!("\x1b[48;5;{color_code}m  \x1b[0m"));
        } else if previous.is_some_and(|line| line.as_bytes().get(column) == Some(&b'#')) {
            let color_code = windows_logo_shadow_color(column);
            out.push_str(&format!("\x1b[48;5;{color_code}m  \x1b[0m"));
        } else {
            out.push_str("  ");
        }
    }
    out.push_str("\x1b[K\n");
    out
}

#[cfg(windows)]
const WINDOWS_PIXEL_LOGO_ROWS: &[&str] = &[
    "####   ###  #   # ####   ###  #   #",
    "#   # #   # #   # #   # #   # #   #",
    "#   # ##### #   # ####  #   #  # # ",
    "#   # #   #  # #  #   # #   # #   #",
    "####  #   #   #   ####   ###  #   #",
];

#[cfg(windows)]
fn windows_logo_color(column: usize) -> u8 {
    const PALETTE: &[u8] = &[39, 45, 82, 118, 226, 214, 207, 171, 129, 201, 196];
    PALETTE[column * PALETTE.len() / WINDOWS_PIXEL_LOGO_ROWS[0].len()]
}

#[cfg(windows)]
fn windows_logo_shadow_color(column: usize) -> u8 {
    const PALETTE: &[u8] = &[24, 25, 28, 58, 94, 95, 54, 90, 89, 125, 52];
    PALETTE[column * PALETTE.len() / WINDOWS_PIXEL_LOGO_ROWS[0].len()]
}

fn render_static_header(style: HeaderStyle) -> String {
    format!(
        "{}{}{}{}",
        render_logo_for_style(style),
        line(&magenta(
            "             local folder uplink // WebDAV over LAN"
        )),
        line(&rainbow_rule(96)),
        header_trailing_space(style),
    )
}

fn header_trailing_space(style: HeaderStyle) -> &'static str {
    match style {
        HeaderStyle::Full => "\x1b[K\n",
        HeaderStyle::Compact => "",
    }
}

fn dynamic_start_row() -> usize {
    static_header_rows() + 1
}

fn static_header_rows() -> usize {
    render_static_header(HeaderStyle::Full).lines().count()
}

#[derive(Clone, Copy)]
enum HeaderStyle {
    Full,
    Compact,
}

#[derive(Clone, Copy)]
enum DashboardDensity {
    Full,
    Compact,
}

#[cfg(windows)]
fn redraw_static_header_each_frame() -> bool {
    true
}

#[cfg(not(windows))]
fn redraw_static_header_each_frame() -> bool {
    false
}

#[cfg(windows)]
fn windows_header_style() -> HeaderStyle {
    HeaderStyle::Compact
}

#[cfg(not(windows))]
fn windows_header_style() -> HeaderStyle {
    HeaderStyle::Full
}

#[cfg(windows)]
fn windows_dashboard_density(style: HeaderStyle) -> DashboardDensity {
    match style {
        HeaderStyle::Full => DashboardDensity::Full,
        HeaderStyle::Compact => DashboardDensity::Compact,
    }
}

#[cfg(not(windows))]
fn windows_dashboard_density(_style: HeaderStyle) -> DashboardDensity {
    DashboardDensity::Full
}

#[cfg(not(windows))]
fn render_logo_lines(style: HeaderStyle) -> &'static [&'static str] {
    match style {
        HeaderStyle::Full => full_logo_lines(),
        HeaderStyle::Compact => compact_logo_lines(),
    }
}

fn full_logo_lines() -> &'static [&'static str] {
    &[
        "██████╗   █████╗  ██╗   ██╗ ██████╗   ██████╗  ██╗  ██╗",
        "██╔══██╗ ██╔══██╗ ██║   ██║ ██╔══██╗ ██╔═══██╗ ╚██╗██╔╝",
        "██║  ██║ ███████║ ██║   ██║ ██████╔╝ ██║   ██║  ╚███╔╝ ",
        "██║  ██║ ██╔══██║ ╚██╗ ██╔╝ ██╔══██╗ ██║   ██║  ██╔██╗ ",
        "██████╔╝ ██║  ██║  ╚████╔╝  ██████╔╝ ╚██████╔╝ ██╔╝ ██╗",
        "╚═════╝  ╚═╝  ╚═╝   ╚═══╝   ╚═════╝   ╚═════╝  ╚═╝  ╚═╝",
    ]
}

#[cfg(any(not(windows), test))]
fn compact_logo_lines() -> &'static [&'static str] {
    &[
        "████▄    █████   ██╗  ██╗ █████▄   █████   ██╗  ██╗",
        "██╔═██╗ ██╔══██╗ ██║  ██║ ██╔═██╗ ██╔══██╗ ╚██╗██╔╝",
        "█████╔╝ ██║  ██║ ╚████╔╝  █████╔╝ ╚█████╔╝ ██╔╝ ██╗",
    ]
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

fn format_activity(metrics: &Metrics, rows: usize) -> String {
    let mut lines = Vec::new();
    for entry in metrics.recent().take(rows) {
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
    while lines.len() < rows {
        lines.push(format!("  {}", dim("·")));
    }
    lines.join("\n")
}

fn render_help_line(density: DashboardDensity) -> String {
    match density {
        DashboardDensity::Full => dim(&format!(
            "Press Enter or Ctrl+C to stop.  Use --no-tui for plain output.\n\n{}",
            product_meta_line()
        )),
        DashboardDensity::Compact => dim(&format!("Enter/Ctrl+C stop · {}", product_meta_line())),
    }
}

fn product_meta_line() -> String {
    format!(
        "Davbox {} · {} · © {} {}",
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_LICENSE"),
        COPYRIGHT_YEAR,
        AUTHOR_ID
    )
}

fn line(value: &str) -> String {
    format!("{value}\x1b[K\n")
}

fn block(value: &str) -> String {
    value.lines().map(line).collect::<String>() + &blank_line()
}

fn compact_block(value: &str) -> String {
    value.lines().map(line).collect::<String>()
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
        .map(|index| color(rule_glyph(), palette[index * palette.len() / width]))
        .collect::<String>()
}

#[cfg(windows)]
fn rule_glyph() -> &'static str {
    "━"
}

#[cfg(not(windows))]
fn rule_glyph() -> &'static str {
    "━"
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

#[cfg(test)]
mod tests {
    use crate::core::events::Metrics;
    use crate::core::server::ServerInfo;

    use super::{
        ConsoleUi, DashboardDensity, HeaderStyle, compact_logo_lines, dynamic_start_row,
        product_meta_line, render_static_header, static_header_rows, windows_dashboard_density,
        windows_header_style,
    };

    #[test]
    fn static_header_row_count_matches_dynamic_start() {
        assert_eq!(
            render_static_header(HeaderStyle::Full).lines().count(),
            static_header_rows()
        );
        assert_eq!(dynamic_start_row(), static_header_rows() + 1);
    }

    #[test]
    fn full_header_keeps_original_row_budget() {
        let header = render_static_header(HeaderStyle::Full);
        assert_eq!(header.lines().count(), 9);
        assert!(header.contains("local folder uplink // WebDAV over LAN"));
        assert!(header.contains("██████"));
    }

    #[test]
    fn compact_header_keeps_logo_without_extra_blank_row() {
        let header = render_static_header(HeaderStyle::Compact);
        let expected_logo_rows = if cfg!(windows) {
            5
        } else {
            compact_logo_lines().len()
        };
        assert_eq!(header.lines().count(), expected_logo_rows + 2);
        assert!(header.contains("local folder uplink // WebDAV over LAN"));
        if cfg!(windows) {
            assert!(header.contains("\x1b[48;5;"));
        } else {
            assert!(header.contains("████▄"));
        }
    }

    #[test]
    fn compact_dashboard_frame_fits_common_windows_terminal_height() {
        let ui = ConsoleUi::new(test_server_info());
        let frame = format!(
            "{}{}",
            render_static_header(HeaderStyle::Compact),
            ui.render_dynamic_dashboard_content(&Metrics::new(), DashboardDensity::Compact),
        );

        if cfg!(windows) {
            assert!(frame.contains("\x1b[48;5;"));
        } else {
            assert!(frame.contains("DAVBOX") || frame.contains("████▄"));
        }
        assert!(frame.contains("UPLINK"));
        assert!(frame.contains("TELEMETRY"));
        assert!(frame.contains("RECENT ACTIVITY"));
        assert!(
            frame.lines().count() <= 24,
            "compact dashboard should not scroll a typical 24-row terminal, got {} rows",
            frame.lines().count()
        );
    }

    #[test]
    fn full_dashboard_keeps_eight_activity_rows() {
        let ui = ConsoleUi::new(test_server_info());
        let content = ui.render_dynamic_dashboard_content(&Metrics::new(), DashboardDensity::Full);
        let placeholder_rows = content.matches("\x1b[90m·\x1b[0m").count();

        assert_eq!(placeholder_rows, 8);
        assert!(content.contains("Press Enter or Ctrl+C to stop."));
        assert!(content.contains("Davbox 0.1.3"));
        assert!(content.contains("MIT"));
        assert!(content.contains("stigachen"));
    }

    #[test]
    fn product_meta_line_includes_build_and_owner_details() {
        assert_eq!(product_meta_line(), "Davbox 0.1.3 · MIT · © 2026 stigachen");
    }

    #[cfg(windows)]
    #[test]
    fn windows_uses_compact_first_frame() {
        assert!(matches!(windows_header_style(), HeaderStyle::Compact));
        assert!(matches!(
            windows_dashboard_density(windows_header_style()),
            DashboardDensity::Compact
        ));
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_keeps_full_first_frame() {
        assert!(matches!(windows_header_style(), HeaderStyle::Full));
        assert!(matches!(
            windows_dashboard_density(windows_header_style()),
            DashboardDensity::Full
        ));
    }

    fn test_server_info() -> ServerInfo {
        ServerInfo {
            folder: "build\\".to_string(),
            bind_host: "0.0.0.0".to_string(),
            display_host: "10.18.222.214".to_string(),
            port: 8080,
            name: "Davbox".to_string(),
            read_only: false,
            auth_enabled: true,
            username: "davbox".to_string(),
            password: Some("1234-5678".to_string()),
            tui_enabled: true,
            tui_refresh_ms: 750,
        }
    }
}
