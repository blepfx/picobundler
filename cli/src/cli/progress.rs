use console::{Term, truncate_str};
use owo_colors::OwoColorize;
use std::fmt::Write;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

static FORCE_LOG: AtomicBool = AtomicBool::new(false);

pub enum Event {
    Begin(String),
    Message(String),
    End,
    Update,
}

pub fn set_force_log(force: bool) {
    FORCE_LOG.store(force, Ordering::Relaxed);
}

pub fn report(event: Event) {
    ensure_update_thread();
    draw_string(generate_status_bar, || generate_event(event));
}

fn draw_string(supported: impl FnOnce() -> String, unsupported: impl FnOnce() -> String) {
    static LAST_LINES: AtomicU32 = AtomicU32::new(0);

    let _lock = std::io::stderr().lock();
    let stderr = Term::stderr();

    let width = stderr.size().1;
    if FORCE_LOG.load(Ordering::Relaxed) || !stderr.is_term() || width < 20 {
        let string = unsupported();
        if string.is_empty() {
            return;
        }

        for line in string.lines() {
            let _ = stderr.write_line(line);
        }
        return;
    }

    let string = supported();
    let line_count = string.lines().count();

    let _ = stderr.clear_last_lines(LAST_LINES.swap(line_count as u32, Ordering::Relaxed) as usize);
    for line in string.lines() {
        let _ = stderr.write_str(&truncate_str(line, width as usize, "..."));
        let _ = stderr.write_line("\x1b[0m");
    }
}

fn generate_event(event: Event) -> String {
    match event {
        Event::Begin(message) => format!("{} {}", ">".bright_blue().bold(), message),
        Event::Message(message) => format!("  {}", message),
        _ => String::new(),
    }
}

fn generate_status_bar() -> String {
    let spinner = {
        let time = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            / 250) as usize;

        const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        SPINNER[time % SPINNER.len()]
    };

    let mut string = String::new();
    super::trace::StatusReporter::get().request_trace_all(|_, stack| {
        for (i, line) in stack.iter().enumerate() {
            let spinner = if i == stack.len() - 1 { spinner } else { ' ' };
            let _ = writeln!(string, "{} {}", spinner.bright_blue().bold(), line.message);
        }
    });

    string
}

fn ensure_update_thread() {
    static UPDATE_THREAD: OnceLock<()> = OnceLock::new();
    UPDATE_THREAD.get_or_init(|| {
        std::thread::spawn(|| {
            loop {
                std::thread::sleep(Duration::from_millis(250));
                report(Event::Update);
            }
        });
    });
}
