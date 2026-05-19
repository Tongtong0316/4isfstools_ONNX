use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AudioFormat {
    pub(crate) sample_rate: u32,
    pub(crate) channels: u16,
}

impl Default for AudioFormat {
    fn default() -> Self {
        Self {
            sample_rate: 44_100,
            channels: 2,
        }
    }
}

pub(crate) fn ensure_parent_dir(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory {:?}: {}", parent, e))?;
    }
    Ok(())
}

pub(crate) fn normalize_source_audio(input_path: &Path, output_path: &Path) -> Result<(), String> {
    ensure_parent_dir(output_path)?;

    let ffmpeg_bin = crate::resolve_ffmpeg_binary_path().ok_or_else(|| {
        "FFmpeg 不可用：未在 PATH 或常见路径（/opt/homebrew/bin, /usr/local/bin）中找到 ffmpeg"
            .to_string()
    })?;

    let status = Command::new(ffmpeg_bin)
        .arg("-y")
        .arg("-nostdin")
        .arg("-i")
        .arg(input_path)
        .arg("-vn")
        .arg("-ac")
        .arg("2")
        .arg("-ar")
        .arg("44100")
        .arg("-c:a")
        .arg("pcm_s16le")
        .arg(output_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("Failed to run ffmpeg for audio normalization: {}", e))?;

    if !status.success() {
        return Err(format!(
            "ffmpeg audio normalization failed with status: {}",
            status
        ));
    }

    if !output_path.exists() {
        return Err("ffmpeg audio normalization finished but output file is missing".to_string());
    }

    Ok(())
}
