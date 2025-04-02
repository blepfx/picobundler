use crate::build::{unzip_archive, wait_unlink, zip_archive};
use crate::cli::{Command, Result};
use crate::{report_message, report_span};
use owo_colors::OwoColorize;
use std::path::Path;

pub fn codesign_bundle(bundle: &Path, identity: Option<&str>) -> Result<()> {
    match identity {
        Some(identity) => {
            report_span!("signing bundle {} with identity", bundle.display().bold());

            Command::new("codesign")
                .arg("--force")
                .arg("--timestamp")
                .arg("--deep")
                .arg("--options=runtime")
                .arg("--strict")
                .arg("-s")
                .arg_secret(identity)
                .arg("-v")
                .arg(bundle)
                .run_stdout(|line| {
                    report_message!("{}", line);
                })
        }
        None => {
            report_span!("ad-hoc signing bundle: {}", bundle.display().bold());

            Command::new("codesign")
                .arg("--force")
                .arg("--timestamp")
                .arg("--deep")
                .arg("-s")
                .arg("-v")
                .arg("-")
                .arg(bundle)
                .run_stdout(|line| {
                    report_message!("{}", line);
                })
        }
    }
}

pub fn lipo(inputs: &[&Path], target: &Path) -> Result<()> {
    report_span!("bundling a fat binary: {}", target.display().bold());

    Command::new("lipo")
        .arg("-create")
        .arg("-output")
        .arg(target)
        .args(inputs)
        .run_stdout(|line| {
            report_message!("{}", line);
        })
}

pub fn reload_audio_unit_cache() -> Result<()> {
    if !cfg!(target_os = "macos") {
        return Ok(());
    }

    report_span!("reloading audio unit registrar");

    let _ = Command::new("killall")
        .arg("-9")
        .arg("AudioComponentRegistrar")
        .run_stdout(|line| {
            report_message!("{}", line);
        });

    let _ = Command::new("auval").arg("-a").run_stdout(|line| {
        report_message!("{}", line);
    });

    Ok(())
}

pub fn validate_audio_unit(
    code_type: &str,
    code_subtype: &str,
    code_manufacturer: &str,
) -> Result<()> {
    if !cfg!(target_os = "macos") {
        return Ok(());
    }

    report_span!(
        "validating audio unit {} {} {}",
        code_type.bold(),
        code_manufacturer.bold(),
        code_subtype.bold(),
    );

    Command::new("auval")
        .arg("-strict")
        .arg("-v")
        .arg(code_type)
        .arg(code_subtype)
        .arg(code_manufacturer)
        .run_stdout(|line| {
            report_message!("{}", line);
        })
}

pub fn notarize_bundle(bundle: &Path, team: &str, username: &str, password: &str) -> Result<()> {
    report_span!("notarizing bundle {}", bundle.display().bold());

    let archive = bundle.with_file_name({
        let mut file = bundle.file_name().unwrap_or_default().to_os_string();
        file.push(".zip");
        file
    });

    zip_archive(bundle, &archive)?;

    {
        report_span!("submitting archive to apple");
        Command::new("xcrun")
            .arg("notarytool")
            .arg("submit")
            .arg(&archive)
            .arg("--apple-id")
            .arg_secret(username)
            .arg("--password")
            .arg_secret(password)
            .arg("--team-id")
            .arg_secret(team)
            .arg("--wait")
            .run_stdout(|line| {
                report_message!("{}", line);
            })?;
    }

    wait_unlink(bundle)?;
    unzip_archive(&archive, bundle)?;
    wait_unlink(&archive)?;

    {
        report_span!("stapling notarization to bundle");
        Command::new("xcrun")
            .arg("stapler")
            .arg("staple")
            .arg(bundle)
            .run_stdout(|line| {
                report_message!("{}", line);
            })?;
    }

    Ok(())
}
