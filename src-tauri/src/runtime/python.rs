use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tauri::{AppHandle, Manager};

use crate::{get_data_dir, is_isolated_runtime_mode};

pub fn get_python_path(app: &AppHandle) -> PathBuf {
    let runtime_dir = get_data_dir().join("runtime");

    // 1. Explicit hint from python_path.txt (set during installation)
    let runtime_python_hint = runtime_dir.join("python_path.txt");
    if runtime_python_hint.exists() {
        if let Ok(path) = fs::read_to_string(&runtime_python_hint) {
            let hinted = PathBuf::from(path.trim());
            if hinted.exists() {
                return hinted;
            }
        }
    }

    // 2. Runtime directory — Windows uses python.exe, Unix uses bin/python3
    if cfg!(windows) {
        let exe = runtime_dir.join("python").join("python.exe");
        if exe.exists() {
            return exe;
        }
    } else {
        let bin = runtime_dir.join("python").join("bin").join("python3");
        if bin.exists() {
            return bin;
        }
    }

    // 3. Isolated runtime mode (bundled in app resources)
    if is_isolated_runtime_mode() {
        let resource_dir = app.path().resource_dir().unwrap_or_default();
        if cfg!(windows) {
            let w = resource_dir.join("python").join("python.exe");
            if w.exists() {
                return w;
            }
        } else {
            let p = resource_dir.join("python").join("bin").join("python3");
            if p.exists() {
                return p;
            }
        }
    }

    // 4. Dev mode: project directory
    if cfg!(windows) {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("python")
            .join("python.exe");
        if dev.exists() {
            return dev;
        }
    } else {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("python")
            .join("bin")
            .join("python3");
        if dev.exists() {
            return dev;
        }
    }

    // 5. Production resource directory
    let resource_dir = app.path().resource_dir().unwrap_or_default();
    if cfg!(windows) {
        let prod = resource_dir.join("python").join("python.exe");
        if prod.exists() {
            return prod;
        }
    } else {
        let prod = resource_dir.join("python").join("bin").join("python3");
        if prod.exists() {
            return prod;
        }
    }

    // Fallback: return path that likely doesn't exist
    if cfg!(windows) {
        runtime_dir.join("python").join("python.exe")
    } else {
        runtime_dir.join("python").join("bin").join("python3")
    }
}

pub fn python_site_packages_dir(python_path: &Path) -> Result<PathBuf, String> {
    let output = Command::new(python_path)
        .args([
            "-c",
            "import sysconfig, site; p = sysconfig.get_paths().get('purelib') or sysconfig.get_paths().get('platlib') or ''; print(p.strip())",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("Failed to resolve Python site-packages dir: {}", e))?;
    if !output.status.success() {
        return Err(format!(
            "Failed to resolve Python site-packages dir: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if dir.is_empty() {
        return Err("Failed to resolve Python site-packages dir: empty result".to_string());
    }
    Ok(PathBuf::from(dir))
}

pub fn python_file_compiles(python_path: &Path, file_path: &Path) -> Result<bool, String> {
    let output = Command::new(python_path)
        .args([
            "-c",
            &format!(
                "import py_compile; py_compile.compile({}, doraise=True)",
                format!("{:?}", file_path.to_string_lossy())
            ),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("Failed to compile-check Python file: {}", e))?;
    if output.status.success() {
        Ok(true)
    } else {
        Ok(false)
    }
}
