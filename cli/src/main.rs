#![deny(unsafe_code)]

use args::ArgsVst3;
use build::{
    cargo_workspace_dir, codesign_bundle, notarize_bundle, plugin_system_folder, reflink,
    reload_audio_unit_cache, run_parallel, wait_unlink,
};
use cli::{Error, print_error, report_message, report_span};
use owo_colors::OwoColorize;

mod args;
mod build;
mod cli;

fn main() {
    print_error(|| {
        let args::Args {
            clap,
            auv2,
            vst3,
            mut build,
            codesign,
            verbose,
            install,
        } = args::parse_args();

        if verbose {
            cli::set_force_log(true);
        }

        let clap = clap || vst3 == ArgsVst3::None && !auv2;
        if build.packages.is_empty() {
            return Err(Error::new("no packages specified"));
        }

        if build.target.is_empty() {
            build.target.push(target_lexicon::HOST.to_string());
        }

        let workspace_dir = cargo_workspace_dir()?;
        let output_dir = workspace_dir.join("target").join("bundled");

        let build_request = build::BuildRequest {
            target_dir: workspace_dir.join("target"),
            packages: build.packages,
            profile: build.profile.unwrap_or("release".to_string()),

            targets: build
                .target
                .into_iter()
                .map(|x| x.parse())
                .collect::<Result<_, Error>>()?,

            features: build.features,
            all_features: build.all_features,
            no_default_features: build.no_default_features,

            clap,
            auv2,
            vst3: match vst3 {
                ArgsVst3::Gpl => Some(build::Vst3Sdk::OpenSource),
                ArgsVst3::Proprietary => Some(build::Vst3Sdk::Proprietary),
                ArgsVst3::None => None,
            },
        };

        let artifacts = build::build(&build_request)?;

        run_parallel(artifacts, |artifact| {
            report_span!(
                "copying {} {} ({}) to the output directory",
                artifact.format.print_name().bold(),
                artifact.package.bold(),
                artifact.target.to_string().bold()
            );

            let output_path = output_dir
                .join(artifact.target.to_string())
                .join(&artifact.package)
                .with_extension(artifact.format.extension());

            let _ = std::fs::create_dir_all(&output_path);
            wait_unlink(&output_path)?;
            reflink(&artifact.path, &output_path)?;

            if let Some(codesign) = codesign.as_ref() {
                codesign_bundle(&output_path, Some(&codesign.identity))?;
                notarize_bundle(
                    &output_path,
                    &codesign.team,
                    &codesign.username,
                    &codesign.password,
                )?;
            } else if cfg!(target_os = "macos") {
                codesign_bundle(&output_path, None)?;
            }

            if install && artifact.target.is_supported(&target_lexicon::HOST) {
                report_message!(
                    "installing {} {} ({})",
                    artifact.format.print_name().bold(),
                    artifact.package.bold(),
                    artifact.target.to_string().bold()
                );

                let install_path =
                    plugin_system_folder(artifact.format, target_lexicon::HOST.operating_system)?
                        .join("dev")
                        .join(&artifact.package)
                        .with_extension(artifact.format.extension());

                let _ = std::fs::create_dir_all(&install_path);
                wait_unlink(&install_path)?;
                reflink(&artifact.path, &install_path)?;
            }

            Ok(())
        })?;

        if install {
            reload_audio_unit_cache()?;
        }

        Ok(())
    });
}
