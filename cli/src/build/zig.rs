use crate::cli::{Command, Error, Result};
use owo_colors::OwoColorize;
use target_lexicon::{OperatingSystem, Triple};

pub fn zig_triple(triple: &Triple, glibc: Option<&str>) -> Result<String> {
    let mut target = String::new();

    target.push_str(&triple.architecture.to_string());
    target.push('-');

    match triple.operating_system {
        OperatingSystem::Linux => {
            target.push_str("linux");
        }
        OperatingSystem::Darwin(_) | OperatingSystem::MacOSX(_) => {
            target.push_str("macos");
        }
        OperatingSystem::Windows => {
            target.push_str("windows");
        }
        _ => {
            return Err(
                Error::new(format!("unsupported target: {}", triple)).with_note(
                    "unsupported operating system: only linux, macos, and windows are supported",
                ),
            );
        }
    };

    match triple.environment {
        target_lexicon::Environment::Gnu => {
            target.push('-');
            target.push_str("gnu");
        }
        target_lexicon::Environment::Musl => {
            target.push('-');
            target.push_str("musl");
        }
        target_lexicon::Environment::Msvc => {
            target.push('-');
            target.push_str("msvc");
        }
        target_lexicon::Environment::Unknown => {}
        _ => {
            return Err(Error::new(format!("unsupported target: {}", triple))
                .with_note("unsupported environment: only gnu, musl and msvc are supported"));
        }
    };

    if let Some(glibc) = glibc {
        target.push('.');
        target.push_str(&glibc);
    }

    Ok(target)
}

pub fn ensure_zig_installed() -> Result<()> {
    let zig_version = Command::new("zig").arg("version").run().unwrap_or_default();

    let minor = zig_version
        .trim()
        .split(".")
        .nth(1)
        .unwrap_or_default()
        .parse::<u32>()
        .unwrap_or_default();

    if minor < 14 {
        return Err(Error::new(format!(
            "cross compilation requires {} (>= 0.14.0) to be installed",
            "zig".bold()
        ))
        .with_note(format!(
            "you can install {} from https://ziglang.org",
            "zig".bold()
        )));
    }

    Ok(())
}
