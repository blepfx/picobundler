use crate::{
    cli::{Command, Error, Result, report_span},
    report_message,
};
use owo_colors::OwoColorize;
use std::path::PathBuf;

pub struct ClapWrapperOptions {
    pub cmake_dir: PathBuf,
    pub build_dir: PathBuf,

    pub package_name: String,
    pub static_lib: PathBuf,

    pub native_static_libs: Option<String>,
    pub zig_triple: Option<String>,
    pub osx_arch: Option<String>,

    pub vst3: Option<PathBuf>,
    pub auv2: bool,
}

pub struct ClapWrapperOutput {
    pub clap: PathBuf,
    pub vst3: Option<PathBuf>,
    pub auv2: Option<PathBuf>,
}

pub fn build_wrapper(options: ClapWrapperOptions) -> Result<ClapWrapperOutput> {
    report_span!("wrapping via {}", "clap-wrapper".bold());

    let build_dir = options.build_dir.join(match &options.zig_triple {
        Some(triple) => triple.clone(),
        None => "host".to_string(),
    });

    let _ = std::fs::create_dir_all(&build_dir);

    #[rustfmt::skip]
    let envs = vec![ 
        ("PICO_PLUGIN_STATIC_LIB", options.static_lib.into_os_string()),
        ("PICO_PLUGIN_NAME", options.package_name.clone().into()),
        ("PICO_PLUGIN_WANT_AUV2", if options.auv2 && options.osx_arch.is_some() { "AUV2" } else { "" }.into()),
        ("PICO_PLUGIN_WANT_VST3", options.vst3.is_some().then_some("VST3").unwrap_or_default().into()),
        ("PICO_SDK_VST3", options.vst3.clone().map(|v| v.into_os_string()).unwrap_or_default()),
        ("PICO_BUILD_ZIG_TARGET", options.zig_triple.map(|v| v.into()).unwrap_or_default()),
        ("PICO_BUILD_OSX_ARCH", options.osx_arch.clone().map(|v| v.into()).unwrap_or_default()),
        ("PICO_BUILD_TYPE", "Release".into()), //TODO: use cargo profile to set this
        ("PICO_BUILD_NATIVE_STATIC_LIBS", options.native_static_libs.map(format_native_static_libs).unwrap_or_default().into()),
    ];

    Command::new("cmake")
        .arg(&options.cmake_dir)
        .cwd(&build_dir)
        .envs(envs.iter().map(|(k, v)| (k, v.as_os_str())))
        .run_stdout_stderr(
            |line| {
                report_message!("{}", line);
            },
            |line| {
                report_message!("{}", line);
            },
        )
        .map_err(|e| {
            e.with_message(format!(
                "failed to configure {}. check the output in --verbose mode for more info",
                "clap-wrapper".bold()
            ))
        })?;

    Command::new("cmake")
        .arg("--build")
        .arg(".")
        .cwd(&build_dir)
        .envs(envs.iter().map(|(k, v)| (k, v.as_os_str())))
        .run_stdout_stderr(
            |line| {
                report_message!("{}", line);
            },
            |line| {
                report_message!("{}", line);
            },
        )
        .map_err(|e| {
            e.with_message(format!(
                "failed to build {}. check the output in --verbose mode for more info",
                "clap-wrapper".bold()
            ))
        })?;

    let output = build_dir
        .join("clap-wrapper-output")
        .join(options.package_name.clone())
        .join(options.package_name.clone());

    Ok(ClapWrapperOutput {
        clap: output.with_extension("clap"),
        vst3: options.vst3.map(|_| output.with_extension("vst3")),
        auv2: if options.auv2 && options.osx_arch.is_some() {
            Some(output.with_extension("component"))
        } else {
            None
        },
    })
}

pub fn ensure_cmake_installed() -> Result<()> {
    Command::new("cmake").arg("--version").run().map_err(|_| {
        Error::new(format!(
            "vst3/auv2 bundling requires {} to be installed",
            "cmake".bold()
        ))
        .with_note(format!(
            "you can install {} from https://cmake.org",
            "cmake".bold()
        ))
    })?;

    Ok(())
}

fn format_native_static_libs(native_static_libs: String) -> String {
    let mut libs = Vec::new();
    let mut s_framework = false;

    for arg in native_static_libs.split_whitespace() {
        if s_framework {
            libs.push(format!("-framework {}", arg));
            s_framework = false;
        } else {
            if arg == "-framework" {
                s_framework = true;
                continue;
            }

            let arg = arg.strip_prefix("-l").unwrap_or(arg);
            let arg = arg.strip_suffix(".lib").unwrap_or(arg);
            libs.push(arg.to_string());
        }
    }

    libs.join(";")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_native_static_libs() {
        let input = "-lobjc -framework CoreFoundation -liconv -lSystem -lc -lm";
        let expected = "objc;-framework CoreFoundation;iconv;System;c;m";
        assert_eq!(format_native_static_libs(input.to_string()), expected);
    }
}
