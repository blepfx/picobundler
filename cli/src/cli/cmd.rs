use crate::cli::{Error, Result};
use owo_colors::OwoColorize;
use std::{
    ffi::OsStr,
    fmt::{Display, Write},
    io::{BufRead, BufReader, Read},
    panic::resume_unwind,
    process::Stdio,
    sync::mpsc::channel,
};

#[must_use]
pub struct Command {
    inner: std::process::Command,
    print: Vec<Component>,
}

impl Command {
    pub fn new(program: &str) -> Self {
        Self {
            inner: std::process::Command::new(program),
            print: vec![Component::Cmd(program.to_string())],
        }
    }

    pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.print
            .push(Component::Arg(arg.as_ref().to_string_lossy().to_string()));
        self.inner.arg(arg);

        self
    }

    pub fn arg_secret(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.print.push(Component::ArgSecret(
            arg.as_ref().to_string_lossy().to_string(),
        ));
        self.inner.arg(arg);

        self
    }

    pub fn args(mut self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Self {
        for arg in args {
            self = self.arg(arg);
        }

        self
    }

    pub fn cwd(mut self, path: impl AsRef<std::path::Path>) -> Self {
        self.print.insert(
            0,
            Component::Env(
                "CWD".to_string(),
                path.as_ref().to_string_lossy().to_string(),
            ),
        );
        self.inner.current_dir(path);
        self
    }

    pub fn env(mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Self {
        self.print.insert(
            0,
            Component::Env(
                key.as_ref().to_string_lossy().to_string(),
                value.as_ref().to_string_lossy().to_string(),
            ),
        );

        self.inner.env(key, value);
        self
    }

    pub fn envs(
        mut self,
        envs: impl IntoIterator<Item = (impl AsRef<OsStr>, impl AsRef<OsStr>)>,
    ) -> Self {
        for (key, value) in envs {
            self = self.env(key, value);
        }

        self
    }

    pub fn run(mut self) -> Result<String> {
        let program = format_program(&self.print);
        let output = self
            .inner
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format_error(&program, e, None))?
            .wait_with_output()
            .map_err(|e| format_error(&program, e, None))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(format_error(&program, stderr, Some(output.status)))
        }
    }

    pub fn run_stdout(mut self, stream: impl FnMut(&str)) -> Result<()> {
        let program = format_program(&self.print);
        let mut result = self
            .inner
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format_error(&program, e, None))?;

        read_to_end(result.stdout.take().unwrap(), stream)?;

        let result = result
            .wait_with_output()
            .map_err(|e| format_error(&program, e, None))?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr).to_string();
            return Err(format_error(&program, stderr, Some(result.status)));
        }

        Ok(())
    }

    pub fn run_stdout_stderr(
        mut self,
        mut stdout: impl FnMut(&str),
        mut stderr: impl FnMut(&str),
    ) -> Result<()> {
        let program = format_program(&self.print);
        let mut result = self
            .inner
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format_error(&program, e, None))?;

        read_double_pipe(
            result.stdout.take().unwrap(),
            result.stderr.take().unwrap(),
            |line| match line {
                Ok(line) => stdout(line),
                Err(line) => stderr(line),
            },
        )?;

        let result = result.wait().map_err(|e| format_error(&program, e, None))?;
        if !result.success() {
            return Err(format_error(
                &program,
                "program exited with non-zero exit code",
                Some(result),
            ));
        }

        Ok(())
    }
}

enum Component {
    Env(String, String),
    Cmd(String),
    Arg(String),
    ArgSecret(String),
}

fn format_program(cmd: &[Component]) -> String {
    let mut buffer = String::new();

    for component in cmd {
        match component {
            Component::Env(key, value) => {
                let _ = write!(
                    buffer,
                    "{}={} ",
                    key.bright_yellow().bold(),
                    value.bright_blue()
                );
            }
            Component::Cmd(cmd) => {
                let _ = write!(buffer, "{}", cmd.bright_blue().bold());
            }
            Component::Arg(arg) => {
                let _ = write!(buffer, " {}", arg.bright_black());
            }
            Component::ArgSecret(arg) => {
                let _ = write!(
                    buffer,
                    " {}**",
                    arg.chars().take(3).collect::<String>().bright_black()
                );
            }
        }
    }

    buffer
}

fn format_error(
    program: &str,
    stderr: impl Display,
    status: Option<std::process::ExitStatus>,
) -> Error {
    let mut err = Error::new(stderr).with_note(format!("the command ran was {}", program.bold()));

    if let Some(status) = status {
        err = err.with_note(match status.code() {
            Some(code) => format!("exit code: {}", code.bright_red().bold()),
            None => "terminated by a signal".to_string(),
        })
    }

    err
}

fn read_to_end(reader: impl Read, mut stream: impl FnMut(&str)) -> Result<()> {
    let mut reader = BufReader::new(reader);
    let mut buffer = String::new();
    loop {
        buffer.clear();
        match reader.read_line(&mut buffer) {
            Ok(0) => break,
            Ok(_) => {
                stream(&buffer);
            }
            Err(e) => {
                return Err(Error::from(e));
            }
        }
    }

    Ok(())
}

fn read_double_pipe(
    left: impl Read + Send,
    right: impl Read + Send,
    mut output: impl FnMut(std::result::Result<&str, &str>),
) -> Result<()> {
    let (sender, receiver) = channel();
    let sender2 = sender.clone();

    std::thread::scope(|scope| {
        let left = scope.spawn(move || {
            read_to_end(left, |line| {
                sender.send(Ok(line.to_owned())).ok();
            })
        });

        let right = scope.spawn(move || {
            read_to_end(right, |line| {
                sender2.send(Err(line.to_owned())).ok();
            })
        });

        while let Some(line) = receiver.recv().ok() {
            match line {
                Ok(line) => output(Ok(&line)),
                Err(line) => output(Err(&line)),
            }
        }

        match left.join() {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(e),
            Err(e) => resume_unwind(e),
        }?;

        match right.join() {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(e),
            Err(e) => resume_unwind(e),
        }
    })
}
