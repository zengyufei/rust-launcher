use std::{path::{Path, PathBuf}, process::Command};

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
                background,
            } => run_shell_command(value, *shell, working_dir.as_deref(), *background),
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
    let program = resolve_program_path(value, working_dir);
    let mut command = Command::new(&program);
    command.args(args);
    if let Some(working_dir) = working_dir {
        command.current_dir(working_dir);
    }
    apply_window_mode(&mut command, WindowMode::Hidden);

    command
        .spawn()
        .map(|_| ())
        .map_err(|source| LauncherError::LaunchFailed {
            item_id: program.display().to_string(),
            message: source.to_string(),
        })
}

fn resolve_program_path(value: &str, working_dir: Option<&str>) -> PathBuf {
    let path = Path::new(value);
    if path.is_absolute() {
        return path.to_path_buf();
    }

    match working_dir {
        Some(dir) if !dir.trim().is_empty() => Path::new(dir).join(path),
        _ => path.to_path_buf(),
    }
}

fn run_shell_command(
    value: &str,
    shell: CommandShell,
    working_dir: Option<&str>,
    background: bool,
) -> Result<()> {
    #[cfg(target_os = "windows")]
    if !background {
        return windows::spawn_shell_command_in_new_window(value, shell, working_dir);
    }

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
    let (program, args) = shell_command_parts(value, shell);
    let mut command = Command::new(program);
    command.args(args);
    command
}

fn shell_command_parts(value: &str, shell: CommandShell) -> (&'static str, Vec<String>) {
    match shell {
        CommandShell::PowerShell => (
            "powershell",
            vec![
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                value,
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        ),
        CommandShell::Cmd => ("cmd", vec!["/C".to_string(), value.to_string()]),
        CommandShell::Sh => ("sh", vec!["-c".to_string(), value.to_string()]),
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

    use crate::{model::CommandShell, platform::{shell_command_parts, WindowMode}, LauncherError, Result};

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

    pub(super) fn spawn_shell_command_in_new_window(
        value: &str,
        shell: CommandShell,
        working_dir: Option<&str>,
    ) -> Result<()> {
        let (program, args) = shell_command_parts(value, shell);
        let mut command = Command::new("cmd");
        command.arg("/C").arg("start").arg("");
        if let Some(working_dir) = working_dir.filter(|dir| !dir.trim().is_empty()) {
            command.arg("/D").arg(working_dir);
            command.current_dir(working_dir);
        }
        command.arg(program);
        command.args(args);

        command
            .spawn()
            .map(|_| ())
            .map_err(|source| LauncherError::LaunchFailed {
                item_id: value.to_string(),
                message: source.to_string(),
            })
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

    #[test]
    fn resolves_relative_program_path_against_working_dir() {
        let resolved = resolve_program_path(
            r".\debug\hostly-off-elevation.exe",
            Some(r"D:\dowork\huizhou_yunyinpay"),
        );
        assert_eq!(
            resolved,
            PathBuf::from(r"D:\dowork\huizhou_yunyinpay").join(r".\debug\hostly-off-elevation.exe")
        );
    }

    #[test]
    fn keeps_relative_program_path_when_working_dir_missing() {
        let resolved = resolve_program_path(r".\debug\hostly-off-elevation.exe", None);
        assert_eq!(resolved, PathBuf::from(r".\debug\hostly-off-elevation.exe"));
    }

    #[test]
    fn shell_command_parts_keep_selected_shell() {
        let (program, args) = shell_command_parts("npm run serve", CommandShell::Cmd);
        assert_eq!(program, "cmd");
        assert_eq!(args, vec!["/C".to_string(), "npm run serve".to_string()]);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_hidden_mode_uses_create_no_window() {
        assert_eq!(windows::show_mode(WindowMode::Hidden), 0);
        assert_eq!(windows::show_mode(WindowMode::Normal), 1);
    }
}
