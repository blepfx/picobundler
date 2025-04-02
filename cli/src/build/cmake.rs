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
        ("PICO_PLUGIN_STATICLIB", options.static_lib.into_os_string()),
        ("PICO_PLUGIN_NAME", options.package_name.clone().into()),
        ("PICO_PLUGIN_WANT_AUV2", if options.auv2 && options.osx_arch.is_some() { "AUV2" } else { "" }.into()),
        ("PICO_PLUGIN_WANT_VST3", options.vst3.is_some().then_some("VST3").unwrap_or_default().into()),
        ("PICO_SDK_VST3", options.vst3.clone().map(|v| v.into_os_string()).unwrap_or_default()),
        ("PICO_BUILD_ZIG_TARGET", options.zig_triple.map(|v| v.into()).unwrap_or_default()),
        ("PICO_BUILD_OSX_ARCH", options.osx_arch.clone().map(|v| v.into()).unwrap_or_default()),
    ];

    Command::new("cmake")
        .arg(&options.cmake_dir)
        .cwd(&build_dir)
        .envs(envs.iter().map(|(k, v)| (k, v.as_os_str())))
        .run_stdout(|line| {
            report_message!("{}", line);
        })?;

    Command::new("cmake")
        .arg("--build")
        .arg(".")
        .cwd(&build_dir)
        .envs(envs.iter().map(|(k, v)| (k, v.as_os_str())))
        .run_stdout(|line| {
            report_message!("{}", line);
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
