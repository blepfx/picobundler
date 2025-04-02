use super::PluginFormat;
use crate::cli::{Command, Error, Result, report_span};
use owo_colors::OwoColorize;
use std::{
    env::var,
    fs,
    io::ErrorKind,
    panic::resume_unwind,
    path::{Path, PathBuf},
};
use target_lexicon::OperatingSystem;

pub fn run_parallel<I, O>(
    items: I,
    f: impl Fn(I::Item) -> Result<O> + Send + Sync,
) -> Result<Vec<O>>
where
    I: IntoIterator,
    O: Send,
    I::Item: Send,
{
    std::thread::scope(|s| {
        let mut handles = Vec::new();
        for item in items {
            let f = &f;
            handles.push(s.spawn(move || f(item)));
        }

        let mut result = vec![];
        for handle in handles {
            match handle.join() {
                Ok(Ok(r)) => result.push(r),
                Ok(Err(e)) => return Err(e),
                Err(e) => resume_unwind(e),
            }
        }

        Ok(result)
    })
}

pub fn reflink(src: &Path, dst: &Path) -> Result<()> {
    report_span!("copying {} to {}", src.display(), dst.display());

    if fs::metadata(src)?.is_file() {
        reflink::reflink_or_copy(src, dst)?;
        return Ok(());
    }

    let mut stack = vec![PathBuf::from("")];
    while let Some(current) = stack.pop() {
        fs::create_dir(dst.join(&current))?;

        let dir_entries = fs::read_dir(src.join(&current))?;
        for file in dir_entries {
            let file = file?;

            if file.file_type()?.is_dir() {
                stack.push(current.join(file.file_name()));
            } else {
                let src = src.join(&current).join(file.file_name());
                let dst = dst.join(&current).join(file.file_name());
                reflink::reflink_or_copy(&src, &dst)?;
            }
        }
    }

    Ok(())
}

pub fn wait_unlink(dst: &Path) -> Result<()> {
    report_span!("removing {}", dst.display());

    let try_remove = || {
        if fs::metadata(dst)?.is_file() {
            fs::remove_file(dst)
        } else {
            fs::remove_dir_all(dst)
        }
    };

    match try_remove() {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),

        #[cfg(windows)]
        Err(e) if e.kind() == ErrorKind::PermissionDenied => loop {
            std::thread::sleep(std::time::Duration::from_millis(200));
            break match try_remove() {
                Ok(_) => Ok(()),
                Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
                Err(e) if e.kind() == ErrorKind::PermissionDenied => continue,
                Err(e) => Err(e.into()),
            };
        },

        Err(e) => Err(e.into()),
    }
}

pub fn plugin_system_folder(plugin: PluginFormat, os: OperatingSystem) -> Result<PathBuf> {
    let path = match (plugin, os) {
        (PluginFormat::Clap, OperatingSystem::Windows) => var("PROGRAMFILES")
            .map(|x| format!("{}/Common Files/CLAP/", x))
            .ok(),
        (PluginFormat::Clap, OperatingSystem::Linux) => {
            var("HOME").map(|x| format!("{}/.clap/", x)).ok()
        }
        (PluginFormat::Clap, OperatingSystem::MacOSX(_))
        | (PluginFormat::Clap, OperatingSystem::Darwin(_)) => var("HOME")
            .map(|x| format!("{}/Library/Audio/Plug-Ins/CLAP/", x))
            .ok(),

        (PluginFormat::Vst3, OperatingSystem::Windows) => var("PROGRAMFILES")
            .map(|x| format!("{}/Common Files/VST3/", x))
            .ok(),
        (PluginFormat::Vst3, OperatingSystem::Linux) => {
            var("HOME").map(|x| format!("{}/.vst3/", x)).ok()
        }
        (PluginFormat::Vst3, OperatingSystem::MacOSX(_))
        | (PluginFormat::Vst3, OperatingSystem::Darwin(_)) => var("HOME")
            .map(|x| format!("{}/Library/Audio/Plug-Ins/VST3/", x))
            .ok(),

        (PluginFormat::Auv2, OperatingSystem::MacOSX(_))
        | (PluginFormat::Auv2, OperatingSystem::Darwin(_)) => var("HOME")
            .map(|x| format!("{}/Library/Audio/Plug-Ins/Components/", x))
            .ok(),

        _ => None,
    };

    path.map(PathBuf::from).ok_or_else(|| {
        Error::new(format!(
            "could not determine the {} system folder for {}",
            plugin.print_name(),
            os.into_str().bold()
        ))
    })
}

pub fn download_file(url: &str, path: &Path) -> Result<()> {
    report_span!("downloading {}", url.bold());

    if cfg!(target_os = "linux") || cfg!(target_os = "macos") {
        Command::new("curl")
            .arg("-SsL")
            .arg("-o")
            .arg(path)
            .arg(url)
            .run()?;
    } else {
        Command::new("powershell")
            .args([
                "-Command",
                &format!(
                    "(New-Object System.Net.WebClient).DownloadFile('{}', '{}')",
                    url,
                    path.display()
                ),
            ])
            .run()?;
    }

    Ok(())
}

pub fn unzip_archive(archive: &Path, path: &Path) -> Result<()> {
    report_span!("unzipping {}", archive.display().bold());

    if cfg!(target_os = "linux") || cfg!(target_os = "macos") {
        Command::new("unzip")
            .arg("-q")
            .arg(archive)
            .arg("-d")
            .arg(path)
            .run()?;
    } else {
        Command::new("powershell")
            .args([
                "-Command",
                &format!(
                    "Expand-Archive -Path {} -DestinationPath {}",
                    archive.to_str().unwrap(),
                    path.to_str().unwrap()
                ),
            ])
            .run()?;
    };

    Ok(())
}

pub fn zip_archive(path: &Path, archive: &Path) -> Result<()> {
    report_span!("zipping {}", path.display().bold());

    if cfg!(target_os = "linux") || cfg!(target_os = "macos") {
        Command::new("zip")
            .arg("-r")
            .arg("-q")
            .arg(archive)
            .arg(".")
            .cwd(path)
            .run()?;
    } else {
        Command::new("powershell")
            .args([
                "-Command",
                &format!(
                    "Compress-Archive -Path {} -DestinationPath {}",
                    path.to_str().unwrap(),
                    archive.to_str().unwrap()
                ),
            ])
            .run()?;
    };

    Ok(())
}
