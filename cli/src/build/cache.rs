use crate::cli::{Command, Error, Result, report_message, report_span};
use owo_colors::OwoColorize;
use std::path::{Path, PathBuf};

use super::{download_file, unzip_archive};

#[derive(Debug)]
pub enum Dependency {
    SelfCmake(String),
    Vst3OSS(String),
    Vst3Proprietary,
}

impl Dependency {
    pub fn folder_name(&self) -> String {
        match self {
            Self::SelfCmake(commit_id) => format!("picobundler-cmake-{}", commit_id),
            Self::Vst3OSS(commit_id) => format!("vst3-sdk-{}", commit_id),
            Self::Vst3Proprietary => "vst3-sdk-proprietary".to_string(),
        }
    }

    pub fn print_name(&self) -> String {
        match self {
            Self::SelfCmake(_) => {
                format!("{}", "picobundler-cmake".bold().bright_purple())
            }
            Self::Vst3OSS(_) => {
                format!("{}", "vst3-sdk".bold().bright_cyan())
            }
            Self::Vst3Proprietary => {
                format!("{}", "vst3-sdk".bold().bright_green())
            }
        }
    }
}

pub struct DependencyCache {
    root: PathBuf,
}

impl DependencyCache {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn load(&self, item: &Dependency) -> Result<PathBuf> {
        report_span!("checking dependency {}", item.print_name());

        let folder_path = self.root.join(item.folder_name());
        if folder_path.exists() {
            return Ok(folder_path);
        }

        let tmp_folder = self.root.join(format!("tmp-{}", item.folder_name()));
        if tmp_folder.exists() {
            std::fs::remove_dir_all(&tmp_folder)?;
        }

        std::fs::create_dir_all(&tmp_folder)?;

        report_message!("downloading dependency {}", item.print_name());
        self.load_item(&tmp_folder, item)?;
        report_message!("commiting dependency {}", item.print_name());

        std::fs::rename(&tmp_folder, &folder_path)?;

        Ok(folder_path)
    }

    fn load_item(&self, folder: &Path, item: &Dependency) -> Result<()> {
        match item {
            Dependency::SelfCmake(commit_id) => {
                git_shallow_clone("https://github.com/blepfx/picobundler", commit_id, folder)?;
                git_shallow_update_submodule(folder, "clap")?;
                git_shallow_update_submodule(folder, "clap-wrapper")?;

                Ok(())
            }

            Dependency::Vst3OSS(commit_id) => {
                git_shallow_clone(
                    "https://github.com/steinbergmedia/vst3sdk",
                    commit_id,
                    folder,
                )?;
                git_shallow_update_submodule(folder, "base")?;
                git_shallow_update_submodule(folder, "cmake")?;
                git_shallow_update_submodule(folder, "pluginterfaces")?;
                git_shallow_update_submodule(folder, "public.sdk")?;
                Ok(())
            }
            Dependency::Vst3Proprietary => {
                let archive = folder.join("vst3sdk.zip");
                download_file("https://www.steinberg.net/vst3sdk", &archive)?;
                unzip_archive(&archive, folder)?;
                std::fs::remove_file(&archive)?;
                Ok(())
            }
        }
    }
}

fn git_shallow_clone(url: &str, commit_id: &str, path: &Path) -> Result<()> {
    let short_commit: String = {
        commit_id
            .chars()
            .rev()
            .take(8)
            .collect::<String>()
            .chars()
            .rev()
            .collect()
    };

    let map_error = |e: Error| {
        e.with_note(format!(
            "make sure you have {} installed",
            "git".bold().bright_cyan()
        ))
    };

    report_span!("cloning {} ({})", url.bold(), short_commit);

    Command::new("git").cwd(path).arg("init").run()?;
    Command::new("git")
        .cwd(path)
        .arg("remote")
        .arg("add")
        .arg("origin")
        .arg(url)
        .run()
        .map_err(map_error)?;
    Command::new("git")
        .cwd(path)
        .arg("fetch")
        .arg("origin")
        .arg(commit_id)
        .arg("--depth=1")
        .run()
        .map_err(map_error)?;
    Command::new("git")
        .cwd(path)
        .arg("reset")
        .arg("--hard")
        .arg(commit_id)
        .run()
        .map_err(map_error)?;

    Ok(())
}

fn git_shallow_update_submodule(path: &Path, submodule: &str) -> Result<()> {
    report_span!("updating submodule {}", submodule.bold());

    let map_error = |e: Error| {
        e.with_note(format!(
            "make sure you have {} installed",
            "git".bold().bright_cyan()
        ))
    };

    Command::new("git")
        .cwd(path)
        .arg("submodule")
        .arg("update")
        .arg("--init")
        .arg("--depth=1")
        .arg("--remote")
        .arg(submodule)
        .run()
        .map_err(map_error)?;

    Ok(())
}
