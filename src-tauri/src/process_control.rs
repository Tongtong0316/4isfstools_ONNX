use std::io;
use std::process::{Child, Command};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Windows: CREATE_NO_WINDOW flag — prevents console window from popping up
/// when spawning child processes from a GUI application.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Apply platform-specific flags to hide console windows on Windows.
pub fn configure_console_visibility(command: &mut Command) {
    #[cfg(windows)]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
}

pub fn spawn_in_own_process_group(command: &mut Command) -> io::Result<Child> {
    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        });
    }
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    command.spawn()
}

#[cfg(unix)]
pub fn terminate_process_group(pid: u32) {
    let pgid = -(pid as i32);
    unsafe {
        let _ = libc::kill(pgid as libc::pid_t, libc::SIGTERM);
    }
}

#[cfg(unix)]
pub fn force_terminate_process_group(pid: u32) {
    let pgid = -(pid as i32);
    unsafe {
        let _ = libc::kill(pgid as libc::pid_t, libc::SIGKILL);
    }
}

#[cfg(windows)]
pub fn terminate_process_group(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T"])
        .status();
}

#[cfg(windows)]
pub fn force_terminate_process_group(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .status();
}
