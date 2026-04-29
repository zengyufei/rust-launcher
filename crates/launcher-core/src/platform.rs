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
            } => spawn_program(value, args, working_dir.as_deref()),
            LaunchTarget::Command {
                value,
                shell,
                working_dir,
            } => run_shell_command(value, *shell, working_dir.as_deref()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowMode {
    Normal,
    Hidden,
}

#[cfg(target_os = "windows")]
fn open_default(value: &str) -> Result<()> {
    windows::shell_open(value)
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

fn spawn_program(value: &str, args: &[String], working_dir: Option<&str>) -> Result<()> {
    let mut command = Command::new(value);
    command.args(args);
    if let Some(working_dir) = working_dir {
        command.current_dir(working_dir);
    }
    apply_window_mode(&mut command, WindowMode::Hidden);

    command
        .spawn()
        .map(|_| ())
        .map_err(|source| LauncherError::LaunchFailed {
            item_id: value.to_string(),
            message: source.to_string(),
        })
}

fn run_shell_command(value: &str, shell: CommandShell, working_dir: Option<&str>) -> Result<()> {
    let mut command = build_shell_command(value, shell);

    if let Some(working_dir) = working_dir {
        command.current_dir(working_dir);
    }

    apply_window_mode(&mut command, WindowMode::Hidden);

    command
        .spawn()
        .map(|_| ())
        .map_err(|source| LauncherError::LaunchFailed {
            item_id: value.to_string(),
            message: source.to_string(),
        })
}

fn build_shell_command(value: &str, shell: CommandShell) -> Command {
    match shell {
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
    }
}

fn apply_window_mode(command: &mut Command, mode: WindowMode) {
    #[cfg(target_os = "windows")]
    windows::apply_window_mode(command, mode);

    #[cfg(not(target_os = "windows"))]
    let _ = (command, mode);
}

#[cfg(target_os = "windows")]
mod windows {
    use std::{
        ffi::OsStr,
        os::windows::{ffi::OsStrExt, process::CommandExt},
        process::Command,
        ptr,
    };

    use windows_sys::Win32::{
        Foundation::HWND,
        UI::{
            Shell::ShellExecuteW,
            WindowsAndMessaging::{SW_HIDE, SW_SHOWNORMAL},
        },
    };

    use crate::{platform::WindowMode, LauncherError, Result};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub(super) fn shell_open(value: &str) -> Result<()> {
        let operation = wide("open");
        let target = wide(value);
        let result = unsafe {
            ShellExecuteW(
                ptr::null_mut::<HWND>() as HWND,
                operation.as_ptr(),
                target.as_ptr(),
                ptr::null(),
                ptr::null(),
                show_mode(WindowMode::Normal),
            )
        } as isize;

        if result <= 32 {
            Err(LauncherError::LaunchFailed {
                item_id: value.to_string(),
                message: format!("ShellExecuteW failed with code {result}"),
            })
        } else {
            Ok(())
        }
    }

    pub(super) fn apply_window_mode(command: &mut Command, mode: WindowMode) {
        if mode == WindowMode::Hidden {
            command.creation_flags(CREATE_NO_WINDOW);
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        OsStr::new(value).encode_wide().chain(Some(0)).collect()
    }

    pub(super) fn show_mode(mode: WindowMode) -> i32 {
        match mode {
            WindowMode::Normal => SW_SHOWNORMAL,
            WindowMode::Hidden => SW_HIDE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_powershell_command_without_cmd_wrapper() {
        let command = build_shell_command("echo hi", CommandShell::PowerShell);
        let debug = format!("{command:?}");
        assert!(debug.contains("\"powershell\""));
        assert!(debug.contains("-NoProfile"));
        assert!(debug.contains("-Command"));
        assert!(!debug.contains("\"cmd\""));
        assert!(!debug.contains("start"));
    }

    #[test]
    fn builds_cmd_command_without_start_wrapper() {
        let command = build_shell_command("echo hi", CommandShell::Cmd);
        let debug = format!("{command:?}");
        assert!(debug.contains("\"cmd\""));
        assert!(debug.contains("/C"));
        assert!(!debug.contains("start"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_hidden_mode_uses_create_no_window() {
        assert_eq!(windows::show_mode(WindowMode::Hidden), 0);
        assert_eq!(windows::show_mode(WindowMode::Normal), 1);
    }
}
