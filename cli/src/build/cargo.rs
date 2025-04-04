use crate::{
    cli::{Command, Error, Result, report_message},
    report_span,
};
use owo_colors::OwoColorize;
use std::{
    collections::HashMap,
    env::var,
    path::{Path, PathBuf},
    str::FromStr,
};
use target_lexicon::{Environment, OperatingSystem, Triple};
use tinyjson::JsonValue;

#[derive(Debug, Copy, Clone)]
pub enum CargoCrateType {
    Cdylib,
    Staticlib,
}

#[derive(Debug, Clone)]
pub struct CargoBuild {
    pub crate_type: CargoCrateType,
    pub target_dir: PathBuf,
    pub packages: Vec<String>,
    pub profile: String,
    pub target: Triple,
    pub features: Vec<String>,
    pub all_features: bool,
    pub no_default_features: bool,
}

pub fn cargo_build(build: CargoBuild) -> Result<HashMap<String, PathBuf>> {
    report_span!("compiling using cargo");

    let mut command = Command::new(&cargo_cmd());

    command = command.arg("rustc");
    command = command.arg("--lib");
    command = command.arg("--message-format=json-diagnostic-rendered-ansi");
    command = command.env("CARGO_TERM_PROGRESS_WHEN", "never");

    command = command.arg("--target-dir").arg(&build.target_dir);
    command = command.arg("--profile").arg(&build.profile);
    command = command.arg("--target").arg(build.target.to_string());

    for package in &build.packages {
        command = command.arg("-p").arg(package);
    }

    for feature in &build.features {
        command = command.arg("-F").arg(feature);
    }

    if build.all_features {
        command = command.arg("--all-features");
    }

    if build.no_default_features {
        command = command.arg("--no-default-features");
    }

    match build.crate_type {
        CargoCrateType::Cdylib => {
            command = command.arg("--crate-type=cdylib");
        }
        CargoCrateType::Staticlib => {
            command = command.arg("--crate-type=staticlib");
        }
    }

    // command = command.arg("--").arg("--print=native-static-libs");

    let mut compiler_messages = vec![];

    command
        .run_stdout_stderr(
            |line| {
                if line.starts_with('{') {
                    if let Ok(CargoMessage::CompilerMessage { message }) =
                        line.parse::<CargoMessage>()
                    {
                        compiler_messages.push(message);
                    }
                }
            },
            |line| {
                report_message!("{}", line.trim());
            },
        )
        .map_err(|e| {
            let message = if compiler_messages.is_empty() {
                "compilation error. check the cargo output in --verbose mode for more info"
                    .to_string()
            } else {
                compiler_messages.join("\n")
            };

            e.with_message(message)
                .with_note("compilation failure while running cargo")
        })?;

    let mut output = HashMap::new();
    for package in build.packages {
        let path = cargo_output_path(
            &build.target_dir,
            &build.profile,
            &build.target,
            &package,
            build.crate_type,
        )?;

        output.insert(package, path);
    }

    Ok(output)
}

pub fn cargo_workspace_dir() -> Result<PathBuf> {
    let path = Command::new(&cargo_cmd())
        .arg("locate-project")
        .arg("--workspace")
        .arg("--message-format=plain")
        .run()
        .map_err(|e| {
            e.with_note(format!(
                "make sure you run the bundler in a {} workspace",
                "cargo".bold()
            ))
            .with_note(format!(
                "make sure you have {} installed",
                "cargo".bright_cyan().bold()
            ))
        })?;

    Ok(PathBuf::from(path.trim())
        .parent()
        .ok_or_else(|| {
            Error::new(format!(
                "malformed path returned from `cargo locate-project`: {}",
                path
            ))
            .with_note(format!(
                "make sure you run the bundler in a {} workspace",
                "cargo".bright_cyan().bold()
            ))
        })?
        .to_path_buf())
}

pub fn cargo_metadata() -> Result<HashMap<String, JsonValue>> {
    fn parse_metadata(str: &str) -> Option<HashMap<String, JsonValue>> {
        let metadata = tinyjson::JsonValue::from_str(str).ok()?;
        if let Some(metadata) = metadata.get::<HashMap<String, JsonValue>>() {
            return Some(metadata.clone());
        }

        None
    }

    let output = Command::new(&cargo_cmd())
        .arg("metadata")
        .run()
        .map_err(|e| {
            e.with_note(format!(
                "make sure you run the bundler in a {} workspace",
                "cargo".bold()
            ))
            .with_note(format!(
                "make sure you have {} installed",
                "cargo".bright_cyan().bold()
            ))
        })?;

    let value = parse_metadata(&output)
        .ok_or_else(|| Error::new(format!("malformed output from {}", "cargo metadata".bold())))?;

    Ok(value)
}

fn cargo_cmd() -> String {
    var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

enum CargoMessage {
    CompilerMessage { message: String },
    Other,
}

impl FromStr for CargoMessage {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, ()> {
        let value = tinyjson::JsonValue::from_str(s)
            .ok()
            .and_then(|x| match x {
                JsonValue::Object(x) => Some(x),
                _ => None,
            })
            .ok_or(())?;

        let reason = value
            .get("reason")
            .and_then(|x| x.get::<String>())
            .map(|x| x.as_str())
            .unwrap_or_default();

        if reason == "compiler-message" {
            let message = value["message"]["rendered"]
                .get::<String>()
                .map(|x| x.as_str())
                .ok_or(())?;

            Ok(CargoMessage::CompilerMessage {
                message: message.to_string(),
            })
        } else {
            Ok(CargoMessage::Other)
        }
    }
}

fn cargo_output_path(
    target: &Path,
    profile: &str,
    triple: &Triple,
    package_name: &str,
    crate_type: CargoCrateType,
) -> Result<PathBuf> {
    let package_name = package_name.replace("-", "_");
    let filename = match (triple.operating_system, triple.environment, crate_type) {
        (OperatingSystem::Linux, _, CargoCrateType::Cdylib) => {
            format!("lib{}.so", package_name)
        }
        (OperatingSystem::Linux, _, CargoCrateType::Staticlib) => {
            format!("lib{}.a", package_name)
        }
        (OperatingSystem::MacOSX(_), _, CargoCrateType::Cdylib)
        | (OperatingSystem::Darwin(_), _, CargoCrateType::Cdylib) => {
            format!("lib{}.dylib", package_name)
        }
        (OperatingSystem::MacOSX(_), _, CargoCrateType::Staticlib)
        | (OperatingSystem::Darwin(_), _, CargoCrateType::Staticlib) => {
            format!("lib{}.a", package_name)
        }
        (OperatingSystem::Windows, _, CargoCrateType::Cdylib) => {
            format!("{}.dll", package_name)
        }
        (OperatingSystem::Windows, Environment::Msvc, CargoCrateType::Staticlib) => {
            format!("{}.lib", package_name)
        }
        (OperatingSystem::Windows, _, CargoCrateType::Staticlib) => {
            format!("lib{}.a", package_name)
        }
        _ => {
            return Err(Error::new(format!(
                "unsupported target: {} ",
                triple.operating_system,
            )));
        }
    };

    let profile_dir = match profile {
        "release" | "bench" => "release",
        "dev" | "test" => "debug",
        x => x,
    };

    Ok(target
        .join(triple.to_string())
        .join(profile_dir)
        .join(filename))
}
