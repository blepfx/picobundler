mod apple;
mod cache;
mod cargo;
mod cmake;
mod util;
mod zig;

pub use apple::*;
pub use cargo::*;
pub use util::*;

use crate::{
    cli::{Error, Result},
    report_span,
};
use cache::{Dependency, DependencyCache};
use cmake::{ClapWrapperOptions, build_wrapper, ensure_cmake_installed};
use owo_colors::OwoColorize;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    str::FromStr,
};
use std::{fmt::Display, panic::resume_unwind};
use target_lexicon::{OperatingSystem, Triple};
use tinyjson::JsonValue;
use zig::{ensure_zig_installed, zig_triple};

#[derive(Debug, Clone)]
pub enum Vst3Sdk {
    OpenSource,
    Proprietary,
    Local(PathBuf),
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum BuildTarget {
    Triple(Triple),
    TripleGlibc(Triple, String),
    AppleUniversal,
}

impl BuildTarget {
    pub fn is_supported(&self, triple: &Triple) -> bool {
        match self {
            Self::Triple(target) => {
                target.architecture == triple.architecture
                    && target.operating_system == triple.operating_system
            }
            Self::TripleGlibc(target, _) => {
                target.architecture == triple.architecture
                    && target.operating_system == triple.operating_system
            }
            Self::AppleUniversal => matches!(
                triple.operating_system,
                OperatingSystem::Darwin(_) | OperatingSystem::MacOSX(_)
            ),
        }
    }
}

impl Display for BuildTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Triple(triple) => write!(f, "{}", triple),
            Self::TripleGlibc(triple, glibc) => write!(f, "{}.{}", triple, glibc),
            Self::AppleUniversal => write!(f, "universal-apple-darwin"),
        }
    }
}

