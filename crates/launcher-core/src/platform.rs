use std::process::Command;

use crate::{
    model::{CommandShell, LaunchTarget},
    LauncherError, Result,
};

pub trait LaunchAdapter {
    fn launch(&self, target: &LaunchTarget) -> Result<()>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemLauncher;

impl LaunchAdapter for SystemLauncher {
    fn launch(&self, target: &LaunchTarget) -> Result<()> {
        match target {
            LaunchTarget::Path { value } | LaunchTarget::Url { value } => open_default(value),
            LaunchTarget::Program {
                value,
                args,
                working_dir,
            } => {
                let mut command = Command::new(value);
                command.args(args);
                if let Some(working_dir) = working_dir {
                    command.current_dir(working_dir);
                }
                command
                    .spawn()
                    .map(|_| ())
                    .map_err(|source| LauncherError::LaunchFailed {
                        item_id: value.clone(),
                        message: source.to_string(),
                    })
            }
            LaunchTarget::Command {
                value,
                shell,
                working_dir,
            } => run_shell_command(value, *shell, working_dir.as_deref()),
        }
    }
}

#[cfg(target_os = "windows")]
fn open_default(value: &str) -> Result<()> {
    Command::new("cmd")
        .args(["/C", "start", "", value])
        .spawn()
        .map(|_| ())
        .map_err(|source| LauncherError::LaunchFailed {
            item_id: value.to_string(),
            message: source.to_string(),
        })
}

#[cfg(target_os = "macos")]
fn open_default(value: &str) -> Result<()> {
    Command::new("open")
        .arg(value)
        .spawn()
        .map(|_| ())
        .map_err(|source| LauncherError::LaunchFailed {
            item_id: value.to_string(),
            message: source.to_string(),
        })
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_default(value: &str) -> Result<()> {
    Command::new("xdg-open")
        .arg(value)
        .spawn()
        .map(|_| ())
        .map_err(|source| LauncherError::LaunchFailed {
            item_id: value.to_string(),
            message: source.to_string(),
        })
}

fn run_shell_command(value: &str, shell: CommandShell, working_dir: Option<&str>) -> Result<()> {
    let mut command = match shell {
        CommandShell::PowerShell => {
            let mut command = Command::new("powershell");
            command.args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                value,
            ]);
            command
        }
        CommandShell::Cmd => {
            let mut command = Command::new("cmd");
            command.args(["/C", value]);
            command
        }
        CommandShell::Sh => {
            let mut command = Command::new("sh");
            command.args(["-c", value]);
            command
        }
    };

    if let Some(working_dir) = working_dir {
        command.current_dir(working_dir);
    }

    command
        .spawn()
        .map(|_| ())
        .map_err(|source| LauncherError::LaunchFailed {
            item_id: value.to_string(),
            message: source.to_string(),
        })
}
