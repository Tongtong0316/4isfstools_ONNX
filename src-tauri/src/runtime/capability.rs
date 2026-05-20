use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use std::io::Read;

use crate::process_control;

pub fn python_module_is_available(
    python_path: &PathBuf,
    module_name: &str,
    timeout_secs: u64,
) -> Result<bool, String> {
    let script = format!(
        r#"
import importlib
try:
    importlib.import_module({module:?})
    print("OK")
except Exception:
    print("NO")
"#,
        module = module_name
    );

    let mut cmd = Command::new(python_path);
    cmd.arg("-c")
        .arg(script)
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    process_control::configure_console_visibility(&mut cmd);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to run python check: {}", e))?;

    let start = Instant::now();
    let timeout = Duration::from_secs(timeout_secs);

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut out = String::new();
                if let Some(mut stdout) = child.stdout.take() {
                    let _ = stdout.read_to_string(&mut out);
                }
                if !status.success() {
                    return Ok(false);
                }
                return Ok(out.trim() == "OK");
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    return Ok(false);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("Python module check failed: {}", e)),
        }
    }
}

pub fn detect_windows_python_path() -> Option<PathBuf> {
    if cfg!(windows) {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "where", "python"]);
        process_control::configure_console_visibility(&mut cmd);
        let output = cmd.output().ok()?;
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let p = PathBuf::from(line.trim());
                if !p.exists() {
                    continue;
                }
                // Reject Windows Store stub (WindowsApps\python.exe) — it opens the Store, not Python
                let p_lower = p.to_string_lossy().to_ascii_lowercase();
                if p_lower.contains("windowsapps") {
                    continue;
                }
                // Validate it actually runs
                let mut probe_cmd = Command::new(&p);
                probe_cmd
                    .args(["-c", "print('ok')"])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                process_control::configure_console_visibility(&mut probe_cmd);
                let probe = probe_cmd.output().ok();
                if let Some(result) = probe {
                    if result.status.success() {
                        return Some(p);
                    }
                }
            }
        }
    }
    None
}
