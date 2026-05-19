use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use std::io::Read;

use crate::process_control;

/// Map CUDA version to PyTorch wheel index suffix.
#[allow(dead_code)]
pub fn cuda_version_to_pytorch_index(cuda_ver: &str) -> &'static str {
    let major_minor: Vec<&str> = cuda_ver.split('.').collect();
    match major_minor.first().copied() {
        Some("12") => {
            let minor = major_minor
                .get(1)
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(4);
            if minor >= 4 {
                "cu124"
            } else if minor >= 1 {
                "cu121"
            } else {
                "cu121"
            }
        }
        Some("11") => "cu118",
        _ => "cu124",
    }
}

#[allow(dead_code)]
pub fn python_torch_cuda_ready(python_path: &Path) -> bool {
    let check_script = r#"
import sys
try:
    import torch
    if torch.cuda.is_available() and getattr(torch.version, "cuda", None):
        print("TORCH_CUDA_READY")
    else:
        print("TORCH_CUDA_NONE")
except:
    print("TORCH_CUDA_FAILED")
"#;

    let mut cmd = Command::new(python_path);
    cmd.arg("-c")
        .arg(check_script)
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    process_control::configure_console_visibility(&mut cmd);
    let output = cmd.output().ok();
    output
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|text| text.contains("TORCH_CUDA_READY"))
        .unwrap_or(false)
}

#[allow(dead_code)]
pub fn check_gpu_availability(python_path: &PathBuf) -> bool {
    let check_script = r#"
import sys
try:
    import torch
    if torch.cuda.is_available():
        print("GPU_CUDA")
    else:
        print("GPU_NONE")
except:
    print("GPU_CHECK_FAILED")
"#;

    let mut cmd = Command::new(python_path);
    cmd.arg("-c")
        .arg(check_script)
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    process_control::configure_console_visibility(&mut cmd);
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(_) => return false,
    };
    let start = Instant::now();
    let timeout = Duration::from_secs(6);
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                let mut out = String::new();
                if let Some(mut stdout) = child.stdout.take() {
                    let _ = stdout.read_to_string(&mut out);
                }
                return out.contains("GPU_CUDA");
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    return false;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return false,
        }
    }
}

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