impl FromStr for BuildTarget {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        if s.eq_ignore_ascii_case("universal-apple-darwin") {
            Ok(Self::AppleUniversal)
        } else if let Some(index) = s.find("gnu.") {
            Ok(Self::TripleGlibc(
                Triple::from_str(&s[..index + 3])
                    .map_err(|_| Error::new(format!("unknown target triple {}", s.bold())))?,
                s[index + 4..].to_owned(),
            ))
        } else {
            Ok(Self::Triple(Triple::from_str(s).map_err(|_| {
                Error::new(format!("unknown target triple {}", s))
            })?))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginFormat {
    Clap,
    Vst3,
    Auv2,
}

impl PluginFormat {
    pub fn print_name(&self) -> String {
        match self {
            Self::Clap => format!("{}", "clap".bold().bright_yellow()),
            Self::Vst3 => format!("{}", "vst3".bold().bright_cyan()),
            Self::Auv2 => format!("{}", "auv2".bold().bright_purple()),
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            Self::Clap => "clap",
            Self::Vst3 => "vst3",
            Self::Auv2 => "component",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BuildRequest {
    pub target_dir: PathBuf,

    pub packages: Vec<String>,
    pub profile: String,
    pub targets: Vec<BuildTarget>,

    pub features: Vec<String>,
    pub all_features: bool,
    pub no_default_features: bool,

    pub clap: bool,
    pub auv2: bool,
    pub vst3: Option<Vst3Sdk>,
}

pub struct BuildArtifact {
    pub package: String,
    pub target: BuildTarget,
    pub format: PluginFormat,
    pub path: PathBuf,
}

pub fn build(request: &BuildRequest) -> Result<Vec<BuildArtifact>> {
    report_span!(
        "building plugins: {}",
        request.packages.join(", ").bold().bright_blue()
    );

    if request.vst3.is_none() && !request.auv2 && !request.clap {
        return Ok(vec![]);
    }

    let use_zig = request.targets.iter().any(|x| match x {
        BuildTarget::Triple(triple) => triple != &target_lexicon::HOST,
        BuildTarget::TripleGlibc(_, _) => true,
        BuildTarget::AppleUniversal => !matches!(
            target_lexicon::HOST.operating_system,
            OperatingSystem::Darwin(_) | OperatingSystem::MacOSX(_)
        ),
    });
    let use_cmake = request.vst3.is_some() || request.auv2 || use_zig;

    if use_zig {
        ensure_zig_installed()?;
    }

    if use_cmake {
        ensure_cmake_installed()?;
    }

    let output_libraries = build_libraries(
        if use_cmake {
            CargoCrateType::Staticlib
        } else {
            CargoCrateType::Cdylib
        },
        request.target_dir.clone(),
        request.profile.clone(),
        request.packages.clone(),
        request.targets.clone(),
        request.features.clone(),
        request.all_features,
        request.no_default_features,
    )?;

    if !use_cmake {
        return Ok(output_libraries
            .into_iter()
            .map(|((target, package), path)| BuildArtifact {
                package,
                target,
                format: PluginFormat::Clap,
                path,
            })
            .collect());
    }

    let mut artifacts = Vec::new();
    let (pico_cmake, vst3_sdk) = load_dependencies(request.vst3.as_ref(), &request.target_dir)?;

    for ((target, package), path) in output_libraries {
        let (zig_triple, osx_arch) = match &target {
            BuildTarget::Triple(triple) => {
                let osx_arch = if matches!(triple.operating_system, OperatingSystem::Darwin(_)) {
                    let osx_arch = match triple.architecture {
                        target_lexicon::Architecture::Aarch64(_) => Some("arm64".to_string()),
                        target_lexicon::Architecture::X86_64 => Some("x86_64".to_string()),
                        _ => None,
                    };

                    osx_arch
                } else {
                    None
                };

                let zig_triple = if target.is_supported(&target_lexicon::HOST) {
                    None
                } else {
                    Some(zig_triple(triple, None)?)
                };

                (zig_triple, osx_arch)
            }

            BuildTarget::TripleGlibc(triple, glibc) => {
                (Some(zig_triple(triple, Some(glibc.clone()))?), None)
            }

            BuildTarget::AppleUniversal => (None, Some("arm64;x86_64".to_string())),
        };

        let output = build_wrapper(ClapWrapperOptions {
            cmake_dir: pico_cmake.clone(),
            build_dir: request.target_dir.join("clap-wrapper/build"),
            package_name: package.clone(),
            static_lib: path.clone(),
            zig_triple,
            osx_arch,
            vst3: vst3_sdk.clone(),
            auv2: request.auv2,
        })?;

        if let Some(vst3) = output.vst3 {
            artifacts.push(BuildArtifact {
                package: package.clone(),
                target: target.clone(),
                format: PluginFormat::Vst3,
                path: vst3,
            });
        }
        if let Some(auv2) = output.auv2 {
            artifacts.push(BuildArtifact {
                package: package.clone(),
                target: target.clone(),
                format: PluginFormat::Auv2,
                path: auv2,
            });
        }

        artifacts.push(BuildArtifact {
            package,
            target,
            format: PluginFormat::Clap,
            path: output.clap,
        });
    }

    Ok(artifacts)
}

#[allow(clippy::too_many_arguments)]
fn build_libraries(
    crate_type: CargoCrateType,
    target_dir: PathBuf,
    profile: String,
    packages: Vec<String>,
    targets: Vec<BuildTarget>,
    features: Vec<String>,
    all_features: bool,
    no_default_features: bool,
) -> Result<HashMap<(BuildTarget, String), PathBuf>> {
    let mut libraries = HashMap::new();
    for target in targets {
        match &target {
            BuildTarget::Triple(triple) | BuildTarget::TripleGlibc(triple, _) => {
                let output = cargo_build(CargoBuild {
                    crate_type,
                    target_dir: target_dir.clone(),
                    packages: packages.clone(),
                    profile: profile.clone(),
                    target: triple.clone(),
                    features: features.clone(),
                    all_features,
                    no_default_features,
                })?;

                for (package, path) in output {
                    libraries.insert((target.clone(), package), path);
                }
            }

            BuildTarget::AppleUniversal => {
                let output_aarch64 = cargo_build(CargoBuild {
                    crate_type,
                    target_dir: target_dir.clone(),
                    packages: packages.clone(),
                    profile: profile.clone(),
                    target: Triple::from_str("aarch64-apple-darwin")?,
                    features: features.clone(),
                    all_features,
                    no_default_features,
                })?;

                let output_x86_64 = cargo_build(CargoBuild {
                    crate_type,
                    target_dir: target_dir.clone(),
                    packages: packages.clone(),
                    profile: profile.clone(),
                    target: Triple::from_str("x86_64-apple-darwin")?,
                    features: features.clone(),
                    all_features,
                    no_default_features,
                })?;

                for package in &packages {
                    let aarch64 = output_aarch64.get(package);
                    let x86_64 = output_x86_64.get(package);

                    if let (Some(aarch64), Some(x86_64)) = (aarch64, x86_64) {
                        let universal = target_dir.join("universal-apple-darwin");
                        let _ = std::fs::create_dir_all(&universal);

                        let universal = universal.join(aarch64.file_name().unwrap_or_default());
                        apple::lipo(&[aarch64, x86_64], &universal)?;
                        libraries.insert((target.clone(), package.clone()), universal);
                    }
                }
            }
        };
    }

    Ok(libraries)
}

fn load_dependencies(
    vst3: Option<&Vst3Sdk>,
    target_dir: &Path,
) -> Result<(PathBuf, Option<PathBuf>)> {
    let cache = DependencyCache::new(target_dir.join("clap-wrapper/deps"));

    fn unwrap_thread<T>(result: std::thread::Result<T>) -> T {
        match result {
            Ok(value) => value,
            Err(e) => resume_unwind(e),
        }
    }

    std::thread::scope(|scope| {
        let vst3 = scope.spawn(|| -> Result<Option<PathBuf>> {
            Ok(match vst3 {
                Some(Vst3Sdk::OpenSource) => Some(cache.load(&Dependency::Vst3OSS(
                    "8b59557d881bb0158ba08ff256b26f025f078314".to_string(),
                ))?),
                Some(Vst3Sdk::Proprietary) => Some(cache.load(&Dependency::Vst3Proprietary)?),
                Some(Vst3Sdk::Local(path)) => {
                    if !path.exists() {
                        return Err(Error::new(format!(
                            "{} not found at {}",
                            "vst3-sdk".bright_cyan().bold(),
                            std::fs::canonicalize(path)?.display()
                        ))
                        .with_note(
                            "you've specified a local path to the sdk, but the path doesn't exist",
                        ));
                    }

                    Some(path.clone())
                }
                None => None,
            })
        });

        let pico = scope.spawn(|| -> Result<PathBuf> {
            let cargo_workspace = cargo_workspace_dir()?;
            let cargo_local_cmake = cargo_metadata()?
                .get("metadata")
                .and_then(|x| x.get::<HashMap<String, JsonValue>>())
                .and_then(|x| x.get("picobundler"))
                .and_then(|x| x.get::<HashMap<String, JsonValue>>())
                .and_then(|x| x.get("local-cmake-path"))
                .and_then(|x| x.get::<String>().cloned())
                .map(|x| cargo_workspace.join(x));

            match cargo_local_cmake {
                Some(path) => Ok(path),
                None => Ok(cache.load(&Dependency::SelfCmake(env!("GIT_HASH").to_string()))?),
            }
        });

        let vst3 = unwrap_thread(vst3.join())?;
        let pico = unwrap_thread(pico.join())?;

        Ok((pico, vst3))
    })
}
