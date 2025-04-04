use owo_colors::OwoColorize;
use std::{
    fmt::Display,
    io::{self, Write},
};

#[derive(Clone)]
pub struct Error(Box<ErrorImpl>);
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn new<T: Display>(message: T) -> Self {
        let message = message.to_string();
        let trace = super::trace::StatusReporter::get()
            .request_trace(std::thread::current().id(), |x| {
                x.iter().map(|x| x.span.to_string()).collect()
            })
            .unwrap_or_default();

        Self(Box::new(ErrorImpl {
            message,
            trace,
            note: vec![],
        }))
    }

    pub fn with_note<T: Display>(mut self, note: T) -> Self {
        self.0.note.push(note.to_string());
        self
    }

    pub fn with_message<T: Display>(mut self, message: T) -> Self {
        self.0.message = message.to_string();
        self
    }
}

impl<T: std::fmt::Display> From<T> for Error {
    fn from(value: T) -> Self {
        Self::new(value.to_string())
    }
}

pub fn print_error(cb: impl FnOnce() -> Result<()>) -> ! {
    match cb() {
        Ok(()) => {
            let _ = io::stdout().flush();
            let _ = io::stderr().flush();

            std::process::exit(0);
        }
        Err(e) => {
            let ErrorImpl {
                message,
                trace,
                note,
            } = *e.0;

            eprintln!("{}: {}", "error".bright_red().bold(), message.bold());

            if !trace.is_empty() {
                eprintln!("  {} caused by:", "-->".bright_blue().bold());

                for line in trace.iter().rev() {
                    eprintln!("   {}\t{}", "|".bright_blue().bold(), line);
                }
            }

            for note in note.iter() {
                eprintln!(
                    "   {} {}: {}",
                    "=".bright_blue().bold(),
                    "note".bold(),
                    note
                );
            }

            let _ = io::stdout().flush();
            let _ = io::stderr().flush();

            std::process::exit(1);
        }
    }
}

#[derive(Clone)]
struct ErrorImpl {
    message: String,
    trace: Vec<String>,
    note: Vec<String>,
}
