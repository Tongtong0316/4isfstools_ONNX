use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use std::io::Read;

use crate::models::TorchCudaCapability;
use crate::process_control;

/// Detect NVIDIA CUDA version via nvidia-smi. Returns CUDA version string (e.g. "12.4") or None.
pub fn detect_nvidia_cuda_version() -> Option<String> {
    let mut cmd = Command::new("nvidia-smi");
    process_control::configure_console_visibility(&mut cmd);
    let output = cmd.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    // nvidia-smi header contains: "CUDA Version: XX.Y"
    for line in text.lines() {
        if !line.contains("CUDA Version") {
            continue;
        }
        // Find "CUDA Version: X.Y" pattern
        if let Some(pos) = line.find("CUDA Version") {
            let rest = &line[pos + "CUDA Version".len()..];
            let trimmed = rest.trim_start_matches(|c: char| c == ':' || c == ' ');
            let ver: String = trimmed
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if !ver.is_empty() && ver.contains('.') {
                return Some(ver);
            }
        }
    }
    None
}

pub fn detect_nvidia_gpu_name() -> Option<String> {
    let mut cmd = Command::new("nvidia-smi");
    process_control::configure_console_visibility(&mut cmd);
    let output = cmd
        .args(["--query-gpu=name", "--format=csv,noheader,nounits"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .map(|line| line.trim())
        .find(|line| !line.is_empty())
        .map(|line| line.to_string())
}

pub fn detect_torch_cuda_capability(python_path: &Path) -> TorchCudaCapability {
    let nvidia_gpu_name = detect_nvidia_gpu_name();
    let nvidia_driver_cuda_version = detect_nvidia_cuda_version();
    let has_nvidia_gpu = nvidia_gpu_name.is_some() || nvidia_driver_cuda_version.is_some();
    let nvidia_driver_visible = has_nvidia_gpu;
    let mut capability = TorchCudaCapability {
        has_nvidia_gpu,
        nvidia_driver_visible,
        nvidia_gpu_name,
        nvidia_driver_cuda_version,
        selected_device: "cpu".to_string(),
        ..Default::default()
    };

    if !python_path.exists() {
        return capability;
    }

    let check_script = r#"
import json
try:
    import torch
    payload = {
        "torchInstalled": True,
        "torchVersion": getattr(torch, "__version__", None),
        "torchCudaAvailable": bool(torch.cuda.is_available()),
        "torchCudaVersion": getattr(torch.version, "cuda", None),
        "torchCudaDeviceName": None,
        "error": None,
    }
    if payload["torchCudaAvailable"]:
        try:
            payload["torchCudaDeviceName"] = torch.cuda.get_device_name(0)
        except Exception as e:
            payload["error"] = f"cuda_device_name_failed: {e}"
    print(json.dumps(payload, ensure_ascii=False))
except Exception as e:
    print(json.dumps({
        "torchInstalled": False,
        "torchVersion": None,
        "torchCudaAvailable": False,
        "torchCudaVersion": None,
        "torchCudaDeviceName": None,
        "error": str(e),
    }, ensure_ascii=False))
"#;

    let mut cmd = Command::new(python_path);
    cmd.arg("-X")
        .arg("utf8")
        .arg("-c")
        .arg(check_script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    process_control::configure_console_visibility(&mut cmd);
    let output = match cmd.output() {
        Ok(output) => output,
        Err(_) => {
            return capability;
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&stdout) {
        capability.torch_installed = val
            .get("torchInstalled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        capability.torch_version = val
            .get("torchVersion")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        capability.torch_cuda_available = val
            .get("torchCudaAvailable")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        capability.torch_cuda_version = val
            .get("torchCudaVersion")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        capability.torch_cuda_device_name = val
            .get("torchCudaDeviceName")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
    } else {
        capability.torch_installed = false;
        capability.torch_cuda_available = false;
    }

    if capability.torch_cuda_available {
        capability.selected_device = "cuda".to_string();
    } else {
        capability.selected_device = "cpu".to_string();
    }

    if !output.status.success() && !stderr.is_empty() {
        eprintln!("[forisfstools] torch CUDA probe stderr: {}", stderr);
    }

    capability
}

/// Map CUDA version to PyTorch wheel index suffix.
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
