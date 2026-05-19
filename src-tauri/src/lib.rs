#[cfg(unix)]
use libc;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

mod events;
mod models;
mod process_control;
mod runtime;
mod separation;
mod separation_queue;
mod storage;
pub(crate) use events::{
    check_cancel_flag, emit_error_for_job, emit_progress, emit_progress_for_job,
    get_active_job_token, is_active_job,
};
pub use models::*;
use storage::{
    ensure_dir, get_data_dir, get_default_asset_root, get_file_storage_settings_path,
    get_library_path, get_lyrics_search_cache_path, get_songs_dir, normalize_file_storage_settings,
};

pub(crate) static SONGS: Mutex<Option<HashMap<String, Song>>> = Mutex::new(None);
pub(crate) static CANCEL_FLAGS: Mutex<Option<HashMap<String, bool>>> = Mutex::new(None);
static JOBS: Mutex<Option<HashMap<String, JobHandle>>> = Mutex::new(None);
pub(crate) static ACTIVE_JOB_TOKENS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
static LYRICS_SEARCH_CACHE: Mutex<Option<HashMap<String, CachedLyricsCandidateBundle>>> =
    Mutex::new(None);
static FILE_STORAGE_SETTINGS: Mutex<Option<FileStorageSettings>> = Mutex::new(None);
static JOB_TOKEN_COUNTER: AtomicU64 = AtomicU64::new(1);
const LYRICS_SEARCH_CACHE_VERSION: &str = "lyrics-search-v3";
const PIP_NETWORK_TIMEOUT_SECONDS: &str = "120";
const PIP_RETRIES: &str = "3";
const BOOTSTRAP_TOTAL_TIMEOUT: Duration = Duration::from_secs(10 * 60);
#[allow(dead_code)]
const TORCH_INSTALL_TIMEOUT: Duration = Duration::from_secs(8 * 60);
#[allow(dead_code)]
const TORCH_FALLBACK_TIMEOUT: Duration = Duration::from_secs(2 * 60);
const PYTHON_PACKAGES_TIMEOUT: Duration = Duration::from_secs(6 * 60);
#[allow(dead_code)]
const TORCH_UNINSTALL_TIMEOUT: Duration = Duration::from_secs(2 * 60);

#[derive(Clone)]
struct JobHandle {
    separator_pid: Option<u32>,
}

struct JobManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileStorageSettings {
    instrumental_root: String,
    vocals_root: String,
    lyrics_root: String,
}

impl Default for FileStorageSettings {
    fn default() -> Self {
        Self {
            instrumental_root: get_default_asset_root("instrumental")
                .to_string_lossy()
                .to_string(),
            vocals_root: get_default_asset_root("vocals")
                .to_string_lossy()
                .to_string(),
            lyrics_root: get_default_asset_root("lyrics")
                .to_string_lossy()
                .to_string(),
        }
    }
}

impl JobManager {
    fn prepare_song_job(song_id: &str) -> String {
        if let Some(job) = get_job(song_id) {
            terminate_known_job(&job, true);
        }
        terminate_song_processes(song_id, true);
        clear_cancel_flag(song_id);
        update_song_status(song_id, "pending", 0, None, None);
        remove_job(song_id);
        clear_active_job_token(song_id);
        let job_token = make_job_token(song_id);
        set_active_job_token(song_id, &job_token);
        job_token
    }

    fn clear_song_job(song_id: &str, reason: &str) {
        let _ = reason;
        set_cancel_flag(song_id);
        clear_active_job_token(song_id);
    }

    fn cleanup_interrupted_jobs() {
        terminate_app_processing_processes(false);
        std::thread::sleep(std::time::Duration::from_millis(250));
        terminate_app_processing_processes(true);

        {
            let mut songs = SONGS.lock().unwrap();
            if let Some(ref mut map) = *songs {
                for song in map.values_mut() {
                    if song.status == "processing" || song.status == "cancelling" {
                        clear_active_job_token(&song.id);
                        song.status = "cancelled".to_string();
                        song.progress = 0;
                        song.processing_stage = Some("cancelled".to_string());
                        song.error_message = Some("上次处理被中断".to_string());
                    }
                }
            }
        }
        save_songs_to_disk();
    }

    fn cancel_active_jobs(reason: &str) {
        terminate_app_processing_processes(false);
        std::thread::sleep(std::time::Duration::from_millis(250));
        terminate_app_processing_processes(true);

        {
            let mut songs = SONGS.lock().unwrap();
            if let Some(ref mut map) = *songs {
                for song in map.values_mut() {
                    if song.status == "processing" || song.status == "cancelling" {
                        set_cancel_flag(&song.id);
                        clear_active_job_token(&song.id);
                        song.status = "cancelled".to_string();
                        song.progress = 0;
                        song.processing_stage = Some("cancelled".to_string());
                        song.error_message = Some(reason.to_string());
                    }
                }
            }
        }
        save_songs_to_disk();
    }
}

fn is_isolated_runtime_mode() -> bool {
    std::env::var("FORISFSTOOLS_ISOLATED")
        .map(|v| {
            let n = v.trim().to_ascii_lowercase();
            n == "1" || n == "true" || n == "yes" || n == "on"
        })
        .unwrap_or(false)
}

fn get_lyrics_json_path(song_id: &str) -> PathBuf {
    resolve_lyrics_json_path(song_id, &get_file_storage_settings_snapshot())
}

fn command_is_available(program: &str, arg: &str) -> bool {
    if program == "ffmpeg" {
        if let Some(ffmpeg_bin) = resolve_ffmpeg_binary_path() {
            return Command::new(ffmpeg_bin)
                .arg(arg)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|status| status.success())
                .unwrap_or(false);
        }
    }
    Command::new(program)
        .arg(arg)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn resolve_ffmpeg_binary_path() -> Option<PathBuf> {
    let mut candidates = vec![PathBuf::from("ffmpeg")];
    // Windows: check runtime directory
    if cfg!(windows) {
        let runtime_ffmpeg = get_runtime_dir()
            .join("ffmpeg")
            .join("bin")
            .join("ffmpeg.exe");
        candidates.insert(0, runtime_ffmpeg);
    }
    // macOS / Linux
    candidates.extend_from_slice(&[
        PathBuf::from("/opt/homebrew/bin/ffmpeg"),
        PathBuf::from("/usr/local/bin/ffmpeg"),
        PathBuf::from("/opt/local/bin/ffmpeg"),
    ]);

    for candidate in candidates {
        let ok = Command::new(&candidate)
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if ok {
            return Some(candidate);
        }
    }
    None
}

fn is_video_import_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase()),
        Some(ext) if matches!(
            ext.as_str(),
            "mp4"
                | "mov"
                | "mkv"
                | "webm"
                | "avi"
                | "m4v"
                | "mpg"
                | "mpeg"
                | "3gp"
                | "3g2"
                | "ts"
                | "m2ts"
                | "mts"
                | "vob"
                | "wmv"
                | "asf"
                | "flv"
                | "f4v"
                | "ogv"
                | "rmvb"
                | "qt"
                | "mxf"
        )
    )
}

fn extract_audio_from_video(input_path: &Path, output_path: &Path) -> Result<(), String> {
    if output_path.exists() {
        return Ok(());
    }

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create audio output directory: {}", e))?;
    }

    let ffmpeg_bin = resolve_ffmpeg_binary_path().ok_or_else(|| {
        "FFmpeg 不可用：未在 PATH 或常见路径（/opt/homebrew/bin, /usr/local/bin）中找到 ffmpeg"
            .to_string()
    })?;

    let status = Command::new(ffmpeg_bin)
        .arg("-y")
        .arg("-nostdin")
        .arg("-i")
        .arg(input_path)
        .arg("-vn")
        .arg("-map")
        .arg("0:a:0")
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
        .map_err(|e| format!("Failed to run ffmpeg for audio extraction: {}", e))?;

    if !status.success() {
        return Err(format!(
            "ffmpeg audio extraction failed with status: {}",
            status
        ));
    }

    if !output_path.exists() {
        return Err("ffmpeg audio extraction finished but output file is missing".to_string());
    }

    Ok(())
}

fn whisper_model_probe(
    python_path: &PathBuf,
    model_dir: &PathBuf,
    timeout_secs: u64,
) -> Result<(), String> {
    let script = r#"
import os
from faster_whisper import WhisperModel
model_dir = os.environ["WHISPER_MODEL_DIR"]
model = WhisperModel(model_dir, device="cpu", compute_type="int8", local_files_only=True)
print("OK")
"#;

    let mut cmd = Command::new(python_path);
    cmd.arg("-c")
        .arg(script)
        .env("WHISPER_MODEL_DIR", model_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    process_control::configure_console_visibility(&mut cmd);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Whisper 模型校验启动失败: {}", e))?;

    let start = Instant::now();
    let timeout = Duration::from_secs(timeout_secs);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut out = String::new();
                let mut err = String::new();
                if let Some(mut stdout) = child.stdout.take() {
                    let _ = stdout.read_to_string(&mut out);
                }
                if let Some(mut stderr) = child.stderr.take() {
                    let _ = stderr.read_to_string(&mut err);
                }
                if !status.success() {
                    let detail = if !err.trim().is_empty() { err } else { out };
                    return Err(format!("Whisper 模型加载失败: {}", detail.trim()));
                }
                if !out.contains("OK") || err.to_ascii_lowercase().contains("traceback") {
                    let detail = if !err.trim().is_empty() { err } else { out };
                    return Err(format!("Whisper 模型输出异常: {}", detail.trim()));
                }
                return Ok(());
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("Whisper 模型校验超时".to_string());
                }
                std::thread::sleep(Duration::from_millis(120));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Whisper 模型校验失败: {}", e));
            }
        }
    }
}

fn whisper_model_is_usable(
    python_path: &PathBuf,
    model_dir: &PathBuf,
    timeout_secs: u64,
) -> Result<bool, String> {
    Ok(whisper_model_probe(python_path, model_dir, timeout_secs).is_ok())
}

fn detect_runtime_health(app: &AppHandle) -> RuntimeHealthReport {
    let python_path = runtime::python::get_python_path(app);
    let python_exists = python_path.exists();
    let mut separation_engine = separation::detect_engine_health(app, &get_models_dir(app));
    if python_exists {
        separation_engine.onnxruntime_available =
            runtime::capability::python_module_is_available(&python_path, "onnxruntime", 6)
                .unwrap_or(false);
        if separation_engine.onnxruntime_available {
            separation_engine.provider_fallback_reason = Some(
                "ONNX Runtime Python package detected; native API execution is pending".to_string(),
            );
        }
    }
    let ffmpeg_ready = command_is_available("ffmpeg", "-version");
    let faster_whisper_ready = if python_exists {
        runtime::capability::python_module_is_available(&python_path, "faster_whisper", 6)
            .unwrap_or(false)
    } else {
        false
    };
    let soundfile_ready = if python_exists {
        runtime::capability::python_module_is_available(&python_path, "soundfile", 6)
            .unwrap_or(false)
    } else {
        false
    };
    let (whisper_base_ready, _whisper_base_detail) = if python_exists {
        match resolve_whisper_base_model_dir(app) {
            Ok(model_dir) => match whisper_model_probe(&python_path, &model_dir, 8) {
                Ok(()) => (true, "AI 听写草稿".to_string()),
                Err(err) => (false, format!("AI 听写草稿：{}", err)),
            },
            Err(err) => (false, format!("AI 听写草稿：{}", err)),
        }
    } else {
        (false, "AI 听写草稿：Python 未就绪".to_string())
    };
    let soundfile_ready = if python_exists {
        runtime::capability::python_module_is_available(&python_path, "soundfile", 6)
            .unwrap_or(false)
    } else {
        false
    };
    let full_ready = is_onnx_capability_ready(&separation_engine, ffmpeg_ready, soundfile_ready);
    let mut checks = vec![
        RuntimeHealthCheck {
            name: "Python".to_string(),
            ok: python_exists,
            severity: if python_exists {
                "info".to_string()
            } else {
                "error".to_string()
            },
            detail: Some(python_path.to_string_lossy().to_string()),
        },
        RuntimeHealthCheck {
            name: "FFmpeg".to_string(),
            ok: ffmpeg_ready,
            severity: if ffmpeg_ready {
                "info".to_string()
            } else {
                "error".to_string()
            },
            detail: Some("音频复合与转换".to_string()),
        },
        RuntimeHealthCheck {
            name: "ONNX Runtime".to_string(),
            ok: separation_engine.onnxruntime_available,
            severity: if separation_engine.onnxruntime_available {
                "info".to_string()
            } else {
                "error".to_string()
            },
            detail: Some(
                separation_engine
                    .provider_fallback_reason
                    .clone()
                    .unwrap_or_else(|| separation_engine.selected_provider.clone()),
            ),
        },
        RuntimeHealthCheck {
            name: "ONNX 默认模型".to_string(),
            ok: separation_engine.default_model_ready,
            severity: if separation_engine.default_model_ready {
                "info".to_string()
            } else {
                "error".to_string()
            },
            detail: Some(separation_engine.default_model_id.clone()),
        },
        RuntimeHealthCheck {
            name: "ONNX Session".to_string(),
            ok: separation_engine.default_model_session_load_ok,
            severity: if separation_engine.default_model_session_load_ok {
                "info".to_string()
            } else {
                "error".to_string()
            },
            detail: Some(if separation_engine.default_model_session_load_ok {
                "已加载".to_string()
            } else {
                separation_engine
                    .default_model_session_load_error
                    .clone()
                    .unwrap_or_else(|| "未加载".to_string())
            }),
        },
        RuntimeHealthCheck {
            name: "ONNX Metadata".to_string(),
            ok: separation_engine.default_model_metadata_ok,
            severity: if separation_engine.default_model_metadata_ok {
                "info".to_string()
            } else {
                "error".to_string()
            },
            detail: Some(if separation_engine.default_model_metadata_ok {
                "已读取".to_string()
            } else {
                separation_engine
                    .default_model_metadata_error
                    .clone()
                    .unwrap_or_else(|| "未读取".to_string())
            }),
        },
        RuntimeHealthCheck {
            name: "ONNX 高质量模型".to_string(),
            ok: separation_engine.high_quality_model_ready,
            severity: "info".to_string(),
            detail: Some(if separation_engine.high_quality_model_ready {
                "可选模型已就绪".to_string()
            } else {
                "可选".to_string()
            }),
        },
        RuntimeHealthCheck {
            name: "SoundFile".to_string(),
            ok: soundfile_ready,
            severity: if soundfile_ready {
                "info".to_string()
            } else {
                "error".to_string()
            },
            detail: Some("torchaudio 兼容音频后端".to_string()),
        },
        RuntimeHealthCheck {
            name: "AI 听写草稿".to_string(),
            ok: faster_whisper_ready && whisper_base_ready,
            severity: "info".to_string(),
            detail: Some(if !faster_whisper_ready {
                "运行时缺失".to_string()
            } else if !whisper_base_ready {
                "模型缺失".to_string()
            } else {
                "已就绪".to_string()
            }),
        },
    ];

    let (level, label, detail) = if full_ready {
        (
            "ready".to_string(),
            "可运行".to_string(),
            "ONNX 分离引擎、默认模型与音频依赖已就绪".to_string(),
        )
    } else {
        (
            "error".to_string(),
            "环境异常".to_string(),
            "ONNX Runtime、默认模型或音频依赖未就绪".to_string(),
        )
    };

    RuntimeHealthReport {
        level,
        label,
        detail,
        separation_engine,
        torch_cuda_available: false,
        selected_device: "cpu".to_string(),
        torch_version: None,
        torch_cuda_version: None,
        torch_cuda_device_name: None,
        has_nvidia_gpu: false,
        nvidia_driver_visible: false,
        nvidia_driver_cuda_version: None,
        checks: {
            checks.sort_by(|a, b| a.name.cmp(&b.name));
            checks
        },
    }
}

fn is_onnx_capability_ready(
    separation_engine: &SeparationEngineHealth,
    ffmpeg_ready: bool,
    soundfile_ready: bool,
) -> bool {
    ffmpeg_ready
        && soundfile_ready
        && separation_engine.onnxruntime_available
        && separation_engine.default_model_ready
        && separation_engine.default_model_session_load_ok
        && separation_engine.default_model_metadata_ok
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn strip_jsonp_wrapper(input: &str) -> Option<&str> {
    let trimmed = input.trim();
    let start = trimmed.find('(')?;
    let end = trimmed.rfind(')')?;
    if end <= start + 1 {
        return None;
    }
    Some(trimmed[start + 1..end].trim())
}

fn parse_jsonp_or_json<T: for<'de> Deserialize<'de>>(input: &str) -> Result<T, String> {
    let json_text = strip_jsonp_wrapper(input).unwrap_or(input).trim();
    serde_json::from_str::<T>(json_text).map_err(|e| e.to_string())
}

fn ensure_lyrics_search_cache_loaded() {
    let mut cache = LYRICS_SEARCH_CACHE.lock().unwrap();
    if cache.is_some() {
        return;
    }

    let path = get_lyrics_search_cache_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(parsed) =
                serde_json::from_str::<HashMap<String, CachedLyricsCandidateBundle>>(&content)
            {
                *cache = Some(parsed);
                return;
            }
        }
    }

    *cache = Some(HashMap::new());
}

fn persist_lyrics_search_cache() {
    ensure_lyrics_search_cache_loaded();
    let cache = LYRICS_SEARCH_CACHE.lock().unwrap();
    if let Some(ref map) = *cache {
        if let Ok(json) = serde_json::to_string_pretty(map) {
            let _ = fs::write(get_lyrics_search_cache_path(), json);
        }
    }
}

fn load_file_storage_settings_from_disk() -> FileStorageSettings {
    let path = get_file_storage_settings_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(parsed) = serde_json::from_str::<FileStorageSettings>(&content) {
                return normalize_file_storage_settings(parsed);
            }
        }
    }
    normalize_file_storage_settings(FileStorageSettings::default())
}

fn ensure_file_storage_settings_loaded() {
    let mut settings = FILE_STORAGE_SETTINGS.lock().unwrap();
    if settings.is_none() {
        *settings = Some(load_file_storage_settings_from_disk());
    }
}

fn get_file_storage_settings_snapshot() -> FileStorageSettings {
    ensure_file_storage_settings_loaded();
    FILE_STORAGE_SETTINGS
        .lock()
        .unwrap()
        .as_ref()
        .cloned()
        .unwrap_or_default()
}

fn persist_file_storage_settings(settings: &FileStorageSettings) {
    let normalized = normalize_file_storage_settings(settings.clone());
    if let Ok(json) = serde_json::to_string_pretty(&normalized) {
        let _ = fs::write(get_file_storage_settings_path(), json);
    }
}

fn set_file_storage_settings(settings: FileStorageSettings) {
    let normalized = normalize_file_storage_settings(settings);
    {
        let mut current = FILE_STORAGE_SETTINGS.lock().unwrap();
        *current = Some(normalized.clone());
    }
    persist_file_storage_settings(&normalized);
}

fn resolve_asset_root(kind: &str, settings: &FileStorageSettings) -> PathBuf {
    let base = match kind {
        "instrumental" => PathBuf::from(&settings.instrumental_root),
        "vocals" => PathBuf::from(&settings.vocals_root),
        "lyrics" => PathBuf::from(&settings.lyrics_root),
        _ => get_default_asset_root(kind),
    };
    base
}

fn song_file_stem(song: &Song) -> String {
    Path::new(&song.original_path)
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("Unknown")
        .to_string()
}

fn legacy_song_workspace_dir(song_id: &str) -> PathBuf {
    get_songs_dir().join(song_id)
}

fn resolve_instrumental_path(song_id: &str, settings: &FileStorageSettings) -> PathBuf {
    resolve_asset_root("instrumental", settings)
        .join(song_id)
        .join("no_vocals.wav")
}

fn resolve_vocals_path(song_id: &str, settings: &FileStorageSettings) -> PathBuf {
    resolve_asset_root("vocals", settings)
        .join(song_id)
        .join("vocals.wav")
}

fn resolve_original_mix_path(song_id: &str, settings: &FileStorageSettings) -> PathBuf {
    resolve_asset_root("vocals", settings)
        .join(song_id)
        .join("original_mix.wav")
}

fn resolve_lyrics_json_path(song_id: &str, settings: &FileStorageSettings) -> PathBuf {
    resolve_asset_root("lyrics", settings)
        .join(song_id)
        .join("lyrics.json")
}

fn resolve_lyrics_lrc_path(song_id: &str, settings: &FileStorageSettings) -> PathBuf {
    resolve_asset_root("lyrics", settings)
        .join(song_id)
        .join("lyrics.lrc")
}

fn legacy_lyrics_json_path(song_id: &str) -> PathBuf {
    legacy_song_workspace_dir(song_id).join("lyrics.json")
}

fn legacy_lyrics_lrc_path(song_id: &str) -> PathBuf {
    legacy_song_workspace_dir(song_id).join("lyrics.lrc")
}

fn move_or_copy_file(source: &Path, target: &Path) -> Result<bool, String> {
    if source == target {
        return Ok(target.exists());
    }
    if target.exists() {
        return Ok(true);
    }
    if !source.exists() {
        return Ok(false);
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory {:?}: {}", parent, e))?;
    }
    match fs::rename(source, target) {
        Ok(_) => Ok(true),
        Err(_) => {
            fs::copy(source, target)
                .map_err(|e| format!("Failed to copy {:?} to {:?}: {}", source, target, e))?;
            let _ = fs::remove_file(source);
            Ok(true)
        }
    }
}

fn pick_existing_path(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|candidate| candidate.exists())
        .cloned()
}

fn migrate_song_assets(song: &mut Song, settings: &FileStorageSettings) -> Result<bool, String> {
    let mut changed = false;
    let source_instrumental = pick_existing_path(&[song
        .instrumental_path
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_default()]);
    let source_vocals = pick_existing_path(&[song
        .vocals_path
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_default()]);
    let source_mix = pick_existing_path(&[song
        .original_mix_path
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_default()]);
    let source_lyrics_lrc = pick_existing_path(&[
        song.lyrics_path
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_default(),
        legacy_lyrics_lrc_path(&song.id),
    ]);
    let source_lyrics_json = pick_existing_path(&[
        song.lyrics_path
            .as_ref()
            .and_then(|path| {
                Path::new(path)
                    .parent()
                    .map(|parent| parent.join("lyrics.json"))
            })
            .unwrap_or_default(),
        legacy_lyrics_json_path(&song.id),
    ]);

    let target_instrumental = resolve_instrumental_path(&song.id, settings);
    let target_vocals = resolve_vocals_path(&song.id, settings);
    let target_mix = resolve_original_mix_path(&song.id, settings);
    let target_lyrics_lrc = resolve_lyrics_lrc_path(&song.id, settings);
    let target_lyrics_json = resolve_lyrics_json_path(&song.id, settings);

    if let Some(source) = source_instrumental {
        if move_or_copy_file(&source, &target_instrumental)? || target_instrumental.exists() {
            song.instrumental_path = Some(target_instrumental.to_string_lossy().to_string());
            changed = true;
        }
    } else if target_instrumental.exists() {
        song.instrumental_path = Some(target_instrumental.to_string_lossy().to_string());
        changed = true;
    }

    if let Some(source) = source_vocals {
        if move_or_copy_file(&source, &target_vocals)? || target_vocals.exists() {
            song.vocals_path = Some(target_vocals.to_string_lossy().to_string());
            changed = true;
        }
    } else if target_vocals.exists() {
        song.vocals_path = Some(target_vocals.to_string_lossy().to_string());
        changed = true;
    }

    if let Some(source) = source_mix {
        if move_or_copy_file(&source, &target_mix)? || target_mix.exists() {
            song.original_mix_path = Some(target_mix.to_string_lossy().to_string());
            changed = true;
        }
    } else if target_mix.exists() {
        song.original_mix_path = Some(target_mix.to_string_lossy().to_string());
        changed = true;
    }

    if let Some(source) = source_lyrics_lrc {
        if move_or_copy_file(&source, &target_lyrics_lrc)? || target_lyrics_lrc.exists() {
            song.lyrics_path = Some(target_lyrics_lrc.to_string_lossy().to_string());
            changed = true;
        }
    } else if target_lyrics_lrc.exists() {
        song.lyrics_path = Some(target_lyrics_lrc.to_string_lossy().to_string());
        changed = true;
    }

    if let Some(source) = source_lyrics_json {
        if move_or_copy_file(&source, &target_lyrics_json)? || target_lyrics_json.exists() {
            changed = true;
        }
    } else if target_lyrics_json.exists() {
        changed = true;
    }

    if changed {
        let legacy_workspace = legacy_song_workspace_dir(&song.id);
        if legacy_workspace.exists() {
            let _ = fs::remove_dir_all(&legacy_workspace);
        }
    }

    Ok(changed)
}

fn migrate_library_assets() {
    let settings = get_file_storage_settings_snapshot();
    let mut changed = false;
    {
        let mut songs = SONGS.lock().unwrap();
        if let Some(ref mut map) = *songs {
            for song in map.values_mut() {
                if let Ok(song_changed) = migrate_song_assets(song, &settings) {
                    changed |= song_changed;
                }
            }
        }
    }
    if changed {
        save_songs_to_disk();
    }
}

fn cleanup_song_artifacts(song: &Song) {
    let mut cleanup_dirs = HashSet::new();
    for path in [
        song.vocals_path.as_ref(),
        song.instrumental_path.as_ref(),
        song.original_mix_path.as_ref(),
        song.lyrics_path.as_ref(),
    ] {
        if let Some(path) = path {
            if let Some(parent) = Path::new(path).parent() {
                cleanup_dirs.insert(parent.to_path_buf());
            }
        }
    }

    cleanup_dirs.insert(legacy_song_workspace_dir(&song.id));

    for dir in cleanup_dirs {
        if dir.exists() {
            let _ = fs::remove_dir_all(dir);
        }
    }
}
fn lyrics_search_cache_key(
    provider: &str,
    song_id: &str,
    query_track: &str,
    query_artist: Option<&str>,
    query_duration_ms: u64,
) -> String {
    format!(
        "{}::{}::{}::{}::{}::{}",
        LYRICS_SEARCH_CACHE_VERSION,
        provider,
        song_id,
        normalize_match_text(query_track),
        query_artist.map(normalize_match_text).unwrap_or_default(),
        query_duration_ms
    )
}

fn get_cached_lyrics_candidates(key: &str) -> Option<Vec<LyricsCandidate>> {
    const CACHE_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1000;
    ensure_lyrics_search_cache_loaded();

    let cache = LYRICS_SEARCH_CACHE.lock().unwrap();
    let Some(map) = cache.as_ref() else {
        return None;
    };
    let Some(entry) = map.get(key) else {
        return None;
    };
    if now_ms().saturating_sub(entry.cached_at) > CACHE_TTL_MS {
        return None;
    }
    Some(entry.candidates.clone())
}

fn set_cached_lyrics_candidates(key: String, candidates: Vec<LyricsCandidate>) {
    ensure_lyrics_search_cache_loaded();
    {
        let mut cache = LYRICS_SEARCH_CACHE.lock().unwrap();
        if let Some(ref mut map) = *cache {
            map.insert(
                key,
                CachedLyricsCandidateBundle {
                    cached_at: now_ms(),
                    candidates,
                },
            );
        }
    }
    persist_lyrics_search_cache();
}

fn fetch_with_lyrics_cache<F>(cache_key: String, fetcher: F) -> Result<Vec<LyricsCandidate>, String>
where
    F: FnOnce() -> Result<Vec<LyricsCandidate>, String>,
{
    if let Some(cached) = get_cached_lyrics_candidates(&cache_key) {
        return Ok(cached);
    }

    let candidates = fetcher()?;
    set_cached_lyrics_candidates(cache_key, candidates.clone());
    Ok(candidates)
}

fn load_songs_from_disk() {
    let lib_path = get_library_path();
    if lib_path.exists() {
        if let Ok(content) = fs::read_to_string(&lib_path) {
            if let Ok(songs_vec) = serde_json::from_str::<Vec<Song>>(&content) {
                let mut songs = SONGS.lock().unwrap();
                let map: HashMap<String, Song> =
                    songs_vec.into_iter().map(|s| (s.id.clone(), s)).collect();
                *songs = Some(map);
                return;
            }
        }
    }
    let mut songs = SONGS.lock().unwrap();
    *songs = Some(HashMap::new());
}

fn save_songs_to_disk() {
    let songs = SONGS.lock().unwrap();
    if let Some(ref map) = *songs {
        let songs_vec: Vec<Song> = map.values().cloned().collect();
        if let Ok(json) = serde_json::to_string_pretty(&songs_vec) {
            let lib_path = get_library_path();
            let _ = fs::write(&lib_path, json);
        }
    }
}

fn build_original_mix(vocals_path: &str, instrumental_path: &str) -> Result<String, String> {
    let vocals = PathBuf::from(vocals_path);
    let mix_path = vocals
        .parent()
        .ok_or_else(|| "Invalid vocals path".to_string())?
        .join("original_mix.wav");

    if mix_path.exists() {
        return Ok(mix_path.to_string_lossy().to_string());
    }

    if let Some(parent) = mix_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create mix directory: {}", e))?;
    }

    let ffmpeg_bin = resolve_ffmpeg_binary_path().ok_or_else(|| {
        "FFmpeg 不可用：未在 PATH 或常见路径（/opt/homebrew/bin, /usr/local/bin）中找到 ffmpeg"
            .to_string()
    })?;

    let status = Command::new(ffmpeg_bin)
        .arg("-y")
        .arg("-nostdin")
        .arg("-i")
        .arg(vocals_path)
        .arg("-i")
        .arg(instrumental_path)
        .arg("-filter_complex")
        .arg("[0:a][1:a]amix=inputs=2:duration=longest:dropout_transition=0:normalize=1")
        .arg("-c:a")
        .arg("pcm_s16le")
        .arg(&mix_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("Failed to run ffmpeg for mix: {}", e))?;

    if !status.success() {
        return Err(format!("ffmpeg mix failed with status: {}", status));
    }

    Ok(mix_path.to_string_lossy().to_string())
}

fn ensure_ffmpeg_runtime() -> Result<(), String> {
    if command_is_available("ffmpeg", "-version") {
        return Ok(());
    }
    if cfg!(target_os = "macos") {
        let has_brew = Command::new("brew")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if has_brew {
            let output = Command::new("brew")
                .args(["install", "ffmpeg"])
                .output()
                .map_err(|e| format!("调用 brew 安装 FFmpeg 失败: {}", e))?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                return Err(format!(
                    "自动安装 FFmpeg 失败（brew）：{}\n{}",
                    stderr.trim(),
                    stdout.trim()
                ));
            }
            if command_is_available("ffmpeg", "-version") {
                return Ok(());
            }
        }
        return Err("FFmpeg 未就绪：未检测到可用 ffmpeg。请先安装 Homebrew 后执行 `brew install ffmpeg`，然后重启应用。".to_string());
    }
    if !cfg!(windows) {
        return Err("FFmpeg 未就绪，请先安装 FFmpeg 并重启应用。".to_string());
    }

    // Windows: download FFmpeg from gh.llkk.cc mirror (China-accessible GitHub proxy)
    let ffmpeg_mirror_url = "https://gh.llkk.cc/https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip";
    let ffmpeg_upstream_url = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip";
    let runtime_dir = get_runtime_dir();
    let ffmpeg_dir = runtime_dir.join("ffmpeg");
    let ffmpeg_bin = ffmpeg_dir.join("bin").join("ffmpeg.exe");

    if ffmpeg_bin.exists() {
        // Add to PATH for this session
        let _ = std::env::set_var(
            "PATH",
            format!(
                "{};{}",
                ffmpeg_dir.join("bin").to_string_lossy(),
                std::env::var("PATH").unwrap_or_default()
            ),
        );
        return Ok(());
    }

    fs::create_dir_all(&ffmpeg_dir).map_err(|e| format!("创建 FFmpeg 目录失败: {}", e))?;
    let ffmpeg_archive = runtime_dir.join("ffmpeg.zip");

    let downloaded = if ffmpeg_archive.exists()
        && ffmpeg_archive
            .metadata()
            .map(|m| m.len() > 1_000_000)
            .unwrap_or(false)
    {
        true
    } else {
        download_to_file(ffmpeg_mirror_url, &ffmpeg_archive)
            .or_else(|_| download_to_file(ffmpeg_upstream_url, &ffmpeg_archive))
            .is_ok()
    };

    if downloaded {
        // Extract zip: PowerShell Expand-Archive
        let script = format!(
            "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
            ffmpeg_archive.to_string_lossy().replace('\'', "''"),
            ffmpeg_dir.to_string_lossy().replace('\'', "''")
        );
        let mut ps_cmd = Command::new("powershell");
        ps_cmd.args(["-NoProfile", "-Command", &script]);
        process_control::configure_console_visibility(&mut ps_cmd);
        let status = ps_cmd
            .status()
            .map_err(|e| format!("解压 FFmpeg 失败: {}", e))?;
        if status.success() {
            // FFmpeg zip extracts to ffmpeg-master-latest-win64-gpl/ directory
            // Move bin/ contents up to ffmpeg_dir/bin/
            let extracted_bin = ffmpeg_dir
                .join("ffmpeg-master-latest-win64-gpl")
                .join("bin");
            if extracted_bin.exists() {
                let target_bin = ffmpeg_dir.join("bin");
                let _ = fs::create_dir_all(&target_bin);
                for entry in fs::read_dir(&extracted_bin)
                    .map_err(|e| format!("读取 FFmpeg bin 失败: {}", e))?
                {
                    let entry = entry.map_err(|e| format!("读取 FFmpeg 条目失败: {}", e))?;
                    let src = entry.path();
                    let dst = target_bin.join(entry.file_name());
                    let _ = fs::copy(&src, &dst);
                }
                let _ = fs::remove_dir_all(ffmpeg_dir.join("ffmpeg-master-latest-win64-gpl"));
            }
            let _ = fs::remove_file(&ffmpeg_archive);
            if ffmpeg_bin.exists() {
                let _ = std::env::set_var(
                    "PATH",
                    format!(
                        "{};{}",
                        ffmpeg_dir.join("bin").to_string_lossy(),
                        std::env::var("PATH").unwrap_or_default()
                    ),
                );
                return Ok(());
            }
        }
        let _ = fs::remove_file(&ffmpeg_archive);
    }

    Err("FFmpeg 自动安装失败。请手动下载 FFmpeg 并放入运行时目录。".to_string())
}

fn get_models_dir(app: &AppHandle) -> PathBuf {
    let runtime_models = get_data_dir().join("runtime").join("models");
    if runtime_models.exists() {
        return runtime_models;
    }

    if is_isolated_runtime_mode() {
        let resource_dir = app.path().resource_dir().unwrap_or_default();
        return resource_dir.join("python").join("models");
    }

    let resource_dir = app.path().resource_dir().unwrap_or_default();
    let models_dir = resource_dir.join("python").join("models");

    if models_dir.exists() {
        return models_dir;
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("python")
        .join("models")
}

fn get_runtime_dir() -> PathBuf {
    get_data_dir().join("runtime")
}

fn resolve_project_root() -> PathBuf {
    if let Ok(root) = std::env::var("FORISFSTOOLS_PROJECT_ROOT") {
        let p = PathBuf::from(root);
        if p.exists() {
            return p;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() {
        return Err(format!("Source not found: {}", src.to_string_lossy()));
    }
    fs::create_dir_all(dst).map_err(|e| format!("Failed to create target dir: {}", e))?;
    for entry in fs::read_dir(src).map_err(|e| format!("Failed to read source dir: {}", e))? {
        let entry = entry.map_err(|e| format!("Failed to read dir entry: {}", e))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let metadata = entry
            .metadata()
            .map_err(|e| format!("Failed to read entry metadata: {}", e))?;
        if metadata.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else if metadata.is_file() {
            if let Some(parent) = dst_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create parent dir: {}", e))?;
            }
            fs::copy(&src_path, &dst_path).map_err(|e| {
                format!("Failed to copy file {}: {}", src_path.to_string_lossy(), e)
            })?;
        }
    }
    Ok(())
}

fn bootstrap_install_python_runtime(app: &AppHandle) -> Result<(), String> {
    let runtime_dir = get_runtime_dir();
    let runtime_python_dir = runtime_dir.join("python");

    // Platform-specific "already installed" check
    if cfg!(windows) {
        let exe = runtime_python_dir.join("python.exe");
        if exe.exists() {
            return Ok(());
        }
    } else {
        let bin = runtime_python_dir.join("bin").join("python3");
        if bin.exists() {
            return Ok(());
        }
    }

    // Clean stale state: if python dir exists but the expected binary is missing, remove it
    if runtime_python_dir.exists() {
        let dominated = if cfg!(windows) {
            !runtime_python_dir.join("python.exe").exists()
        } else {
            !runtime_python_dir.join("bin").join("python3").exists()
        };
        if dominated {
            let _ = fs::remove_dir_all(&runtime_python_dir);
        }
    }

    fs::create_dir_all(&runtime_dir).map_err(|e| format!("Failed to create runtime dir: {}", e))?;

    if cfg!(windows) {
        // Windows: download portable Python from gh.llkk.cc mirror
        let python_mirror_url = "https://gh.llkk.cc/https://github.com/astral-sh/python-build-standalone/releases/download/20260508/cpython-3.10.20%2B20260508-x86_64-pc-windows-msvc-install_only_stripped.tar.gz";
        let python_upstream_url = "https://github.com/astral-sh/python-build-standalone/releases/download/20260508/cpython-3.10.20%2B20260508-x86_64-pc-windows-msvc-install_only_stripped.tar.gz";
        let python_archive = runtime_dir.join("python-windows.tar.gz");

        let downloaded = if python_archive.exists()
            && python_archive
                .metadata()
                .map(|m| m.len() > 1_000_000)
                .unwrap_or(false)
        {
            true
        } else {
            download_to_file(python_mirror_url, &python_archive)
                .or_else(|_| download_to_file(python_upstream_url, &python_archive))
                .is_ok()
        };

        if downloaded {
            let mut tar_cmd = Command::new("tar");
            tar_cmd.args([
                "-xzf",
                &python_archive.to_string_lossy(),
                "-C",
                &runtime_dir.to_string_lossy(),
            ]);
            process_control::configure_console_visibility(&mut tar_cmd);
            let status = tar_cmd
                .status()
                .map_err(|e| format!("解压 Python 运行时失败: {}", e))?;
            if status.success() {
                let runtime_python = runtime_dir.join("python").join("python.exe");
                if runtime_python.exists() {
                    let hint = runtime_dir.join("python_path.txt");
                    let _ = fs::create_dir_all(hint.parent().unwrap_or(&runtime_dir));
                    let _ = fs::write(&hint, runtime_python.to_string_lossy().to_string());
                    let _ = fs::remove_file(&python_archive);
                    return Ok(());
                }
            }
            let _ = fs::remove_file(&python_archive);
        }

        // Fallback: try system Python, but reject Windows Store stub
        if let Some(system_python) = runtime::capability::detect_windows_python_path() {
            let hint = get_runtime_dir().join("python_path.txt");
            let _ = fs::create_dir_all(hint.parent().unwrap_or(&runtime_dir));
            let _ = fs::write(&hint, system_python.to_string_lossy().to_string());
            return Ok(());
        }

        return Err("Python 运行时安装失败：下载便携 Python 失败，系统也未检测到可用 Python。请检查网络连接后重试。".to_string());
    }

    // macOS / Linux: try bundled archive first
    let resource_dir = app.path().resource_dir().unwrap_or_default();
    let bundled_archives = [
        resource_dir.join("python").join("python-standalone.tar.gz"),
        resource_dir.join("python-standalone.tar.gz"),
    ];
    let runtime_bin = runtime_python_dir.join("bin").join("python3");
    for bundled_archive in bundled_archives {
        if bundled_archive.exists() {
            let status = Command::new("tar")
                .arg("-xzf")
                .arg(&bundled_archive)
                .arg("-C")
                .arg(&runtime_dir)
                .status()
                .map_err(|e| format!("Failed to extract bundled python archive: {}", e))?;
            if status.success() && runtime_bin.exists() {
                return Ok(());
            }
        }
    }

    // Dev fallback: copy project python directory into runtime
    let project_python_dir = resolve_project_root().join("python");
    if project_python_dir.exists() {
        copy_dir_recursive(&project_python_dir, &runtime_python_dir)?;
        if runtime_bin.exists() {
            return Ok(());
        }
    }

    Err("Python 运行时安装失败：未找到可用安装源（内置包或开发目录）。".to_string())
}

fn host_is_mainland_preferred(url: &str) -> bool {
    let parsed = match reqwest::Url::parse(url) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    host.ends_with(".cn")
        || host.contains(".cn.")
        || host.contains("aliyuncs.com")
        || host.contains("tencent")
        || host == "hf-mirror.com"
        || host.ends_with(".hf-mirror.com")
        || host == "dl.fbaipublicfiles.com"
        || host == "mirrors.tuna.tsinghua.edu.cn"
        || host == "mirrors.bfsu.edu.cn"
}

fn normalize_sha256(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn file_sha256(path: &Path) -> Result<String, String> {
    #[cfg(windows)]
    {
        let output = Command::new("certutil")
            .args(["-hashfile", &path.to_string_lossy(), "SHA256"])
            .output()
            .map_err(|e| format!("计算 SHA256 失败(certutil): {}", e))?;
        if !output.status.success() {
            return Err("计算 SHA256 失败：certutil 返回非 0".to_string());
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let token = line.trim().replace(' ', "");
            if token.len() == 64 && token.chars().all(|ch| ch.is_ascii_hexdigit()) {
                return Ok(token.to_ascii_lowercase());
            }
        }
        return Err("计算 SHA256 失败：未解析到有效哈希".to_string());
    }
    #[cfg(not(windows))]
    {
        let output = Command::new("shasum")
            .args(["-a", "256", &path.to_string_lossy()])
            .output()
            .map_err(|e| format!("计算 SHA256 失败(shasum): {}", e))?;
        if !output.status.success() {
            return Err("计算 SHA256 失败：shasum 返回非 0".to_string());
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let actual = stdout.split_whitespace().next().unwrap_or_default();
        if actual.len() == 64 && actual.chars().all(|ch| ch.is_ascii_hexdigit()) {
            Ok(actual.to_ascii_lowercase())
        } else {
            Err("计算 SHA256 失败：shasum 输出格式异常".to_string())
        }
    }
}

fn verify_download_sha256(path: &Path, expected: &Option<String>) -> Result<(), String> {
    if let Some(expected_hash) = expected {
        let expected_norm = normalize_sha256(expected_hash);
        let actual = file_sha256(path)?;
        if actual != expected_norm {
            return Err(format!(
                "校验失败：期望 SHA256={}, 实际 SHA256={}",
                expected_norm, actual
            ));
        }
    }
    Ok(())
}

fn download_to_file(url: &str, target_file: &Path) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(1200))
        .build()
        .map_err(|e| format!("创建下载客户端失败: {}", e))?;
    let mut response = client
        .get(url)
        .send()
        .map_err(|e| format!("下载失败 {}: {}", url, e))?;
    if !response.status().is_success() {
        return Err(format!("下载失败 {}: HTTP {}", url, response.status()));
    }
    if let Some(parent) = target_file.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    let mut file = fs::File::create(target_file).map_err(|e| format!("写入临时文件失败: {}", e))?;
    io::copy(&mut response, &mut file).map_err(|e| format!("写入下载文件失败: {}", e))?;
    Ok(())
}

fn extract_archive(archive_path: &Path, runtime_models: &Path) -> Result<(), String> {
    let filename = archive_path
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if filename.ends_with(".zip") {
        #[cfg(windows)]
        {
            let script = format!(
                "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
                archive_path.to_string_lossy().replace('\'', "''"),
                runtime_models.to_string_lossy().replace('\'', "''")
            );
            let mut ps_cmd = Command::new("powershell");
            ps_cmd.args(["-NoProfile", "-Command", &script]);
            process_control::configure_console_visibility(&mut ps_cmd);
            let status = ps_cmd
                .status()
                .map_err(|e| format!("解压 ZIP 失败: {}", e))?;
            if !status.success() {
                return Err("解压 ZIP 失败：PowerShell Expand-Archive 返回非 0".to_string());
            }
            return Ok(());
        }
        #[cfg(not(windows))]
        {
            let status = Command::new("unzip")
                .arg("-o")
                .arg(archive_path)
                .arg("-d")
                .arg(runtime_models)
                .status()
                .map_err(|e| format!("解压 ZIP 失败: {}", e))?;
            if !status.success() {
                return Err("解压 ZIP 失败：unzip 返回非 0".to_string());
            }
            return Ok(());
        }
    }

    let mut tar_cmd = Command::new("tar");
    tar_cmd
        .arg("-xzf")
        .arg(archive_path)
        .arg("-C")
        .arg(runtime_models);
    process_control::configure_console_visibility(&mut tar_cmd);
    let status = tar_cmd
        .status()
        .map_err(|e| format!("解压模型归档失败: {}", e))?;
    if !status.success() {
        return Err("解压模型归档失败：tar 返回非 0".to_string());
    }
    Ok(())
}

fn bootstrap_model_from_manifest_sources(
    runtime_models: &Path,
    model_name: &str,
    target_dir: &Path,
    sources: &[RuntimeManifestArtifact],
) -> Result<bool, String> {
    if sources.is_empty() {
        return Ok(false);
    }
    fs::create_dir_all(runtime_models).map_err(|e| format!("创建模型目录失败: {}", e))?;
    let mut attempts = Vec::new();
    let mut ordered_sources = sources.to_vec();
    ordered_sources.sort_by_key(|s| {
        if host_is_mainland_preferred(&s.url) {
            0
        } else {
            1
        }
    });

    for (idx, source) in ordered_sources.iter().enumerate() {
        if let Some(inline_text) = &source.inline_text {
            let rel = match &source.target_relpath {
                Some(v) if !v.trim().is_empty() => v.trim(),
                _ => {
                    attempts.push(format!("{}: inline_text 缺少 targetRelpath", model_name));
                    continue;
                }
            };
            let out_path = runtime_models.join(rel);
            let step = (|| -> Result<(), String> {
                if let Some(parent) = out_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
                }
                fs::write(&out_path, inline_text).map_err(|e| format!("写入文件失败: {}", e))?;
                verify_download_sha256(&out_path, &source.sha256)?;
                Ok(())
            })();
            match step {
                Ok(_) => {}
                Err(err) => attempts.push(format!("inline:{} => {}", rel, err)),
            }
            continue;
        }

        let lower = source.url.to_ascii_lowercase();
        let is_archive =
            lower.ends_with(".zip") || lower.ends_with(".tar.gz") || lower.ends_with(".tgz");
        let suffix = if lower.ends_with(".zip") {
            "zip"
        } else {
            "tar.gz"
        };
        let archive_path = runtime_models.join(format!("{}_source_{}.{}", model_name, idx, suffix));

        let step = if is_archive {
            let result = (|| -> Result<(), String> {
                download_to_file(&source.url, &archive_path)?;
                verify_download_sha256(&archive_path, &source.sha256)?;
                extract_archive(&archive_path, runtime_models)?;
                Ok(())
            })();
            let _ = fs::remove_file(&archive_path);
            result
        } else {
            let rel = match &source.target_relpath {
                Some(v) if !v.trim().is_empty() => v.trim(),
                _ => {
                    attempts.push(format!(
                        "{}: 文件源缺少 targetRelpath: {}",
                        model_name, source.url
                    ));
                    continue;
                }
            };
            let out_path = runtime_models.join(rel);
            (|| -> Result<(), String> {
                if out_path.exists() && verify_download_sha256(&out_path, &source.sha256).is_ok() {
                    return Ok(());
                }
                download_to_file(&source.url, &out_path)?;
                verify_download_sha256(&out_path, &source.sha256)?;
                Ok(())
            })()
        };
        match step {
            Ok(_) => {}
            Err(err) => attempts.push(format!("{} => {}", source.url, err)),
        }
    }

    match verify_manifest_targets(runtime_models, sources) {
        Ok(()) => return Ok(true),
        Err(err) => attempts.push(format!("{}: {}", model_name, err)),
    }

    let mut missing_files = Vec::new();
    for artifact in sources {
        let rel = match artifact
            .target_relpath
            .as_deref()
            .map(str::trim)
            .filter(|rel| !rel.is_empty())
        {
            Some(rel) => rel,
            None => continue,
        };
        let p = runtime_models.join(rel);
        if !p.exists() {
            missing_files.push(p.to_string_lossy().to_string());
        }
    }

    Err(format!(
        "{} 下载尝试失败：{}{}",
        model_name,
        attempts.join(" | "),
        if missing_files.is_empty() {
            "".to_string()
        } else {
            format!(" | 缺少文件: {}", missing_files.join(", "))
        }
    ))
}

fn verify_manifest_targets(
    runtime_models: &Path,
    sources: &[RuntimeManifestArtifact],
) -> Result<(), String> {
    let mut targets: HashMap<String, Option<String>> = HashMap::new();

    for source in sources {
        let rel = match source
            .target_relpath
            .as_deref()
            .map(str::trim)
            .filter(|rel| !rel.is_empty())
        {
            Some(rel) => rel.to_string(),
            None => continue,
        };
        let sha = source.sha256.as_ref().map(|value| normalize_sha256(value));
        match targets.get_mut(&rel) {
            Some(existing_sha) => match (existing_sha.as_ref(), sha.as_ref()) {
                (Some(existing), Some(next)) if existing != next => {
                    return Err(format!("targetRelpath {} 的 SHA256 不一致", rel));
                }
                (None, Some(next)) => {
                    *existing_sha = Some(next.clone());
                }
                _ => {}
            },
            None => {
                targets.insert(rel, sha);
            }
        }
    }

    if targets.is_empty() {
        return Err("未配置可验证的 targetRelpath".to_string());
    }

    let mut missing = Vec::new();
    for (rel, sha) in targets {
        let path = runtime_models.join(&rel);
        if !path.exists() {
            missing.push(rel);
            continue;
        }
        if let Some(expected) = sha {
            verify_download_sha256(&path, &Some(expected))?;
        }
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("缺少文件: {}", missing.join(", ")))
    }
}

fn bootstrap_install_whisper_model(app: &AppHandle) -> Result<(), String> {
    let runtime_models = get_runtime_dir().join("models");
    let runtime_whisper = runtime_models.join("whisper");
    let python_path = runtime::python::get_python_path(app);
    let manifest =
        runtime::manifest::load_runtime_manifest(app, &get_runtime_dir(), &resolve_project_root());
    let platform_manifest = runtime::manifest::current_platform_manifest(&manifest);
    let whisper_sources = if platform_manifest.models.whisper_base.is_empty() {
        runtime::manifest::fallback_model_artifacts(&manifest, "whisper")
    } else {
        platform_manifest.models.whisper_base
    };

    if runtime_whisper.exists() {
        if python_path.exists() {
            let whisper_usable = resolve_whisper_base_model_dir(app)
                .ok()
                .and_then(|model_dir| whisper_model_is_usable(&python_path, &model_dir, 8).ok())
                .unwrap_or(false);
            if whisper_usable {
                return Ok(());
            }
        }
        let _ = fs::remove_dir_all(&runtime_whisper);
    }

    match bootstrap_model_from_manifest_sources(
        &runtime_models,
        "whisper",
        &runtime_whisper,
        &whisper_sources,
    ) {
        Ok(true) => {}
        Ok(false) => return Err("whisper base: 未配置可用在线源".to_string()),
        Err(err) => return Err(format!("whisper base 安装失败: {}", err)),
    }

    if !python_path.exists() {
        return Err("找不到 Python 运行时，无法校验 Whisper base".to_string());
    }
    let model_dir = resolve_whisper_base_model_dir(app)?;
    if whisper_model_is_usable(&python_path, &model_dir, 8).unwrap_or(false) {
        Ok(())
    } else {
        Err("Whisper base 模型文件存在但不可用，请重新执行一键安装运行环境。".to_string())
    }
}

fn bootstrap_install_models(app: &AppHandle) -> Result<(), String> {
    let runtime_models = get_runtime_dir().join("models");
    let runtime_onnx = runtime_models.join("onnx");
    let runtime_whisper = runtime_models.join("whisper");
    let python_path = runtime::python::get_python_path(app);
    let onnx_ready_initial = if python_path.exists() {
        let engine = separation::detect_engine_health(app, &runtime_models);
        engine.default_model_ready
            && engine.default_model_session_load_ok
            && engine.default_model_metadata_ok
    } else {
        false
    };
    let whisper_ready_initial = if python_path.exists() {
        match resolve_whisper_base_model_dir(app) {
            Ok(model_dir) => whisper_model_probe(&python_path, &model_dir, 8).is_ok(),
            Err(_) => false,
        }
    } else {
        false
    };
    if onnx_ready_initial && whisper_ready_initial {
        return Ok(());
    }
    let has_whisper = runtime_whisper.exists();

    let project_models = resolve_project_root().join("python").join("models");
    fs::create_dir_all(&runtime_models)
        .map_err(|e| format!("Failed to create runtime models dir: {}", e))?;
    let mut install_notes: Vec<String> = Vec::new();

    if !onnx_ready_initial && project_models.exists() {
        let src = project_models.join("onnx");
        if src.exists() {
            copy_dir_recursive(&src, &runtime_onnx)?;
            install_notes.push("onnx default: 本地离线模型已复制".to_string());
        }
    }
    if !has_whisper && project_models.exists() {
        let src = project_models.join("whisper");
        if src.exists() {
            copy_dir_recursive(&src, &runtime_whisper)?;
            install_notes.push("whisper: 本地离线模型已复制".to_string());
        }
    }

    let manifest =
        runtime::manifest::load_runtime_manifest(app, &get_runtime_dir(), &resolve_project_root());
    let platform_manifest = runtime::manifest::current_platform_manifest(&manifest);
    let whisper_sources = if platform_manifest.models.whisper_base.is_empty() {
        runtime::manifest::fallback_model_artifacts(&manifest, "whisper")
    } else {
        platform_manifest.models.whisper_base
    };
    if !onnx_ready_initial {
        let candidate_sources = [
            project_models.join("onnx"),
            app.path()
                .resource_dir()
                .unwrap_or_default()
                .join("python")
                .join("models")
                .join("onnx"),
        ];
        if let Some(src) = candidate_sources.iter().find(|path| path.exists()) {
            copy_dir_recursive(src, &runtime_onnx)?;
            install_notes.push("onnx default: 已从本地可用源复制".to_string());
        }
    }
    if !runtime_whisper.exists() {
        match bootstrap_model_from_manifest_sources(
            &runtime_models,
            "whisper",
            &runtime_whisper,
            &whisper_sources,
        ) {
            Ok(true) => install_notes.push("whisper base: 已从大陆优先在线源下载".to_string()),
            Ok(false) => install_notes.push("whisper base: 未配置可用在线源".to_string()),
            Err(err) => install_notes.push(err),
        }
    }

    // Whisper runtime folder may exist but be unusable (e.g. empty snapshot, corrupted model.bin).
    if runtime_whisper.exists() {
        let python_path = runtime::python::get_python_path(app);
        if python_path.exists() {
            let whisper_usable = resolve_whisper_base_model_dir(app)
                .ok()
                .and_then(|model_dir| whisper_model_is_usable(&python_path, &model_dir, 8).ok())
                .unwrap_or(false);
            if !whisper_usable {
                let _ = fs::remove_dir_all(&runtime_whisper);
                match bootstrap_model_from_manifest_sources(
                    &runtime_models,
                    "whisper",
                    &runtime_whisper,
                    &whisper_sources,
                ) {
                    Ok(true) => install_notes.push("whisper base: 检测到损坏后已重装".to_string()),
                    Ok(false) => install_notes
                        .push("whisper base: 检测到损坏，但未配置可用在线源".to_string()),
                    Err(err) => install_notes.push(format!("whisper base 重装失败: {}", err)),
                }
            }
        }
    }

    let mut still_missing = Vec::new();
    let onnx_health = if python_path.exists() {
        separation::detect_engine_health(app, &runtime_models)
    } else {
        separation::detect_engine_health(app, &runtime_models)
    };
    if !onnx_health.default_model_ready {
        still_missing.push("onnx default model");
    }
    if !onnx_health.default_model_session_load_ok {
        still_missing.push("onnx session");
    }
    if !onnx_health.default_model_metadata_ok {
        still_missing.push("onnx metadata");
    }

    let python_path = runtime::python::get_python_path(app);
    let whisper_ready = if python_path.exists() {
        match resolve_whisper_base_model_dir(app) {
            Ok(model_dir) => match whisper_model_probe(&python_path, &model_dir, 8) {
                Ok(()) => true,
                Err(err) => {
                    install_notes.push(format!("whisper base 自检失败: {}", err));
                    false
                }
            },
            Err(err) => {
                install_notes.push(format!("whisper base 目录异常: {}", err));
                false
            }
        }
    } else {
        false
    };
    if !whisper_ready {
        still_missing.push("whisper base 模型");
    }

    if still_missing.is_empty() {
        Ok(())
    } else {
        let onnx_missing = still_missing.iter().any(|s| {
            s.contains("onnx default") || s.contains("onnx session") || s.contains("onnx metadata")
        });
        let whisper_missing = still_missing.iter().any(|s| s.contains("whisper base"));
        if onnx_missing {
            Err(format!(
                "核心模型（人声分离）安装失败：ONNX 默认模型缺失或不可用。请检查模型文件或配置离线模型。细节：{}",
                if install_notes.is_empty() {
                    "无安装日志".to_string()
                } else {
                    install_notes.join(" | ")
                }
            ))
        } else if whisper_missing {
            // Whisper 可选，降级为成功但记录警告
            Ok(())
        } else if !still_missing.is_empty() {
            // 未知模型缺失不得静默 OK
            Err(format!(
                "模型安装失败：{}。仅 Whisper base 缺失可降级，其他模型缺失必须解决。细节：{}",
                still_missing.join("、"),
                if install_notes.is_empty() {
                    "无安装日志".to_string()
                } else {
                    install_notes.join(" | ")
                }
            ))
        } else {
            Ok(())
        }
    }
}

fn install_python_packages_with_fallbacks(
    app: &AppHandle,
    python_path: &Path,
    packages: &[&str],
    deadline: Instant,
) -> Result<(), String> {
    if packages.is_empty() {
        return Ok(());
    }
    let mirrors = [
        (
            "https://mirrors.tuna.tsinghua.edu.cn/pypi/web/simple",
            "mirrors.tuna.tsinghua.edu.cn",
        ),
        (
            "https://mirrors.aliyun.com/pypi/simple",
            "mirrors.aliyun.com",
        ),
        ("https://pypi.org/simple", "pypi.org"),
    ];

    let mut errors = Vec::new();
    for (mirror, host) in mirrors {
        let mut args = vec![
            "-m",
            "pip",
            "install",
            "-U",
            "--disable-pip-version-check",
            "--no-input",
            "--timeout",
            PIP_NETWORK_TIMEOUT_SECONDS,
            "--retries",
            PIP_RETRIES,
            "-i",
            mirror,
            "--trusted-host",
            host,
        ];
        for pkg in packages {
            args.push(pkg);
        }

        emit_bootstrap_progress(
            app,
            "install_python_packages",
            48,
            &format!("正在从 {} 安装 Python 依赖：{}", host, packages.join("、")),
        );
        let mut command = Command::new(python_path);
        command.args(&args);
        let output = run_hidden_command_with_timeout(
            &mut command,
            remaining_bootstrap_timeout(deadline, PYTHON_PACKAGES_TIMEOUT)?,
            "Python 依赖安装",
            Some(app),
            "install_python_packages",
            52,
            &format!("正在安装 Python 依赖：{}", packages.join("、")),
        )?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        errors.push(format!("[{}] {} {}", mirror, stderr, stdout));
    }

    Err(format!("多源安装失败：{}", errors.join(" | ")))
}

/// Patch torchaudio/__init__.py to use soundfile backend instead of torchcodec.
/// torchaudio 2.11+ defaults to torchcodec which requires FFmpeg shared DLLs not present in our runtime.
/// Import hooks (sitecustomize.py) don't work for child processes, so we patch the source directly.
#[allow(dead_code)]
fn install_torchaudio_compat_patch(python_path: &Path) -> Result<(), String> {
    let site_packages = runtime::python::python_site_packages_dir(python_path)?;
    let init_path = site_packages.join("torchaudio").join("__init__.py");
    if !init_path.exists() {
        return Err(format!(
            "torchaudio __init__.py not found: {}",
            init_path.display()
        ));
    }

    // If the current file is already valid and contains our soundfile fallback, nothing to do.
    let already_patched = fs::read_to_string(&init_path)
        .ok()
        .map(|content| content.contains("soundfile as sf") && content.contains("sf.read"))
        .unwrap_or(false)
        && runtime::python::python_file_compiles(python_path, &init_path).unwrap_or(false);
    if already_patched {
        return Ok(());
    }

    // Prefer a backup copy when available so we can repatch a broken runtime file from a clean source.
    let backup = init_path.with_extension("py.bak");
    let source_path = if backup.exists() { &backup } else { &init_path };
    let content = fs::read_to_string(source_path)
        .map_err(|e| format!("Failed to read torchaudio __init__.py: {}", e))?;

    if !backup.exists() {
        let _ = fs::copy(&init_path, &backup);
    }

    // Patch load function: replace `return load_with_torchcodec(...)` block with soundfile impl
    let load_replacement = r#"import soundfile as sf
    data, samplerate = sf.read(str(uri), dtype="float32", start=frame_offset, stop=(frame_offset + num_frames if num_frames > 0 else None))
    data = torch.from_numpy(data)
    if data.ndim == 1:
        data = data.unsqueeze(0)
    elif channels_first:
        data = data.T
    return data, samplerate"#;

    // Patch save function: replace `return save_with_torchcodec(...)` block with soundfile impl
    let save_replacement = r#"import soundfile as sf
    if src.ndim == 1:
        src = src.unsqueeze(0)
    data = src.numpy()
    if channels_first and data.shape[0] > 1:
        data = data.T
    sf.write(str(uri), data, sample_rate, format=format)"#;

    let mut new_content = content.clone();

    // Replace load_with_torchcodec return block
    if let Some(start) = new_content.find("return load_with_torchcodec(") {
        if let Some(end) = find_return_block_end(&new_content, start) {
            let before = &new_content[..start];
            let after = &new_content[end..];
            new_content = format!("{}{}{}", before, load_replacement, after);
        }
    }

    // Replace save_with_torchcodec return block
    if let Some(start) = new_content.find("return save_with_torchcodec(") {
        if let Some(end) = find_return_block_end(&new_content, start) {
            let before = &new_content[..start];
            let after = &new_content[end..];
            new_content = format!("{}{}{}", before, save_replacement, after);
        }
    }

    if new_content == content {
        return Err("Failed to patch torchaudio: patterns not found".to_string());
    }

    fs::write(&init_path, &new_content)
        .map_err(|e| format!("Failed to write patched torchaudio: {}", e))?;

    if !runtime::python::python_file_compiles(python_path, &init_path).unwrap_or(false) {
        if backup.exists() {
            let _ = fs::copy(&backup, &init_path);
        }
        return Err(format!(
            "Failed to validate patched torchaudio: {}",
            init_path.display()
        ));
    }

    Ok(())
}

/// Find the end of a `return func(...)` block by tracking parenthesis depth
#[allow(dead_code)]
fn find_return_block_end(content: &str, start: usize) -> Option<usize> {
    let bytes = content[start..].as_bytes();
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    // Include trailing newline
                    let end = start + i + 1;
                    let remaining = &content[end..];
                    if remaining.starts_with('\n') {
                        return Some(end + 1);
                    }
                    return Some(end);
                }
            }
            _ => {}
        }
    }
    None
}

fn emit_bootstrap_progress(app: &AppHandle, stage: &str, progress: u32, message: &str) {
    let _ = app.emit(
        "bootstrap-progress",
        serde_json::json!({
            "stage": stage,
            "progress": progress.min(100),
            "message": message,
        }),
    );
}

fn run_hidden_command_with_timeout(
    command: &mut Command,
    timeout: Duration,
    label: &str,
    app: Option<&AppHandle>,
    stage: &str,
    progress: u32,
    heartbeat_message: &str,
) -> Result<Output, String> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    process_control::configure_console_visibility(command);
    let start = Instant::now();
    let mut last_emit = Instant::now()
        .checked_sub(Duration::from_secs(30))
        .unwrap_or_else(Instant::now);
    let mut child =
        spawn_in_own_process_group(command).map_err(|e| format!("启动 {} 失败: {}", label, e))?;

    loop {
        if child
            .try_wait()
            .map_err(|e| format!("等待 {} 失败: {}", label, e))?
            .is_some()
        {
            return child
                .wait_with_output()
                .map_err(|e| format!("读取 {} 输出失败: {}", label, e));
        }

        let elapsed = start.elapsed();
        if elapsed >= timeout {
            force_terminate_process_group(child.id());
            let _ = child.wait();
            return Err(format!(
                "{} 超时：已运行 {} 分钟仍未结束，已自动终止。请检查网络、代理、杀毒软件或 Python/pip 源。",
                label,
                (elapsed.as_secs() + 59) / 60
            ));
        }

        if let Some(app) = app {
            if last_emit.elapsed() >= Duration::from_secs(5) {
                emit_bootstrap_progress(
                    app,
                    stage,
                    progress,
                    &format!("{}（已运行 {} 秒）", heartbeat_message, elapsed.as_secs()),
                );
                last_emit = Instant::now();
            }
        }

        std::thread::sleep(Duration::from_millis(250));
    }
}

fn remaining_bootstrap_timeout(deadline: Instant, preferred: Duration) -> Result<Duration, String> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or_else(|| "一键部署超过 10 分钟上限，已停止继续安装。".to_string())?;
    if remaining < Duration::from_secs(5) {
        return Err("一键部署剩余时间不足，已停止继续安装。".to_string());
    }
    Ok(preferred.min(remaining))
}

#[allow(dead_code)]
fn summarize_separator_failure_output(
    stdout: &str,
    stderr: &str,
    status: &std::process::ExitStatus,
) -> String {
    let mut lines = Vec::new();
    for text in [stdout, stderr] {
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            lines.push(trimmed.to_string());
        }
    }
    if lines.is_empty() {
        return format!("分离脚本输出为空，退出码: {}", status);
    }

    let noisy_markers = [
        "Traceback (most recent call last)",
        "File \"<frozen runpy>\"",
        "exec(code, run_globals)",
        "runpy.py",
    ];
    let mut candidate = None;
    for line in &lines {
        if noisy_markers.iter().any(|marker| line.contains(marker)) || line.starts_with("File ") {
            continue;
        }
        if [
            "ImportError",
            "ModuleNotFoundError",
            "RuntimeError",
            "OSError",
            "ValueError",
            "AssertionError",
            "FileNotFoundError",
            "PermissionError",
        ]
        .iter()
        .any(|prefix| line.starts_with(prefix))
            || line.contains(": ")
        {
            candidate = Some(line.clone());
        }
    }

    let mut summary = candidate.unwrap_or_else(|| lines.last().cloned().unwrap_or_default());
    let tail = lines.iter().rev().take(4).cloned().collect::<Vec<_>>();
    if !tail.is_empty() {
        let tail_text = tail.into_iter().rev().collect::<Vec<_>>().join(" | ");
        if !tail_text.contains(&summary) {
            summary = format!("{} | {}", summary, tail_text);
        }
    }
    if summary.len() > 900 {
        summary.truncate(900);
    }
    summary
}

#[allow(dead_code)]
fn read_log_tail_for_error(path: &Path, max_lines: usize) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let lines = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .rev()
        .take(max_lines)
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return None;
    }
    Some(lines.into_iter().rev().collect::<Vec<_>>().join(" | "))
}

fn timestamp_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn append_text_log(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut content = String::new();
    if let Ok(existing) = fs::read_to_string(path) {
        content.push_str(&existing);
        if !existing.ends_with('\n') {
            content.push('\n');
        }
    }
    content.push_str(text);
    let _ = fs::write(path, content);
}

fn format_log_block(title: &str, lines: &[(&str, String)]) -> String {
    let mut out = String::new();
    out.push_str(&format!("[{}] {}\n", timestamp_millis(), title));
    for (key, value) in lines {
        out.push_str(&format!("{}={}\n", key, value));
    }
    out.push('\n');
    out
}

#[allow(dead_code)]
fn required_onnx_runtime_packages() -> Vec<&'static str> {
    if cfg!(windows) {
        vec!["onnxruntime-directml", "numpy", "soundfile"]
    } else {
        vec!["onnxruntime", "numpy", "soundfile"]
    }
}

fn ensure_onnx_runtime_modules(app: &AppHandle, deadline: Instant) -> Result<(), String> {
    let python_path = runtime::python::get_python_path(app);
    if !python_path.exists() {
        return Err("未检测到 Python 运行时".to_string());
    }

    let mut required_missing = Vec::new();
    for module in ["onnxruntime", "numpy", "soundfile"] {
        if !runtime::capability::python_module_is_available(&python_path, module, 6)
            .unwrap_or(false)
        {
            required_missing.push(module.to_string());
        }
    }

    if !required_missing.is_empty() {
        let packages = required_onnx_runtime_packages();
        install_python_packages_with_fallbacks(app, &python_path, &packages, deadline)
            .map_err(|e| format!("ONNX Runtime 依赖安装失败: {}", e))?;
    }

    let mut final_missing = Vec::new();
    for module in ["onnxruntime", "numpy", "soundfile"] {
        if !runtime::capability::python_module_is_available(&python_path, module, 6)
            .unwrap_or(false)
        {
            final_missing.push(module.to_string());
        }
    }
    if !final_missing.is_empty() {
        return Err(format!(
            "一键安装后仍缺少 ONNX 分离依赖: {}",
            final_missing.join(", ")
        ));
    }

    // AI 听写仍依赖 faster-whisper，但它不是 ONNX 分离主线的阻断项。
    if !runtime::capability::python_module_is_available(&python_path, "faster_whisper", 6)
        .unwrap_or(false)
    {
        let _ = install_python_packages_with_fallbacks(
            app,
            &python_path,
            &["faster-whisper"],
            deadline,
        )
        .map_err(|e| {
            eprintln!(
                "[forisfstools] faster-whisper optional install failed: {}",
                e
            );
            e
        });
    }

    Ok(())
}

#[allow(dead_code)]
fn uninstall_existing_torch_packages(app: &AppHandle, python_path: &Path, deadline: Instant) {
    emit_bootstrap_progress(app, "uninstall_torch", 36, "正在清理既有 Torch 组件...");
    let mut command = Command::new(python_path);
    command.args([
        "-m",
        "pip",
        "uninstall",
        "--disable-pip-version-check",
        "--no-input",
        "-y",
        "torch",
        "torchaudio",
    ]);
    let output = run_hidden_command_with_timeout(
        &mut command,
        remaining_bootstrap_timeout(deadline, TORCH_UNINSTALL_TIMEOUT)
            .unwrap_or(Duration::from_secs(5)),
        "Torch 清理",
        Some(app),
        "uninstall_torch",
        38,
        "正在清理既有 Torch 组件...",
    );
    if let Err(err) = output {
        eprintln!(
            "[forisfstools] 卸载既有 torch/torchaudio 时出现错误，继续安装: {}",
            err
        );
    }
}

/// Install PyTorch with automatic CUDA detection.
/// Source priority: Aliyun CUDA wheel mirror -> PyTorch official CUDA index -> Tsinghua PyPI/official CPU.
#[allow(dead_code)]
fn install_torch_with_cuda_detection(
    app: &AppHandle,
    python_path: &Path,
    prefer_cuda: bool,
    deadline: Instant,
) -> Result<(), String> {
    let detected_cuda = if prefer_cuda {
        runtime::capability::detect_nvidia_cuda_version()
    } else {
        None
    };
    let install_from_index = |cuda_index: &str, expect_cuda: bool| -> Result<(), String> {
        let mut first_label = "清华 PyPI";
        let mut args = vec![
            "-m".to_string(),
            "pip".to_string(),
            "install".to_string(),
            "-U".to_string(),
            "--disable-pip-version-check".to_string(),
            "--no-input".to_string(),
            "--timeout".to_string(),
            PIP_NETWORK_TIMEOUT_SECONDS.to_string(),
            "--retries".to_string(),
            PIP_RETRIES.to_string(),
        ];
        if expect_cuda {
            first_label = "阿里云 PyTorch CUDA 镜像";
            args.extend([
                "--force-reinstall".to_string(),
                "--no-cache-dir".to_string(),
            ]);
        }
        args.extend(["torch".to_string(), "torchaudio".to_string()]);
        if expect_cuda {
            let aliyun_links = format!("https://mirrors.aliyun.com/pytorch-wheels/{}/", cuda_index);
            args.extend([
                "--no-index".to_string(),
                "--find-links".to_string(),
                aliyun_links,
                "--trusted-host".to_string(),
                "mirrors.aliyun.com".to_string(),
            ]);
        } else {
            args.extend([
                "--index-url".to_string(),
                "https://mirrors.tuna.tsinghua.edu.cn/pypi/web/simple".to_string(),
                "--trusted-host".to_string(),
                "mirrors.tuna.tsinghua.edu.cn".to_string(),
            ]);
        }
        emit_bootstrap_progress(
            app,
            "install_torch",
            if expect_cuda { 42 } else { 44 },
            if expect_cuda {
                "正在安装 CUDA 版 Torch，文件较大，请保持网络连接..."
            } else {
                "正在安装 CPU 版 Torch..."
            },
        );
        let mut command = Command::new(python_path);
        command.args(&args);
        let output = run_hidden_command_with_timeout(
            &mut command,
            remaining_bootstrap_timeout(deadline, TORCH_INSTALL_TIMEOUT)?,
            if expect_cuda {
                "CUDA Torch 安装"
            } else {
                "CPU Torch 安装"
            },
            Some(app),
            "install_torch",
            if expect_cuda { 46 } else { 48 },
            if expect_cuda {
                "正在安装 CUDA 版 Torch，文件较大，请稍候..."
            } else {
                "正在安装 CPU 版 Torch..."
            },
        )?;

        if output.status.success() {
            if !expect_cuda {
                return Ok(());
            }
            let refreshed_capability =
                runtime::capability::detect_torch_cuda_capability(python_path);
            if refreshed_capability.torch_cuda_available {
                return Ok(());
            }
        }

        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let official_index = format!("https://download.pytorch.org/whl/{}", cuda_index);
        let mut fallback_args = vec![
            "-m".to_string(),
            "pip".to_string(),
            "install".to_string(),
            "-U".to_string(),
            "--disable-pip-version-check".to_string(),
            "--no-input".to_string(),
            "--timeout".to_string(),
            PIP_NETWORK_TIMEOUT_SECONDS.to_string(),
            "--retries".to_string(),
            PIP_RETRIES.to_string(),
        ];
        if expect_cuda {
            fallback_args.extend([
                "--force-reinstall".to_string(),
                "--no-cache-dir".to_string(),
            ]);
        }
        fallback_args.extend([
            "torch".to_string(),
            "torchaudio".to_string(),
            "--index-url".to_string(),
            official_index,
        ]);
        emit_bootstrap_progress(
            app,
            "install_torch_fallback",
            50,
            "镜像源未完成，正在尝试 PyTorch 官方源...",
        );
        let mut fallback_command = Command::new(python_path);
        fallback_command.args(&fallback_args);
        let fallback_output = run_hidden_command_with_timeout(
            &mut fallback_command,
            remaining_bootstrap_timeout(deadline, TORCH_FALLBACK_TIMEOUT)?,
            "Torch 官方源安装",
            Some(app),
            "install_torch_fallback",
            54,
            "正在尝试 PyTorch 官方源...",
        )?;

        if fallback_output.status.success() {
            if !expect_cuda {
                return Ok(());
            }
            let refreshed_capability =
                runtime::capability::detect_torch_cuda_capability(python_path);
            if refreshed_capability.torch_cuda_available {
                return Ok(());
            }
        }

        let fallback_stderr = String::from_utf8_lossy(&fallback_output.stderr).to_string();
        Err(format!(
            "torch 安装失败。{}: {} | PyTorch 官方: {}",
            first_label,
            stderr.trim(),
            fallback_stderr.trim()
        ))
    };

    if prefer_cuda {
        let preferred_cuda_index = match &detected_cuda {
            Some(cuda_ver) => {
                let idx = runtime::capability::cuda_version_to_pytorch_index(&cuda_ver);
                eprintln!(
                    "[forisfstools] 检测到 NVIDIA GPU / CUDA {}, 使用 PyTorch {} 版本",
                    cuda_ver, idx
                );
                idx
            }
            None if cfg!(windows) => {
                eprintln!(
                    "[forisfstools] 请求 CUDA 安装但未能读取 nvidia-smi，先尝试使用 cu124 轮子"
                );
                "cu124"
            }
            None => "cpu",
        };

        uninstall_existing_torch_packages(app, python_path, deadline);
        match install_from_index(preferred_cuda_index, true) {
            Ok(()) => return Ok(()),
            Err(cuda_err) => {
                eprintln!(
                    "[forisfstools] CUDA torch 安装未成功，回退到 CPU 轮子: {}",
                    cuda_err
                );
                match install_from_index("cpu", false) {
                    Ok(()) => {
                        return Err(format!(
                            "CUDA torch 安装未成功；已回退安装 CPU torch。{}",
                            cuda_err
                        ));
                    }
                    Err(cpu_err) => {
                        return Err(format!(
                            "{} | CPU torch 回退安装也失败: {}",
                            cuda_err, cpu_err
                        ));
                    }
                }
            }
        }
    }

    eprintln!("[forisfstools] 未请求 CUDA，使用 PyTorch CPU 版本");
    install_from_index("cpu", false)
}

fn detect_bootstrap_status(app: &AppHandle) -> BootstrapStatus {
    let python_path = runtime::python::get_python_path(app);
    let python_ready = python_path.exists();
    let mut separation_engine = separation::detect_engine_health(app, &get_models_dir(app));
    if python_ready {
        separation_engine.onnxruntime_available =
            runtime::capability::python_module_is_available(&python_path, "onnxruntime", 6)
                .unwrap_or(false);
    }
    let onnx_model_ready = separation_engine.default_model_ready;
    let whisper_base_ready = if python_ready {
        resolve_whisper_base_model_dir(app)
            .ok()
            .and_then(|model_dir| whisper_model_is_usable(&python_path, &model_dir, 8).ok())
            .unwrap_or(false)
    } else {
        false
    };
    let ffmpeg_ready = command_is_available("ffmpeg", "-version");
    let soundfile_ready = if python_ready {
        runtime::capability::python_module_is_available(&python_path, "soundfile", 6)
            .unwrap_or(false)
    } else {
        false
    };
    let can_run_core = is_onnx_capability_ready(&separation_engine, ffmpeg_ready, soundfile_ready);

    let detail = if can_run_core {
        "ONNX Runtime、默认分离模型与音频依赖已就绪，可运行人声分离。".to_string()
    } else {
        "ONNX Runtime、默认分离模型或音频依赖未就绪，请继续安装/修复。".to_string()
    };

    BootstrapStatus {
        runtime_ready: python_ready,
        onnx_model_ready,
        whisper_base_ready,
        ffmpeg_ready,
        can_run_core,
        selected_provider: separation_engine.selected_provider.clone(),
        torch_cuda_available: false,
        selected_device: "cpu".to_string(),
        torch_version: None,
        torch_cuda_version: None,
        torch_cuda_device_name: None,
        has_nvidia_gpu: false,
        nvidia_driver_visible: false,
        nvidia_driver_cuda_version: None,
        detail,
    }
}

fn format_missing_core_components_with_reason(health: &RuntimeHealthReport) -> String {
    let mut missing = health
        .checks
        .iter()
        .filter(|c| !c.ok)
        // AI 听写草稿、Torch CUDA、NVIDIA GPU 都是可选能力，不参与核心就绪判断
        .filter(|c| c.name != "AI 听写草稿")
        .filter(|c| c.name != "Torch CUDA")
        .filter(|c| c.name != "NVIDIA GPU")
        .map(|c| {
            let detail = c.detail.as_deref().unwrap_or("").trim();
            if detail.is_empty() {
                c.name.clone()
            } else {
                format!("{}（{}）", c.name, detail)
            }
        })
        .collect::<Vec<String>>();
    missing.sort();
    missing.dedup();
    if missing.is_empty() {
        "未知".to_string()
    } else {
        missing.join("、")
    }
}

fn update_song_status_for_job(
    song_id: &str,
    job_token: &str,
    status: &str,
    progress: u32,
    stage: Option<&str>,
    error: Option<&str>,
) {
    if is_active_job(song_id, job_token) {
        update_song_status(song_id, status, progress, stage, error);
    }
}

fn clear_cancel_flag(song_id: &str) {
    let mut flags = CANCEL_FLAGS.lock().unwrap();
    if let Some(ref mut map) = *flags {
        map.remove(song_id);
    }
}

fn set_cancel_flag(song_id: &str) {
    let mut flags = CANCEL_FLAGS.lock().unwrap();
    if flags.is_none() {
        *flags = Some(HashMap::new());
    }
    if let Some(ref mut map) = *flags {
        map.insert(song_id.to_string(), true);
    }
}

fn get_job(song_id: &str) -> Option<JobHandle> {
    let jobs = JOBS.lock().unwrap();
    jobs.as_ref().and_then(|m| m.get(song_id).cloned())
}

#[allow(dead_code)]
fn set_job(song_id: &str, job: JobHandle) {
    let mut jobs = JOBS.lock().unwrap();
    if jobs.is_none() {
        *jobs = Some(HashMap::new());
    }
    if let Some(ref mut map) = *jobs {
        map.insert(song_id.to_string(), job);
    }
}

fn make_job_token(song_id: &str) -> String {
    let seq = JOB_TOKEN_COUNTER.fetch_add(1, Ordering::Relaxed);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{}:{}:{}", song_id, ts, seq)
}

fn set_active_job_token(song_id: &str, job_token: &str) {
    let mut tokens = ACTIVE_JOB_TOKENS.lock().unwrap();
    if tokens.is_none() {
        *tokens = Some(HashMap::new());
    }
    if let Some(ref mut map) = *tokens {
        map.insert(song_id.to_string(), job_token.to_string());
    }
}

fn clear_active_job_token(song_id: &str) {
    let mut tokens = ACTIVE_JOB_TOKENS.lock().unwrap();
    if let Some(ref mut map) = *tokens {
        map.remove(song_id);
    }
}

fn remove_job(song_id: &str) {
    let mut jobs = JOBS.lock().unwrap();
    if let Some(ref mut map) = *jobs {
        map.remove(song_id);
    }
}

fn spawn_in_own_process_group(command: &mut Command) -> io::Result<std::process::Child> {
    process_control::spawn_in_own_process_group(command)
}

#[cfg(unix)]
fn terminate_process_group(pid: u32) {
    process_control::terminate_process_group(pid);
}

#[cfg(unix)]
fn force_terminate_process_group(pid: u32) {
    process_control::force_terminate_process_group(pid);
}

#[cfg(windows)]
fn terminate_process_group(pid: u32) {
    process_control::terminate_process_group(pid);
}

#[cfg(windows)]
fn force_terminate_process_group(pid: u32) {
    process_control::force_terminate_process_group(pid);
}

fn terminate_known_job(job: &JobHandle, force: bool) {
    if let Some(pid) = job.separator_pid {
        if force {
            force_terminate_process_group(pid);
        } else {
            terminate_process_group(pid);
        }
    }
}

fn terminate_song_processes(song_id: &str, force: bool) {
    #[cfg(windows)]
    {
        if let Some(job) = get_job(song_id) {
            terminate_known_job(&job, force);
        }
        return;
    }
    #[cfg(unix)]
    {
        let output = match Command::new("ps")
            .args(["-axo", "pid,pgid,command"])
            .output()
        {
            Ok(output) => output,
            Err(_) => return,
        };
        let text = String::from_utf8_lossy(&output.stdout);
        let current_pid = std::process::id() as i32;
        let mut process_groups = HashSet::new();

        for line in text.lines().skip(1) {
            if !line.contains(song_id) {
                continue;
            }
            let mut parts = line.split_whitespace();
            let pid = parts.next().and_then(|value| value.parse::<i32>().ok());
            let pgid = parts.next().and_then(|value| value.parse::<i32>().ok());
            if let (Some(pid), Some(pgid)) = (pid, pgid) {
                if pid != current_pid && pgid > 0 {
                    process_groups.insert(pgid);
                }
            }
        }

        for pgid in process_groups {
            unsafe {
                let signal = if force { libc::SIGKILL } else { libc::SIGTERM };
                let _ = libc::kill(-(pgid as libc::pid_t), signal);
            }
        }
    }
}

fn terminate_app_processing_processes(force: bool) {
    #[cfg(windows)]
    {
        let jobs = JOBS.lock().unwrap();
        if let Some(ref map) = *jobs {
            for job in map.values() {
                if let Some(pid) = job.separator_pid {
                    if force {
                        force_terminate_process_group(pid);
                    } else {
                        terminate_process_group(pid);
                    }
                }
            }
        }
        return;
    }
    #[cfg(unix)]
    {
        let output = match Command::new("ps")
            .args(["-axo", "pid,pgid,command"])
            .output()
        {
            Ok(output) => output,
            Err(_) => return,
        };
        let data_dir = get_data_dir().to_string_lossy().to_string();
        let text = String::from_utf8_lossy(&output.stdout);
        let current_pid = std::process::id() as i32;
        let mut process_groups = HashSet::new();

        for line in text.lines().skip(1) {
            let is_app_process = line.contains(&data_dir) || line.contains("4isfstools/songs");
            if !is_app_process {
                continue;
            }

            let mut parts = line.split_whitespace();
            let pid = parts.next().and_then(|value| value.parse::<i32>().ok());
            let pgid = parts.next().and_then(|value| value.parse::<i32>().ok());
            if let (Some(pid), Some(pgid)) = (pid, pgid) {
                if pid != current_pid && pgid > 0 {
                    process_groups.insert(pgid);
                }
            }
        }

        for pgid in process_groups {
            unsafe {
                let signal = if force { libc::SIGKILL } else { libc::SIGTERM };
                let _ = libc::kill(-(pgid as libc::pid_t), signal);
            }
        }
    }
}

fn cleanup_interrupted_processing_jobs() {
    JobManager::cleanup_interrupted_jobs();
}

fn cancel_active_processing_jobs(reason: &str) {
    JobManager::cancel_active_jobs(reason);
}

pub(crate) fn update_song_status(
    song_id: &str,
    status: &str,
    progress: u32,
    stage: Option<&str>,
    error: Option<&str>,
) {
    let mut songs = SONGS.lock().unwrap();
    if let Some(ref mut map) = *songs {
        if let Some(song) = map.get_mut(song_id) {
            // Once cancelled, ignore stale background writes except explicit cancelled/cancelling/pending.
            if song.status == "cancelled"
                && status != "cancelled"
                && status != "cancelling"
                && status != "pending"
            {
                return;
            }
            song.status = status.to_string();
            song.progress = progress;
            if let Some(s) = stage {
                song.processing_stage = Some(s.to_string());
            }
            if let Some(e) = error {
                song.error_message = Some(e.to_string());
            }
        }
    }
    drop(songs);
    save_songs_to_disk();
}

fn lyric_document_to_lrc(document: &LyricDocument) -> String {
    let mut lines: Vec<&LyricLineDoc> = document.lines.iter().collect();
    lines.sort_by_key(|line| line.start_ms);
    lines
        .iter()
        .filter(|line| !line.text.trim().is_empty())
        .map(|line| {
            let shifted = if document.global_offset_ms >= 0 {
                line.start_ms
                    .saturating_add(document.global_offset_ms as u64)
            } else {
                line.start_ms
                    .saturating_sub((-document.global_offset_ms) as u64)
            };
            let minutes = shifted / 60000;
            let seconds = (shifted % 60000) / 1000;
            let ms = shifted % 1000;
            format!("[{:02}:{:02}.{:03}]{}", minutes, seconds, ms, line.text)
        })
        .collect::<Vec<String>>()
        .join("\n")
}

fn normalize_folder_name(folder: Option<String>) -> Option<String> {
    folder
        .map(|value| value.trim().to_string())
        .and_then(|value| if value.is_empty() { None } else { Some(value) })
}

fn normalize_match_text(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ")
}

fn strip_bracketed_segments(text: &str) -> String {
    let mut depth = 0i32;
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '(' | '[' | '{' | '（' | '【' | '「' | '『' => {
                depth += 1;
            }
            ')' | ']' | '}' | '）' | '】' | '」' | '』' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            _ => {
                if depth == 0 {
                    out.push(ch);
                }
            }
        }
    }
    out
}

fn is_search_noise_token(token: &str) -> bool {
    matches!(
        token,
        "remix"
            | "mix"
            | "instrumental"
            | "karaoke"
            | "cover"
            | "live"
            | "official"
            | "mv"
            | "hd"
            | "demo"
            | "radio"
            | "edit"
            | "original"
            | "originally"
            | "pure"
            | "acoustic"
            | "version"
            | "track"
            | "single"
    ) || token.contains("伴奏")
        || token.contains("人声")
        || token.contains("原唱")
        || token.contains("歌词")
        || token.contains("纯音乐")
        || token.contains("完整版")
        || token.contains("原版")
        || token.contains("伴奏版")
        || token.contains("混音")
        || token.contains("混录")
}

fn clean_lyrics_search_hint(text: &str) -> String {
    let mut normalized_source = strip_bracketed_segments(text).replace(
        [
            '_', '-', '—', '–', '·', '•', '|', '/', '\\', ':', '，', '。', '！', '？', ',', '.',
        ],
        " ",
    );
    normalized_source = normalized_source.replace("feat.", " ");
    normalized_source = normalized_source.replace("ft.", " ");
    normalized_source = normalized_source.replace("Feat.", " ");
    normalized_source = normalized_source.replace("FT.", " ");
    normalized_source = normalized_source.replace("featuring", " ");
    normalized_source = normalized_source.replace(" featuring ", " ");
    let normalized = normalize_match_text(&normalized_source);
    let mut tokens = Vec::new();
    for token in normalized.split_whitespace() {
        if token.is_empty() || is_search_noise_token(token) {
            continue;
        }
        tokens.push(token.to_string());
    }
    tokens.join(" ")
}

fn clean_song_search_hint(song: &Song) -> String {
    let file_stem = Path::new(&song.name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(&song.name);
    clean_lyrics_search_hint(file_stem)
}

fn build_lyrics_search_intent(song: &Song, manual_query: Option<&str>) -> LyricsSearchIntent {
    if let Some(query) = manual_query
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let (artist_hint, track_hint) = split_artist_track_hint(query);
        let query_track = clean_lyrics_search_hint(&track_hint);
        let query_artist = artist_hint
            .as_deref()
            .map(clean_lyrics_search_hint)
            .filter(|value| !value.is_empty());
        let search_hint = match query_artist.as_deref() {
            Some(artist) if !artist.is_empty() => format!("{} - {}", artist, query_track),
            _ => query_track.clone(),
        };
        let variants = candidate_query_variants(&search_hint, &query_track);
        let allow_weak_fallback = query_track.chars().filter(|c| !c.is_whitespace()).count() > 4
            || query_artist.is_some();
        return LyricsSearchIntent {
            query_track: if query_track.is_empty() {
                track_hint
            } else {
                query_track
            },
            query_artist,
            variants,
            allow_weak_fallback,
        };
    }

    let cleaned_song_hint = clean_song_search_hint(song);
    let search_hint = if cleaned_song_hint.is_empty() {
        song.name.clone()
    } else {
        cleaned_song_hint.clone()
    };
    let variants = candidate_query_variants(&search_hint, &search_hint);
    let allow_weak_fallback = search_hint.chars().filter(|c| !c.is_whitespace()).count() > 6;
    let (query_artist, query_track) = split_artist_track_hint(&search_hint);
    LyricsSearchIntent {
        query_track: clean_lyrics_search_hint(&query_track),
        query_artist: query_artist
            .as_deref()
            .map(clean_lyrics_search_hint)
            .filter(|value| !value.is_empty()),
        variants,
        allow_weak_fallback,
    }
}

fn split_artist_track_hint(hint: &str) -> (Option<String>, String) {
    let trimmed = hint.trim();
    let separators = [" - ", " — ", " – ", " | ", " / "];
    for separator in separators {
        if let Some((artist, track)) = trimmed.split_once(separator) {
            let artist = artist.trim();
            let track = track.trim();
            if !track.is_empty() {
                return (
                    if artist.is_empty() {
                        None
                    } else {
                        Some(artist.to_string())
                    },
                    track.to_string(),
                );
            }
        }
    }
    (None, trimmed.to_string())
}

fn candidate_query_variants(
    query_hint: &str,
    fallback_hint: &str,
) -> Vec<(Option<String>, String)> {
    let mut variants = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let push_variant = |artist: Option<String>,
                        track: String,
                        variants: &mut Vec<(Option<String>, String)>,
                        seen: &mut std::collections::HashSet<String>| {
        let normalized_key = format!(
            "{}::{}",
            artist
                .as_deref()
                .map(normalize_match_text)
                .unwrap_or_default(),
            normalize_match_text(&track),
        );
        if seen.insert(normalized_key) {
            variants.push((artist, track));
        }
    };

    let build_variants = |hint: &str,
                          variants: &mut Vec<(Option<String>, String)>,
                          seen: &mut std::collections::HashSet<String>| {
        let trimmed = clean_lyrics_search_hint(hint);
        if trimmed.is_empty() {
            return;
        }

        push_variant(None, trimmed.to_string(), variants, seen);

        let (artist, track) = split_artist_track_hint(&trimmed);
        push_variant(artist.clone(), track.clone(), variants, seen);

        let mut stripped = trimmed.to_string();
        let removals = [("（", "）"), ("(", ")"), ("【", "】"), ("[", "]")];
        for (open, close) in removals {
            while let Some(start) = stripped.find(open) {
                if let Some(end) = stripped[start + open.len()..].find(close) {
                    let end = start + open.len() + end + close.len();
                    stripped.replace_range(start..end, "");
                } else {
                    break;
                }
            }
        }
        let stripped = stripped
            .replace("feat.", "")
            .replace("ft.", "")
            .replace("Feat.", "")
            .replace("FT.", "")
            .replace(" featuring ", " ")
            .replace("原唱", " ")
            .replace("伴奏", " ")
            .replace("人声", " ")
            .replace("歌词", " ")
            .replace(" ", " ")
            .trim()
            .to_string();
        if !stripped.is_empty() && stripped != trimmed {
            push_variant(None, stripped.clone(), variants, seen);
            let (artist, track) = split_artist_track_hint(&stripped);
            push_variant(artist, track, variants, seen);
        }

        let normalized_tokens = normalize_match_text(&trimmed);
        let simple_tokens = normalized_tokens
            .split_whitespace()
            .filter(|token| token.len() >= 2)
            .take(4)
            .map(|token| token.to_string())
            .collect::<Vec<String>>();
        if simple_tokens.len() >= 2 {
            let collapsed = simple_tokens.join(" ");
            push_variant(None, collapsed, variants, seen);

            let cjk_tokens = simple_tokens
                .iter()
                .filter(|token| {
                    token.chars().any(|ch| {
                        ('\u{4e00}'..='\u{9fff}').contains(&ch)
                            || ('\u{3400}'..='\u{4dbf}').contains(&ch)
                    })
                })
                .cloned()
                .collect::<Vec<String>>();
            if !cjk_tokens.is_empty() {
                let cjk_collapsed = cjk_tokens.join(" ");
                push_variant(None, cjk_collapsed, variants, seen);
            }
        }

        if simple_tokens.len() == 1 {
            push_variant(None, simple_tokens[0].clone(), variants, seen);
        }
    };

    build_variants(query_hint, &mut variants, &mut seen);
    if normalize_match_text(query_hint) != normalize_match_text(fallback_hint) {
        build_variants(fallback_hint, &mut variants, &mut seen);
    }

    variants
}

fn extract_fallback_keywords(hint: &str) -> Vec<String> {
    let normalized = normalize_match_text(&clean_lyrics_search_hint(hint));
    normalized
        .split_whitespace()
        .filter(|token| token.len() >= 2)
        .map(|token| token.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn sample_document() -> LyricDocument {
        build_document_from_plain_lines("song_1", "test", "test", None, "hello world", 0.5).unwrap()
    }

    fn sample_candidate(
        title: &str,
        artist: Option<&str>,
        source: &str,
        score: i32,
    ) -> LyricsCandidate {
        LyricsCandidate {
            id: format!("{}::{}", source, title),
            source: source.to_string(),
            source_label: source.to_string(),
            title: title.to_string(),
            artist: artist.map(|value| value.to_string()),
            album: None,
            score,
            synced: false,
            preview: "hello world".to_string(),
            document: sample_document(),
        }
    }

    #[test]
    fn clean_song_hint_drops_noise_suffixes() {
        let song = Song {
            id: "song_1".to_string(),
            name: "isis_临渊_remix.wav".to_string(),
            original_path: "/tmp/isis_临渊_remix.wav".to_string(),
            playlist_folder: None,
            vocals_path: None,
            instrumental_path: None,
            original_mix_path: None,
            lyrics_path: None,
            duration: 0,
            status: "pending".to_string(),
            progress: 0,
            processing_stage: None,
            error_message: None,
            added_at: 0,
        };
        let cleaned = clean_song_search_hint(&song);
        assert!(cleaned.contains("临渊"));
        assert!(!cleaned.contains("remix"));
    }

    #[test]
    fn auto_search_intent_extracts_cjk_suffix_variant() {
        let song = Song {
            id: "song_1".to_string(),
            name: "isis_临渊_remix.wav".to_string(),
            original_path: "/tmp/isis_临渊_remix.wav".to_string(),
            playlist_folder: None,
            vocals_path: None,
            instrumental_path: None,
            original_mix_path: None,
            lyrics_path: None,
            duration: 0,
            status: "pending".to_string(),
            progress: 0,
            processing_stage: None,
            error_message: None,
            added_at: 0,
        };
        let intent = build_lyrics_search_intent(&song, None);
        assert!(intent
            .variants
            .iter()
            .any(|(_, track)| normalize_match_text(track).contains("临渊")));
    }

    #[test]
    fn manual_search_short_query_does_not_allow_weak_fallback() {
        let song = Song {
            id: "song_1".to_string(),
            name: "爱你.wav".to_string(),
            original_path: "/tmp/爱你.wav".to_string(),
            playlist_folder: None,
            vocals_path: None,
            instrumental_path: None,
            original_mix_path: None,
            lyrics_path: None,
            duration: 0,
            status: "pending".to_string(),
            progress: 0,
            processing_stage: None,
            error_message: None,
            added_at: 0,
        };
        let intent = build_lyrics_search_intent(&song, Some("爱你"));
        assert_eq!(intent.query_track, "爱你");
        assert!(intent.query_artist.is_none());
        assert!(!intent.allow_weak_fallback);
    }

    #[test]
    fn rank_lyrics_candidates_filters_obvious_noise_when_no_weak_fallback() {
        let ranked = rank_lyrics_candidates(
            vec![sample_candidate(
                "432赫兹",
                Some("Thomas Dallan"),
                "netease",
                20,
            )],
            "爱你",
            None,
            false,
        );
        assert!(ranked.is_empty());
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{}_{}_{}", prefix, std::process::id(), stamp));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn format_missing_core_components_filters_optional_checks() {
        let health = RuntimeHealthReport {
            level: "error".to_string(),
            label: "环境异常".to_string(),
            detail: "core missing".to_string(),
            separation_engine: SeparationEngineHealth::default(),
            torch_cuda_available: false,
            selected_device: "cpu".to_string(),
            torch_version: None,
            torch_cuda_version: None,
            torch_cuda_device_name: None,
            has_nvidia_gpu: false,
            nvidia_driver_visible: false,
            nvidia_driver_cuda_version: None,
            checks: vec![
                RuntimeHealthCheck {
                    name: "FFmpeg".to_string(),
                    ok: false,
                    severity: "error".to_string(),
                    detail: Some("audio".to_string()),
                },
                RuntimeHealthCheck {
                    name: "AI 听写草稿".to_string(),
                    ok: false,
                    severity: "info".to_string(),
                    detail: Some("optional".to_string()),
                },
                RuntimeHealthCheck {
                    name: "Torch CUDA".to_string(),
                    ok: false,
                    severity: "info".to_string(),
                    detail: Some("optional".to_string()),
                },
                RuntimeHealthCheck {
                    name: "NVIDIA GPU".to_string(),
                    ok: false,
                    severity: "info".to_string(),
                    detail: Some("optional".to_string()),
                },
            ],
        };

        let missing = format_missing_core_components_with_reason(&health);
        assert_eq!(missing, "FFmpeg（audio）");
    }

    #[test]
    fn verify_manifest_targets_rejects_hash_mismatch() {
        let runtime_models = unique_temp_dir("manifest_targets");
        let target_relpath = "onnx/test-model.th";
        let target_path = runtime_models.join(target_relpath);
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&target_path, b"hello world").unwrap();

        let sources = vec![RuntimeManifestArtifact {
            url: "https://example.com/test-model.th".to_string(),
            sha256: Some(
                "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            ),
            note: None,
            target_relpath: Some(target_relpath.to_string()),
            inline_text: None,
        }];

        let result = verify_manifest_targets(&runtime_models, &sources);
        assert!(result.is_err());

        let _ = fs::remove_dir_all(&runtime_models);
    }

    #[test]
    fn cuda_version_mapping_covers_supported_ranges() {
        assert_eq!(
            crate::runtime::capability::cuda_version_to_pytorch_index("12.4"),
            "cu124"
        );
        assert_eq!(
            crate::runtime::capability::cuda_version_to_pytorch_index("12.8"),
            "cu124"
        );
        assert_eq!(
            crate::runtime::capability::cuda_version_to_pytorch_index("12.1"),
            "cu121"
        );
        assert_eq!(
            crate::runtime::capability::cuda_version_to_pytorch_index("11.8"),
            "cu118"
        );
    }
}

fn normalized_overlap_ratio(left: &str, right: &str) -> f32 {
    let left_norm = normalize_match_text(left);
    let right_norm = normalize_match_text(right);
    if left_norm.is_empty() || right_norm.is_empty() {
        return 0.0;
    }

    let left_chars = left_norm
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<Vec<char>>();
    if left_chars.is_empty() {
        return 0.0;
    }

    let right_chars = right_norm
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<std::collections::HashSet<char>>();

    let mut seen = std::collections::HashSet::new();
    let mut hit = 0usize;
    for ch in left_chars {
        if seen.insert(ch) && right_chars.contains(&ch) {
            hit += 1;
        }
    }

    hit as f32 / seen.len().max(1) as f32
}

fn lyrics_candidate_source_priority(source: &str) -> i32 {
    match source {
        "lrclib" => 3,
        "netease" => 2,
        "qq" => 1,
        _ => 0,
    }
}

fn classify_lyrics_candidate_tier(
    query_track: &str,
    query_artist: Option<&str>,
    candidate_track: &str,
    candidate_artist: &str,
) -> LyricsCandidateTier {
    let query_track_norm = normalize_match_text(query_track);
    let candidate_track_norm = normalize_match_text(candidate_track);
    let query_artist_norm = query_artist.map(normalize_match_text).unwrap_or_default();
    let candidate_artist_norm = normalize_match_text(candidate_artist);

    if query_track_norm.is_empty() {
        return LyricsCandidateTier::Weak;
    }

    if query_track_norm == candidate_track_norm
        || candidate_track_norm.contains(&query_track_norm)
        || query_track_norm.contains(&candidate_track_norm)
    {
        return LyricsCandidateTier::Strong;
    }

    let track_overlap = normalized_overlap_ratio(&query_track_norm, &candidate_track_norm);
    if track_overlap >= 0.62 {
        return LyricsCandidateTier::Strong;
    }
    if track_overlap >= 0.35 {
        return LyricsCandidateTier::Acceptable;
    }

    if !query_artist_norm.is_empty() {
        if query_artist_norm == candidate_artist_norm
            || candidate_artist_norm.contains(&query_artist_norm)
            || query_artist_norm.contains(&candidate_artist_norm)
        {
            return LyricsCandidateTier::Acceptable;
        }
        if normalized_overlap_ratio(&query_artist_norm, &candidate_artist_norm) >= 0.5 {
            return LyricsCandidateTier::Acceptable;
        }
    }

    LyricsCandidateTier::Weak
}

fn rank_lyrics_candidates(
    candidates: Vec<LyricsCandidate>,
    query_track: &str,
    query_artist: Option<&str>,
    allow_weak_fallback: bool,
) -> Vec<LyricsCandidate> {
    let mut strong = Vec::new();
    let mut acceptable = Vec::new();
    let mut weak = Vec::new();

    for candidate in candidates {
        let tier = classify_lyrics_candidate_tier(
            query_track,
            query_artist,
            &candidate.title,
            candidate.artist.as_deref().unwrap_or_default(),
        );
        let text_score = score_text_relevance(
            query_track,
            query_artist,
            &candidate.title,
            candidate.artist.as_deref().unwrap_or_default(),
        );
        let source_priority = lyrics_candidate_source_priority(&candidate.source);
        let source_score = candidate.score;
        let display_score = text_score
            .saturating_mul(10)
            .saturating_add(source_priority * 4)
            .saturating_add((source_score / 10).clamp(0, 50))
            .saturating_add(if candidate.synced { 12 } else { 0 });
        let mut candidate = candidate;
        candidate.score = display_score;
        let bucket = (text_score, source_priority, source_score, candidate.synced);

        match tier {
            LyricsCandidateTier::Strong => strong.push((bucket, candidate)),
            LyricsCandidateTier::Acceptable => acceptable.push((bucket, candidate)),
            LyricsCandidateTier::Weak => weak.push((bucket, candidate)),
        }
    }

    let sort_bucket = |items: &mut Vec<((i32, i32, i32, bool), LyricsCandidate)>| {
        items.sort_by(|a, b| {
            b.0 .0
                .cmp(&a.0 .0)
                .then_with(|| b.0 .1.cmp(&a.0 .1))
                .then_with(|| b.0 .2.cmp(&a.0 .2))
                .then_with(|| b.0 .3.cmp(&a.0 .3))
        });
    };
    sort_bucket(&mut strong);
    sort_bucket(&mut acceptable);
    sort_bucket(&mut weak);

    let mut result = Vec::new();

    if !strong.is_empty() {
        for (_, candidate) in strong.into_iter() {
            result.push(candidate);
            if result.len() >= 8 {
                return result;
            }
        }
        for (_, candidate) in acceptable.into_iter() {
            result.push(candidate);
            if result.len() >= 8 {
                return result;
            }
        }
        return result;
    }

    if !acceptable.is_empty() {
        for (_, candidate) in acceptable.into_iter() {
            result.push(candidate);
            if result.len() >= 8 {
                return result;
            }
        }
        return result;
    }

    if !allow_weak_fallback {
        return result;
    }

    let weak_floor = if normalize_match_text(query_track)
        .chars()
        .filter(|c| !c.is_whitespace())
        .count()
        <= 4
    {
        10
    } else {
        5
    };
    for (_, candidate) in weak.into_iter() {
        let text_score = score_text_relevance(
            query_track,
            query_artist,
            &candidate.title,
            candidate.artist.as_deref().unwrap_or_default(),
        );
        if text_score < weak_floor {
            continue;
        }
        result.push(candidate);
        if result.len() >= 8 {
            break;
        }
    }

    result
}

fn score_text_relevance(
    query_track: &str,
    query_artist: Option<&str>,
    candidate_track: &str,
    candidate_artist: &str,
) -> i32 {
    let query_track_norm = normalize_match_text(query_track);
    let candidate_track_norm = normalize_match_text(candidate_track);
    let query_artist_norm = query_artist.map(normalize_match_text);
    let candidate_artist_norm = normalize_match_text(candidate_artist);
    let query_track_len = query_track_norm
        .chars()
        .filter(|c| !c.is_whitespace())
        .count();

    let mut score = 0;
    let mut track_hit = false;
    let mut artist_hit = false;

    if !query_track_norm.is_empty() {
        if query_track_norm == candidate_track_norm {
            score += 150;
            track_hit = true;
        } else if candidate_track_norm.contains(&query_track_norm)
            || query_track_norm.contains(&candidate_track_norm)
        {
            score += 90;
            track_hit = true;
        } else {
            let overlap = normalized_overlap_ratio(&query_track_norm, &candidate_track_norm);
            if overlap >= 0.75 {
                score += 45;
                track_hit = true;
            } else if overlap >= 0.5 {
                score += 22;
            } else if overlap > 0.0 {
                score -= 18;
            } else if query_track_len <= 4 {
                score -= 80;
            } else {
                score -= 55;
            }
        }
    }

    if let Some(query_artist_norm) = query_artist_norm {
        if !query_artist_norm.is_empty() {
            if query_artist_norm == candidate_artist_norm {
                score += 35;
                artist_hit = true;
            } else if candidate_artist_norm.contains(&query_artist_norm)
                || query_artist_norm.contains(&candidate_artist_norm)
            {
                score += 18;
                artist_hit = true;
            } else {
                let overlap = normalized_overlap_ratio(&query_artist_norm, &candidate_artist_norm);
                if overlap >= 0.75 {
                    score += 8;
                    artist_hit = true;
                } else if overlap >= 0.5 {
                    score += 4;
                } else if overlap > 0.0 {
                    score -= 12;
                } else {
                    score -= 22;
                }
            }
        }
    }

    if (!query_track_norm.is_empty() || query_artist.is_some()) && !track_hit && !artist_hit {
        score -= if query_track_len <= 4 { 40 } else { 25 };
    }

    score
}

fn parse_lrclib_timestamp(timestamp: &str) -> Option<u64> {
    let (minutes_part, seconds_part) = timestamp.split_once(':')?;
    let minutes = minutes_part.trim().parse::<u64>().ok()?;
    let (seconds_str, fraction_str) =
        if let Some((seconds, fraction)) = seconds_part.split_once('.') {
            (seconds, fraction)
        } else if let Some((seconds, fraction)) = seconds_part.split_once(',') {
            (seconds, fraction)
        } else {
            (seconds_part, "0")
        };
    let seconds = seconds_str.trim().parse::<u64>().ok()?;
    let fraction = fraction_str.trim();
    let milliseconds = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<u64>().ok()? * 100,
        2 => fraction.parse::<u64>().ok()? * 10,
        _ => fraction[..3.min(fraction.len())].parse::<u64>().ok()?,
    };
    Some(minutes.saturating_mul(60_000) + seconds.saturating_mul(1_000) + milliseconds.min(999))
}

fn parse_lrclib_synced_lines(content: &str) -> Vec<(u64, String)> {
    let mut lines = Vec::new();
    for raw_line in content.lines() {
        let mut remainder = raw_line.trim();
        if remainder.is_empty() {
            continue;
        }
        let mut timestamps = Vec::new();
        while remainder.starts_with('[') {
            let Some(end_idx) = remainder.find(']') else {
                break;
            };
            let timestamp = &remainder[1..end_idx];
            timestamps.push(timestamp.to_string());
            remainder = remainder[end_idx + 1..].trim_start();
        }
        let text = remainder.trim();
        if text.is_empty() || timestamps.is_empty() {
            continue;
        }
        for timestamp in timestamps {
            if let Some(ms) = parse_lrclib_timestamp(&timestamp) {
                lines.push((ms, text.to_string()));
            }
        }
    }
    lines.sort_by_key(|(start_ms, _)| *start_ms);
    lines
}

fn build_document_from_timed_lines(
    song_id: &str,
    source: &str,
    alignment_engine: &str,
    language: Option<String>,
    timed_lines: Vec<(u64, String)>,
    quality_score: f32,
) -> Option<LyricDocument> {
    let mut normalized: Vec<(u64, String)> = timed_lines
        .into_iter()
        .filter_map(|(start_ms, text)| {
            let text = text.trim().to_string();
            if text.is_empty() {
                None
            } else {
                Some((start_ms, text))
            }
        })
        .collect();
    normalized.sort_by_key(|(start_ms, _)| *start_ms);
    if normalized.is_empty() {
        return None;
    }

    let now_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let mut doc_lines = Vec::new();

    for (idx, (start_ms, text)) in normalized.iter().enumerate() {
        let next_start = normalized
            .get(idx + 1)
            .map(|(value, _)| *value)
            .unwrap_or(start_ms.saturating_add(2500));
        let end_ms = next_start
            .saturating_sub(50)
            .max(start_ms.saturating_add(300));
        let token = LyricToken {
            id: format!("token_{}_0", idx),
            line_id: format!("line_{}", idx),
            index: 0,
            text: text.clone(),
            start_ms: *start_ms,
            end_ms,
            confidence: 0.9,
            kind: "word".to_string(),
        };
        doc_lines.push(LyricLineDoc {
            id: format!("line_{}", idx),
            index: idx as u32,
            start_ms: *start_ms,
            end_ms,
            text: text.clone(),
            confidence: 0.9,
            edited: false,
            locked: false,
            tokens: vec![token],
        });
    }

    Some(LyricDocument {
        song_id: song_id.to_string(),
        version: 1,
        language,
        source: source.to_string(),
        alignment_engine: alignment_engine.to_string(),
        created_at: now_ts,
        updated_at: now_ts,
        global_offset_ms: 0,
        dirty: false,
        quality_score,
        lines: doc_lines,
    })
}

fn build_document_from_plain_lines(
    song_id: &str,
    source: &str,
    alignment_engine: &str,
    language: Option<String>,
    plain_lyrics: &str,
    quality_score: f32,
) -> Option<LyricDocument> {
    let timed_lines = plain_lyrics
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .enumerate()
        .map(|(idx, text)| ((idx as u64) * 2500, text.to_string()))
        .collect::<Vec<(u64, String)>>();
    build_document_from_timed_lines(
        song_id,
        source,
        alignment_engine,
        language,
        timed_lines,
        quality_score,
    )
}

fn build_lrclib_document(
    song_id: &str,
    track: &LrclibTrack,
    use_synced: bool,
) -> Option<LyricDocument> {
    if use_synced {
        if let Some(synced) = track.synced_lyrics.as_deref() {
            let timed_lines = parse_lrclib_synced_lines(synced);
            if !timed_lines.is_empty() {
                return build_document_from_timed_lines(
                    song_id,
                    "lrclib",
                    "lrclib_synced",
                    None,
                    timed_lines,
                    0.96,
                );
            }
        }
    }

    if let Some(plain) = track.plain_lyrics.as_deref() {
        return build_document_from_plain_lines(
            song_id,
            "lrclib",
            "lrclib_plain",
            None,
            plain,
            0.86,
        );
    }

    None
}

fn build_netease_document(
    song_id: &str,
    synced_lyrics: Option<&str>,
    plain_lyrics: Option<&str>,
) -> Option<LyricDocument> {
    if let Some(synced) = synced_lyrics {
        let timed_lines = parse_lrclib_synced_lines(synced);
        if !timed_lines.is_empty() {
            return build_document_from_timed_lines(
                song_id,
                "netease",
                "netease_synced",
                None,
                timed_lines,
                0.94,
            );
        }
    }

    if let Some(plain) = plain_lyrics {
        return build_document_from_plain_lines(
            song_id,
            "netease",
            "netease_plain",
            None,
            plain,
            0.84,
        );
    }

    None
}

fn score_netease_song(
    query_track: &str,
    query_artist: Option<&str>,
    query_duration_ms: u64,
    song: &NeteaseSong,
    has_synced: bool,
) -> i32 {
    let mut score = 0;
    let candidate_track = song.name.as_str();
    let candidate_artist = song
        .artists
        .first()
        .and_then(|artist| artist.name.as_deref())
        .unwrap_or_default();
    score += score_text_relevance(query_track, query_artist, candidate_track, candidate_artist);

    if has_synced {
        score += 20;
    }

    if query_duration_ms > 0 {
        if let Some(candidate_duration_ms) = song.duration {
            let diff = (candidate_duration_ms as f64 - query_duration_ms as f64).abs() / 1000.0;
            if diff <= 2.0 {
                score += 20;
            } else if diff < 6.0 {
                score += 12;
            } else if diff < 12.0 {
                score += 4;
            } else {
                score -= (diff * 3.0) as i32;
            }
        }
    }

    score
}

fn fetch_netease_candidates(
    song_id: &str,
    query_track: &str,
    query_artist: Option<&str>,
    query_duration_ms: u64,
) -> Result<Vec<LyricsCandidate>, String> {
    let cache_key = lyrics_search_cache_key(
        "netease",
        song_id,
        query_track,
        query_artist,
        query_duration_ms,
    );

    fetch_with_lyrics_cache(cache_key, || {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(4))
            .user_agent("Macaron Singer/1.0 (+https://github.com/suntong/4isfstools)")
            .build()
            .map_err(|e| format!("Failed to build netease client: {}", e))?;

        let mut search_queries = vec![query_track.to_string()];
        for token in extract_fallback_keywords(query_track) {
            if !search_queries
                .iter()
                .any(|q| normalize_match_text(q) == normalize_match_text(&token))
            {
                search_queries.push(token);
            }
        }
        search_queries.truncate(4);

        let mut songs = Vec::new();
        for query in search_queries {
            let response = match client
                .post("https://music.163.com/api/search/get/")
                .header("Referer", "https://music.163.com")
                .header("Origin", "https://music.163.com")
                .form(&[
                    ("s", query.as_str()),
                    ("limit", "10"),
                    ("type", "1"),
                    ("offset", "0"),
                ])
                .send()
            {
                Ok(response) if response.status().is_success() => response,
                _ => continue,
            };

            let parsed = match response.json::<NeteaseSearchResponse>() {
                Ok(parsed) => parsed,
                Err(e) => return Err(format!("Failed to parse NetEase search response: {}", e)),
            };

            if let Some(result) = parsed.result {
                songs.extend(result.songs);
            }

            if songs.len() >= 12 {
                break;
            }
        }

        if songs.is_empty() {
            return Ok(Vec::new());
        }

        let mut scored = Vec::new();
        for song in songs {
            let metadata_score =
                score_netease_song(query_track, query_artist, query_duration_ms, &song, false);
            let artist = song.artists.first().and_then(|value| value.name.clone());
            let album = song.album.as_ref().and_then(|value| value.name.clone());
            scored.push((metadata_score, song, artist, album));
        }

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        let mut filtered = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (score, song, artist, album) in scored.into_iter() {
            let key = format!(
                "{}::{}::{}",
                normalize_match_text(artist.as_deref().unwrap_or_default()),
                normalize_match_text(&song.name),
                song.duration.unwrap_or_default()
            );
            if !seen.insert(key) {
                continue;
            }
            filtered.push((score, song, artist, album));
            if filtered.len() >= 6 {
                break;
            }
        }

        let mut scored = Vec::new();
        for (base_score, song, artist, album) in filtered {
            let lyric_response = match client
                .get("https://music.163.com/api/song/lyric")
                .header("Referer", "https://music.163.com")
                .header("Origin", "https://music.163.com")
                .query(&[
                    ("lv", "-1"),
                    ("tv", "-1"),
                    ("kv", "-1"),
                    ("id", &song.id.to_string()),
                ])
                .send()
            {
                Ok(response) if response.status().is_success() => response,
                _ => continue,
            };

            let lyric_payload = match lyric_response.json::<NeteaseLyricResponse>() {
                Ok(payload) => payload,
                Err(_) => continue,
            };

            let synced_lyrics = lyric_payload
                .lrc
                .as_ref()
                .and_then(|block| block.lyric.as_deref())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);

            let plain_lyrics = lyric_payload
                .tlyric
                .as_ref()
                .and_then(|block| block.lyric.as_deref())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);

            let has_synced = synced_lyrics
                .as_deref()
                .map(|value| !parse_lrclib_synced_lines(value).is_empty())
                .unwrap_or(false);
            let score = base_score + if has_synced { 18 } else { 0 };

            if let Some(document) =
                build_netease_document(song_id, synced_lyrics.as_deref(), plain_lyrics.as_deref())
            {
                scored.push((score, song, artist, album, has_synced, document));
            }
        }

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        let mut result = Vec::new();
        for (score, song, artist, album, has_synced, document) in scored {
            result.push(LyricsCandidate {
                id: format!("netease::{}", song.id),
                source: "netease".to_string(),
                source_label: "163MusicLyrics · 网易云".to_string(),
                title: song.name.clone(),
                artist,
                album,
                score,
                synced: has_synced,
                preview: preview_document(&document, 3),
                document,
            });
            if result.len() >= 10 {
                break;
            }
        }

        Ok(result)
    })
}

fn build_qq_document(song_id: &str, lyric: Option<&str>) -> Option<LyricDocument> {
    let Some(lyric) = lyric.map(str::trim).filter(|value| !value.is_empty()) else {
        return None;
    };
    let timed_lines = parse_lrclib_synced_lines(lyric);
    if !timed_lines.is_empty() {
        return build_document_from_timed_lines(
            song_id,
            "qq",
            "qq_synced",
            None,
            timed_lines,
            0.93,
        );
    }
    build_document_from_plain_lines(song_id, "qq", "qq_plain", None, lyric, 0.80)
}

fn score_qq_song(
    query_track: &str,
    query_artist: Option<&str>,
    query_duration_ms: u64,
    song: &QqSong,
    has_synced: bool,
) -> i32 {
    let mut score = 0;
    let candidate_track = song.songname.as_deref().unwrap_or_default();
    let candidate_artist = song
        .singer
        .first()
        .and_then(|artist| artist.name.as_deref())
        .unwrap_or_default();
    score += score_text_relevance(query_track, query_artist, candidate_track, candidate_artist);

    if has_synced {
        score += 20;
    }

    if query_duration_ms > 0 {
        if let Some(candidate_duration_ms) = song.interval {
            let diff =
                (candidate_duration_ms as f64 * 1000.0 - query_duration_ms as f64).abs() / 1000.0;
            if diff <= 2.0 {
                score += 20;
            } else if diff < 6.0 {
                score += 12;
            } else if diff < 12.0 {
                score += 4;
            } else {
                score -= (diff * 3.0) as i32;
            }
        }
    }

    score
}

fn fetch_qq_candidates(
    song_id: &str,
    query_track: &str,
    query_artist: Option<&str>,
    query_duration_ms: u64,
) -> Result<Vec<LyricsCandidate>, String> {
    let cache_key =
        lyrics_search_cache_key("qq", song_id, query_track, query_artist, query_duration_ms);

    fetch_with_lyrics_cache(cache_key, || {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(4))
            .user_agent("Macaron Singer/1.0 (+https://github.com/suntong/4isfstools)")
            .build()
            .map_err(|e| format!("Failed to build qq client: {}", e))?;

        let mut search_queries = vec![query_track.to_string()];
        for token in extract_fallback_keywords(query_track) {
            if !search_queries
                .iter()
                .any(|q| normalize_match_text(q) == normalize_match_text(&token))
            {
                search_queries.push(token);
            }
        }
        search_queries.truncate(4);

        let mut songs = Vec::new();
        for query in search_queries {
            let response = match client
                .get("https://c.y.qq.com/soso/fcgi-bin/client_search_cp")
                .header("Referer", "https://y.qq.com/portal/player.html")
                .header("Origin", "https://y.qq.com")
                .query(&[
                    ("p", "1"),
                    ("n", "10"),
                    ("w", query.as_str()),
                    ("format", "json"),
                    ("inCharset", "utf8"),
                    ("outCharset", "utf-8"),
                    ("notice", "0"),
                    ("platform", "yqq"),
                    ("needNewCode", "0"),
                ])
                .send()
            {
                Ok(response) if response.status().is_success() => response,
                _ => continue,
            };

            let body = match response.text() {
                Ok(text) => text,
                Err(e) => return Err(format!("Failed to read QQ search response: {}", e)),
            };
            let parsed = match parse_jsonp_or_json::<QqSearchResponse>(&body) {
                Ok(parsed) => parsed,
                Err(e) => return Err(format!("Failed to parse QQ search response: {}", e)),
            };

            if let Some(data) = parsed.data {
                if let Some(container) = data.song {
                    songs.extend(container.list);
                }
            }

            if songs.len() >= 12 {
                break;
            }
        }

        if songs.is_empty() {
            return Ok(Vec::new());
        }

        let mut scored = Vec::new();
        for song in songs {
            let Some(songmid) = song
                .songmid
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_string())
            else {
                continue;
            };

            let metadata_score =
                score_qq_song(query_track, query_artist, query_duration_ms, &song, false);
            let artist = song.singer.first().and_then(|value| value.name.clone());
            let album = song.albumname.clone();
            scored.push((metadata_score, song, artist, album, songmid));
        }

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        let mut filtered = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (score, song, artist, album, songmid) in scored.into_iter() {
            let key = format!(
                "{}::{}::{}",
                normalize_match_text(artist.as_deref().unwrap_or_default()),
                normalize_match_text(song.songname.as_deref().unwrap_or_default()),
                song.interval.unwrap_or_default()
            );
            if !seen.insert(key) {
                continue;
            }
            filtered.push((score, song, artist, album, songmid));
            if filtered.len() >= 6 {
                break;
            }
        }

        let mut scored = Vec::new();
        for (base_score, song, artist, album, songmid) in filtered {
            let lyric_response = match client
                .get("https://c.y.qq.com/lyric/fcgi-bin/fcg_query_lyric_new.fcg")
                .header("Referer", "https://y.qq.com/portal/player.html")
                .header("Origin", "https://y.qq.com")
                .query(&[
                    ("songmid", songmid.as_str()),
                    ("format", "json"),
                    ("nobase64", "1"),
                    ("platform", "yqq"),
                    ("needNewCode", "0"),
                    ("pcachetime", &now_ms().to_string()),
                ])
                .send()
            {
                Ok(response) if response.status().is_success() => response,
                _ => continue,
            };

            let body = match lyric_response.text() {
                Ok(text) => text,
                Err(_) => continue,
            };
            let lyric_text = parse_jsonp_or_json::<serde_json::Value>(&body)
                .ok()
                .and_then(|value| {
                    value
                        .get("lyric")
                        .and_then(|v| v.as_str())
                        .or_else(|| {
                            value
                                .get("data")
                                .and_then(|d| d.get("lyric"))
                                .and_then(|v| v.as_str())
                        })
                        .map(|s| s.to_string())
                });

            let has_synced = lyric_text
                .as_deref()
                .map(|value| !parse_lrclib_synced_lines(value).is_empty())
                .unwrap_or(false);
            let score = base_score + if has_synced { 18 } else { 0 };

            if let Some(document) = build_qq_document(song_id, lyric_text.as_deref()) {
                scored.push((score, song, artist, album, has_synced, document));
            }
        }

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        let mut result = Vec::new();

        for (score, song, artist, album, has_synced, document) in scored {
            result.push(LyricsCandidate {
                id: song
                    .songmid
                    .as_deref()
                    .map(|id| format!("qq::{}", id))
                    .unwrap_or_else(|| format!("qq::{}::{}", song_id, result.len())),
                source: "qq".to_string(),
                source_label: "163MusicLyrics · QQ 音乐".to_string(),
                title: song
                    .songname
                    .clone()
                    .unwrap_or_else(|| query_track.to_string()),
                artist,
                album,
                score,
                synced: has_synced,
                preview: preview_document(&document, 3),
                document,
            });

            if result.len() >= 10 {
                break;
            }
        }

        Ok(result)
    })
}

fn resolve_whisper_base_model_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let root = get_models_dir(app)
        .join("whisper")
        .join("models--Systran--faster-whisper-base");

    let refs_main = root.join("refs").join("main");
    if refs_main.exists() {
        let hash = fs::read_to_string(&refs_main)
            .map_err(|e| format!("Failed to read whisper base ref: {}", e))?
            .trim()
            .to_string();
        if !hash.is_empty() {
            let snapshot = root.join("snapshots").join(hash);
            if snapshot.exists() {
                let model_bin = snapshot.join("model.bin");
                let tokenizer_json = snapshot.join("tokenizer.json");
                if model_bin.exists()
                    && tokenizer_json.exists()
                    && looks_like_json_file(&tokenizer_json)
                {
                    return Ok(snapshot);
                }
            }
        }
    }

    let blobs_dir = root.join("blobs");
    if blobs_dir.exists() {
        // Some copy flows may lose snapshot symlinks; rebuild a materialized snapshot from blobs.
        let fallback_snapshot = root.join("snapshots").join("recovered-local-copy");
        let model_blob =
            blobs_dir.join("d01c3014881c9c6f3133c182f3d2887eb6ca1c789a7538c5c007196857a0a6a9");
        // faster-whisper-base blobs:
        // 7818... => tokenizer.json
        // c907... => vocabulary.txt
        let tokenizer_blob = blobs_dir.join("7818adb6de9fa3064d3ff81226fdd675be1f6344");
        let config_blob = blobs_dir.join("867cf1a0fece1394e01d55e287ba2f09a577c046");
        let vocab_blob = blobs_dir.join("c9074644d9d1205686f16d411564729461324b75");
        if model_blob.exists()
            && tokenizer_blob.exists()
            && config_blob.exists()
            && vocab_blob.exists()
        {
            let _ = fs::create_dir_all(&fallback_snapshot);
            let copy_or_keep = |src: &Path, dst: &Path| -> Result<(), String> {
                if !dst.exists() {
                    fs::copy(src, dst).map_err(|e| {
                        format!(
                            "Failed to recover whisper snapshot file {}: {}",
                            dst.to_string_lossy(),
                            e
                        )
                    })?;
                }
                Ok(())
            };
            copy_or_keep(&model_blob, &fallback_snapshot.join("model.bin"))?;
            copy_or_keep(&tokenizer_blob, &fallback_snapshot.join("tokenizer.json"))?;
            copy_or_keep(&config_blob, &fallback_snapshot.join("config.json"))?;
            copy_or_keep(&vocab_blob, &fallback_snapshot.join("vocabulary.txt"))?;
            let recovered_tokenizer = fallback_snapshot.join("tokenizer.json");
            if fallback_snapshot.join("model.bin").exists()
                && recovered_tokenizer.exists()
                && looks_like_json_file(&recovered_tokenizer)
            {
                return Ok(fallback_snapshot);
            }
        }
    }

    let snapshots_dir = root.join("snapshots");
    if snapshots_dir.exists() {
        let mut snapshots = fs::read_dir(&snapshots_dir)
            .map_err(|e| format!("Failed to inspect whisper base snapshots: {}", e))?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect::<Vec<PathBuf>>();
        snapshots.sort();
        for snapshot in snapshots {
            let model_bin = snapshot.join("model.bin");
            let tokenizer_json = snapshot.join("tokenizer.json");
            if model_bin.exists()
                && tokenizer_json.exists()
                && looks_like_json_file(&tokenizer_json)
            {
                return Ok(snapshot);
            }
        }
    }

    Err("未找到 Whisper base 模型，请重新下载或检查 models 目录".to_string())
}

fn looks_like_json_file(path: &Path) -> bool {
    let bytes = match fs::read(path) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let first = bytes.into_iter().find(|b| !b.is_ascii_whitespace());
    matches!(first, Some(b'{') | Some(b'['))
}

fn ensure_whisper_runtime_ready(app: &AppHandle) -> Result<PathBuf, String> {
    let python_path = runtime::python::get_python_path(app);
    if !python_path.exists() {
        return Err("找不到 Python 运行时，无法使用 AI 听写".to_string());
    }

    if !runtime::capability::python_module_is_available(&python_path, "faster_whisper", 6)
        .unwrap_or(false)
    {
        install_python_packages_with_fallbacks(
            app,
            &python_path,
            &["faster-whisper"],
            Instant::now() + PYTHON_PACKAGES_TIMEOUT,
        )?;
    }

    bootstrap_install_whisper_model(app)?;

    let model_dir = resolve_whisper_base_model_dir(app)?;
    if whisper_model_is_usable(&python_path, &model_dir, 8).unwrap_or(false) {
        Ok(model_dir)
    } else {
        Err("Whisper base 模型文件存在但不可用（常见原因是 tokenizer/config 损坏），请重新执行一键安装运行环境。".to_string())
    }
}

fn is_whitespace_or_punct(text: &str) -> bool {
    let cleaned = text.trim();
    cleaned.is_empty()
        || cleaned
            .chars()
            .all(|ch| ch.is_ascii_punctuation() || ch.is_whitespace())
}

fn seconds_to_ms(value: f64) -> u64 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    (value * 1000.0).round() as u64
}

fn build_document_from_whisper_segments(
    song_id: &str,
    source: &str,
    alignment_engine: &str,
    language: Option<String>,
    segments: Vec<WhisperSegmentResult>,
) -> Option<LyricDocument> {
    let now_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let mut doc_lines = Vec::new();

    for (line_index, segment) in segments.into_iter().enumerate() {
        let line_text = segment.text.trim().to_string();
        if line_text.is_empty() {
            continue;
        }

        let mut tokens = Vec::new();
        let mut token_index = 0u32;
        let mut line_start_ms = seconds_to_ms(segment.start);
        let mut line_end_ms = seconds_to_ms(segment.end).max(line_start_ms.saturating_add(300));
        let mut confidence_sum = 0.0f64;
        let mut confidence_count = 0u32;

        if let Some(words) = segment.words {
            for word in words {
                let word_text = word.word.trim().to_string();
                if word_text.is_empty() || is_whitespace_or_punct(&word_text) {
                    continue;
                }
                let start_ms = word.start.map(seconds_to_ms).unwrap_or(line_start_ms);
                let end_ms = word
                    .end
                    .map(seconds_to_ms)
                    .unwrap_or(start_ms.saturating_add(180));
                line_start_ms = line_start_ms.min(start_ms);
                line_end_ms = line_end_ms.max(end_ms);
                confidence_sum += word.probability.unwrap_or(0.75);
                confidence_count += 1;
                tokens.push(LyricToken {
                    id: format!("line_{}_token_{}", line_index, token_index),
                    line_id: format!("line_{}", line_index),
                    index: token_index,
                    text: word_text,
                    start_ms,
                    end_ms: end_ms.max(start_ms.saturating_add(120)),
                    confidence: word.probability.unwrap_or(0.75) as f32,
                    kind: "word".to_string(),
                });
                token_index += 1;
            }
        }

        if tokens.is_empty() {
            let end_ms = line_end_ms.max(line_start_ms.saturating_add(300));
            tokens.push(LyricToken {
                id: format!("line_{}_token_0", line_index),
                line_id: format!("line_{}", line_index),
                index: 0,
                text: line_text.clone(),
                start_ms: line_start_ms,
                end_ms,
                confidence: 0.7,
                kind: "word".to_string(),
            });
            line_end_ms = end_ms;
            confidence_sum = 0.7;
            confidence_count = 1;
        }

        doc_lines.push(LyricLineDoc {
            id: format!("line_{}", line_index),
            index: line_index as u32,
            start_ms: line_start_ms,
            end_ms: line_end_ms,
            text: line_text,
            confidence: (confidence_sum / confidence_count.max(1) as f64) as f32,
            edited: false,
            locked: false,
            tokens,
        });
    }

    if doc_lines.is_empty() {
        return None;
    }

    Some(LyricDocument {
        song_id: song_id.to_string(),
        version: 1,
        language,
        source: source.to_string(),
        alignment_engine: alignment_engine.to_string(),
        created_at: now_ts,
        updated_at: now_ts,
        global_offset_ms: 0,
        dirty: false,
        quality_score: 0.84,
        lines: doc_lines,
    })
}

fn preview_document(document: &LyricDocument, limit: usize) -> String {
    document
        .lines
        .iter()
        .take(limit)
        .map(|line| line.text.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<&str>>()
        .join("\n")
}

fn score_lrclib_track(
    query_track: &str,
    query_artist: Option<&str>,
    query_duration_ms: u64,
    track: &LrclibTrack,
) -> i32 {
    let mut score = 0;
    let candidate_track = track.track_name.as_deref().unwrap_or_default();
    let candidate_artist = track.artist_name.as_deref().unwrap_or_default();
    score += score_text_relevance(query_track, query_artist, candidate_track, candidate_artist);

    if track
        .synced_lyrics
        .as_deref()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
    {
        score += 25;
    } else if track
        .plain_lyrics
        .as_deref()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
    {
        score += 10;
    }

    if track.instrumental.unwrap_or(false) {
        score -= 120;
    }

    if query_duration_ms > 0 {
        if let Some(candidate_duration) = track.duration {
            let query_duration = query_duration_ms as f64 / 1000.0;
            let diff = (candidate_duration - query_duration).abs();
            if diff < 2.0 {
                score += 30;
            } else if diff < 5.0 {
                score += 15;
            } else if diff < 12.0 {
                score += 5;
            } else {
                score -= (diff * 4.0) as i32;
            }
        }
    }

    score
}

fn fetch_lrclib_candidates(
    song_id: &str,
    query_track: &str,
    query_artist: Option<&str>,
    query_duration_ms: u64,
) -> Result<Vec<LyricsCandidate>, String> {
    let cache_key = lyrics_search_cache_key(
        "lrclib",
        song_id,
        query_track,
        query_artist,
        query_duration_ms,
    );
    if let Some(cached) = get_cached_lyrics_candidates(&cache_key) {
        return Ok(cached);
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .user_agent("Macaron Singer/1.0 (+https://github.com/suntong/4isfstools)")
        .build()
        .map_err(|e| format!("Failed to build lrclib client: {}", e))?;

    let duration_seconds = if query_duration_ms > 0 {
        Some(format!("{:.3}", query_duration_ms as f64 / 1000.0))
    } else {
        None
    };

    let mut candidates: Vec<(i32, LrclibTrack)> = Vec::new();
    let mut search_queries = vec![query_track.to_string()];
    for token in extract_fallback_keywords(query_track) {
        if !search_queries
            .iter()
            .any(|q| normalize_match_text(q) == normalize_match_text(&token))
        {
            search_queries.push(token);
        }
    }
    search_queries.truncate(3);

    let mut get_request = client
        .get("https://lrclib.net/api/get")
        .query(&[("track_name", query_track)]);
    if let Some(query_artist) = query_artist {
        get_request = get_request.query(&[("artist_name", query_artist)]);
    }
    if let Some(duration_seconds) = duration_seconds.as_deref() {
        get_request = get_request.query(&[("duration", duration_seconds)]);
    }

    if let Ok(response) = get_request.send() {
        if response.status().is_success() {
            if let Ok(track) = response.json::<LrclibTrack>() {
                let score =
                    score_lrclib_track(query_track, query_artist, query_duration_ms, &track);
                candidates.push((score, track));
            }
        }
    }

    for query in search_queries {
        for query_param in ["q", "query"] {
            let search_request = client
                .get("https://lrclib.net/api/search")
                .query(&[(query_param, query.as_str())]);
            // For keyword search, keep the request broad and avoid over-filtering.
            let _ = query_artist;
            let _ = duration_seconds;

            let response = match search_request.send() {
                Ok(response) if response.status().is_success() => response,
                _ => continue,
            };

            let tracks = match response.json::<Vec<LrclibTrack>>() {
                Ok(tracks) => tracks,
                Err(e) => return Err(format!("Failed to parse lrclib response: {}", e)),
            };

            for track in tracks {
                let score =
                    score_lrclib_track(query_track, query_artist, query_duration_ms, &track);
                candidates.push((score, track));
            }
            if candidates.len() >= 12 {
                break;
            }
        }
        if candidates.len() >= 12 {
            break;
        }
    }

    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for (score, track) in candidates {
        let key = format!(
            "{}::{}::{}",
            normalize_match_text(track.artist_name.as_deref().unwrap_or_default()),
            normalize_match_text(track.track_name.as_deref().unwrap_or_default()),
            (track.duration.unwrap_or_default() * 10.0).round() as i64
        );
        if !seen.insert(key) {
            continue;
        }
        let title = track
            .track_name
            .clone()
            .or(track.name.clone())
            .unwrap_or_else(|| query_track.to_string());
        let artist = track.artist_name.clone();
        let album = track.album_name.clone();
        if let Some(document) = build_lrclib_document(song_id, &track, true) {
            result.push(LyricsCandidate {
                id: track
                    .id
                    .map(|id| format!("lrclib::{}", id))
                    .unwrap_or_else(|| format!("lrclib::{}::{}", song_id, result.len())),
                source: "lrclib".to_string(),
                source_label: "LRCLib".to_string(),
                title,
                artist,
                album,
                score,
                synced: track
                    .synced_lyrics
                    .as_deref()
                    .map(|v| !v.trim().is_empty())
                    .unwrap_or(false),
                preview: preview_document(&document, 3),
                document,
            });
        }
    }

    result.sort_by(|a, b| b.score.cmp(&a.score));
    if result.is_empty() {
        // Try broader plain-lyrics fallback before giving up.
        let mut broader = Vec::new();
        for query in extract_fallback_keywords(query_track).into_iter().take(3) {
            for query_param in ["q", "query"] {
                let search_request = client
                    .get("https://lrclib.net/api/search")
                    .query(&[(query_param, query.as_str())]);
                let _ = query_artist;
                let _ = duration_seconds;
                let response = match search_request.send() {
                    Ok(response) if response.status().is_success() => response,
                    _ => continue,
                };
                let tracks = match response.json::<Vec<LrclibTrack>>() {
                    Ok(tracks) => tracks,
                    Err(_) => continue,
                };
                for track in tracks {
                    let score =
                        score_lrclib_track(query_track, query_artist, query_duration_ms, &track);
                    broader.push((score, track));
                }
                if broader.len() >= 20 {
                    break;
                }
            }
            if broader.len() >= 20 {
                break;
            }
        }

        broader.sort_by(|a, b| b.0.cmp(&a.0));
        let mut seen = std::collections::HashSet::new();
        for (score, track) in broader {
            let key = format!(
                "{}::{}::{}",
                normalize_match_text(track.artist_name.as_deref().unwrap_or_default()),
                normalize_match_text(track.track_name.as_deref().unwrap_or_default()),
                (track.duration.unwrap_or_default() * 10.0).round() as i64
            );
            if !seen.insert(key) {
                continue;
            }
            let title = track
                .track_name
                .clone()
                .or(track.name.clone())
                .unwrap_or_else(|| query_track.to_string());
            let artist = track.artist_name.clone();
            let album = track.album_name.clone();
            if let Some(document) = build_lrclib_document(song_id, &track, true) {
                result.push(LyricsCandidate {
                    id: track
                        .id
                        .map(|id| format!("lrclib::{}", id))
                        .unwrap_or_else(|| format!("lrclib::{}::{}", song_id, result.len())),
                    source: "lrclib".to_string(),
                    source_label: "LRCLib".to_string(),
                    title,
                    artist,
                    album,
                    score,
                    synced: track
                        .synced_lyrics
                        .as_deref()
                        .map(|v| !v.trim().is_empty())
                        .unwrap_or(false),
                    preview: preview_document(&document, 3),
                    document,
                });
            }
        }
        result.sort_by(|a, b| b.score.cmp(&a.score));
    }
    set_cached_lyrics_candidates(cache_key, result.clone());
    Ok(result)
}

fn fetch_lrclib_candidates_manual(
    song_id: &str,
    raw_query: &str,
) -> Result<Vec<LyricsCandidate>, String> {
    let query = clean_lyrics_search_hint(raw_query);
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let cache_key = lyrics_search_cache_key("lrclib_manual", song_id, &query, None, 0);
    if let Some(cached) = get_cached_lyrics_candidates(&cache_key) {
        return Ok(cached);
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(4))
        .user_agent("Macaron Singer/1.0 (+https://github.com/suntong/4isfstools)")
        .build()
        .map_err(|e| format!("Failed to build lrclib client: {}", e))?;

    let (query_artist, query_track) = split_artist_track_hint(&query);
    let mut tracks: Vec<LrclibTrack> = Vec::new();
    let mut exact_request = client
        .get("https://lrclib.net/api/get")
        .query(&[("track_name", query_track.as_str())]);
    if let Some(query_artist) = query_artist.as_deref() {
        if !query_artist.trim().is_empty() {
            exact_request = exact_request.query(&[("artist_name", query_artist)]);
        }
    }
    if let Ok(response) = exact_request.send() {
        if response.status().is_success() {
            if let Ok(track) = response.json::<LrclibTrack>() {
                tracks.push(track);
            }
        }
    }

    for query_param in ["q", "query"] {
        let response = match client
            .get("https://lrclib.net/api/search")
            .query(&[(query_param, query.as_str())])
            .send()
        {
            Ok(response) if response.status().is_success() => response,
            _ => continue,
        };
        match response.json::<Vec<LrclibTrack>>() {
            Ok(mut parsed) => tracks.append(&mut parsed),
            Err(_) => continue,
        }
        if !tracks.is_empty() {
            break;
        }
    }

    if tracks.is_empty() {
        return Ok(Vec::new());
    }

    let mut scored: Vec<(i32, LrclibTrack)> = tracks
        .into_iter()
        .map(|track| {
            let score = score_lrclib_track(&query_track, query_artist.as_deref(), 0, &track);
            (score, track)
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));

    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for (score, track) in scored {
        let key = format!(
            "{}::{}::{}",
            normalize_match_text(track.artist_name.as_deref().unwrap_or_default()),
            normalize_match_text(track.track_name.as_deref().unwrap_or_default()),
            (track.duration.unwrap_or_default() * 10.0).round() as i64
        );
        if !seen.insert(key) {
            continue;
        }
        let title = track
            .track_name
            .clone()
            .or(track.name.clone())
            .unwrap_or_else(|| query_track.to_string());
        let artist = track.artist_name.clone();
        let album = track.album_name.clone();
        if let Some(document) = build_lrclib_document(song_id, &track, true) {
            result.push(LyricsCandidate {
                id: track
                    .id
                    .map(|id| format!("lrclib::{}", id))
                    .unwrap_or_else(|| format!("lrclib::{}::{}", song_id, result.len())),
                source: "lrclib".to_string(),
                source_label: "LRCLib".to_string(),
                title,
                artist,
                album,
                score,
                synced: track
                    .synced_lyrics
                    .as_deref()
                    .map(|v| !v.trim().is_empty())
                    .unwrap_or(false),
                preview: preview_document(&document, 3),
                document,
            });
        }
        if result.len() >= 12 {
            break;
        }
    }

    set_cached_lyrics_candidates(cache_key, result.clone());
    Ok(result)
}

#[tauri::command]
async fn import_songs(_app: AppHandle, paths: Vec<String>) -> Result<Vec<Song>, String> {
    let mut songs = SONGS.lock().unwrap();
    if songs.is_none() {
        *songs = Some(HashMap::new());
    }

    let mut new_songs = Vec::new();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let songs_dir = get_songs_dir();
    ensure_dir(&songs_dir).map_err(|e| e.to_string())?;

    for (i, path) in paths.iter().enumerate() {
        let source_path = Path::new(path);
        let song_id = format!("song_{}_{}", timestamp, i);
        let filename = source_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string();

        let song_dir = songs_dir.join(&song_id);
        ensure_dir(&song_dir).map_err(|e| e.to_string())?;
        let stored_original_path = if is_video_import_path(source_path) {
            let extracted_audio_path = song_dir.join("original.wav");
            extract_audio_from_video(source_path, &extracted_audio_path)?;
            extracted_audio_path.to_string_lossy().to_string()
        } else {
            path.clone()
        };

        let song = Song {
            id: song_id.clone(),
            name: filename,
            original_path: stored_original_path,
            playlist_folder: None,
            vocals_path: None,
            instrumental_path: None,
            original_mix_path: None,
            lyrics_path: None,
            duration: 0,
            status: "pending".to_string(),
            progress: 0,
            processing_stage: None,
            error_message: None,
            added_at: timestamp,
        };
        new_songs.push(song.clone());
        songs.as_mut().unwrap().insert(song_id, song);
    }

    drop(songs);
    save_songs_to_disk();

    Ok(new_songs)
}

#[tauri::command]
async fn start_process(
    app: AppHandle,
    song_id: String,
    _prefer_onnx_provider: bool,
) -> Result<(), String> {
    let song = {
        let songs = SONGS.lock().unwrap();
        songs.as_ref().and_then(|m| m.get(&song_id).cloned())
    };

    let song = match song {
        Some(s) => s,
        None => return Err("Song not found".to_string()),
    };

    if song.status != "pending" && song.status != "error" && song.status != "cancelled" {
        return Err(format!("Cannot process song with status: {}", song.status));
    }

    if separation_queue::is_queued(&song_id) {
        return Err("Song is already queued for processing".to_string());
    }

    let job_token = JobManager::prepare_song_job(&song_id);
    update_song_status(&song_id, "queued", 0, Some("queued"), None);

    let songs_dir = get_songs_dir();
    ensure_dir(&songs_dir).map_err(|e| e.to_string())?;

    let song_dir = songs_dir.join(&song_id);
    ensure_dir(&song_dir).map_err(|e| e.to_string())?;

    let input_path = song.original_path.clone();
    let song_duration_ms = song.duration;

    separation_queue::submit_task(separation_queue::SeparationTask {
        app,
        song_id,
        job_token,
        input_path,
        output_dir: song_dir,
        song_duration_ms,
    });

    Ok(())
}

fn process_song_with_onnx_skeleton(
    app: AppHandle,
    song_id: String,
    job_token: String,
    input_path: String,
    output_dir: PathBuf,
) {
    if check_cancel_flag(&song_id) {
        return;
    }

    emit_progress_for_job(
        &app,
        &song_id,
        &job_token,
        "checking_onnx",
        5,
        "正在检查 ONNX 分离引擎...",
        Some(120),
    );
    update_song_status_for_job(
        &song_id,
        &job_token,
        "processing",
        5,
        Some("checking_onnx"),
        None,
    );

    let mut engine_health = separation::detect_engine_health(&app, &get_models_dir(&app));
    let python_path = runtime::python::get_python_path(&app);
    if python_path.exists() {
        engine_health.onnxruntime_available =
            runtime::capability::python_module_is_available(&python_path, "onnxruntime", 6)
                .unwrap_or(false);
    }

    let debug_dir = output_dir.join("debug");
    let _ = fs::create_dir_all(&debug_dir);
    let result_file = output_dir.join("separator_result.json");
    let debug_result_file = debug_dir.join("separator_result.json");
    let command_file = debug_dir.join("command.json");
    let debug_log = debug_dir.join("separator_debug.log");
    let progress_file = output_dir.join("separator_progress.json");

    let message = if !engine_health.onnxruntime_available {
        "ONNX Runtime 不可用"
    } else if !engine_health.default_model_ready {
        "ONNX 默认模型 UVR_MDXNET_9482.onnx 尚未就绪"
    } else if !engine_health.default_model_session_load_ok {
        "ONNX Session 加载失败"
    } else if !engine_health.default_model_metadata_ok {
        "ONNX 模型元数据读取失败"
    } else {
        "ONNX 探针已完成，真实分离执行器尚未接入。"
    };

    let requested_providers = engine_health.requested_providers.clone();
    let payload = serde_json::json!({
        "success": false,
        "error": message,
        "error_code": if !engine_health.onnxruntime_available {
            "ONNX_RUNTIME_UNAVAILABLE"
        } else if !engine_health.default_model_ready {
            "ONNX_ENGINE_NOT_READY"
        } else if !engine_health.default_model_session_load_ok {
            "ONNX_SESSION_LOAD_FAILED"
        } else if !engine_health.default_model_metadata_ok {
            "ONNX_MODEL_METADATA_FAILED"
        } else {
            "ONNX_ENGINE_PROBE_ONLY"
        },
        "stage": "checking_onnx",
        "engine": engine_health.active_engine,
        "requested_providers": requested_providers,
        "selected_provider": engine_health.selected_provider,
        "provider_fallback_reason": engine_health.provider_fallback_reason,
        "onnxruntime_available": engine_health.onnxruntime_available,
        "default_model_id": engine_health.default_model_id,
        "default_model_path": engine_health.default_model_path,
        "default_model_ready": engine_health.default_model_ready,
        "default_model_session_load_ok": engine_health.default_model_session_load_ok,
        "default_model_session_load_error": engine_health.default_model_session_load_error,
        "default_model_metadata_ok": engine_health.default_model_metadata_ok,
        "default_model_metadata_error": engine_health.default_model_metadata_error,
        "default_model_input_shape": engine_health.default_model_input_shape,
        "default_model_output_shape": engine_health.default_model_output_shape,
        "default_model_dummy_inference_ok": engine_health.default_model_dummy_inference_ok,
        "default_model_dummy_inference_error": engine_health.default_model_dummy_inference_error,
        "high_quality_model_id": engine_health.high_quality_model_id,
        "high_quality_model_path": engine_health.high_quality_model_path,
        "high_quality_model_ready": engine_health.high_quality_model_ready,
        "high_quality_model_session_load_ok": engine_health.high_quality_model_session_load_ok,
        "high_quality_model_session_load_error": engine_health.high_quality_model_session_load_error,
        "high_quality_model_metadata_ok": engine_health.high_quality_model_metadata_ok,
        "high_quality_model_metadata_error": engine_health.high_quality_model_metadata_error,
        "high_quality_model_input_shape": engine_health.high_quality_model_input_shape,
        "high_quality_model_output_shape": engine_health.high_quality_model_output_shape,
        "high_quality_model_dummy_inference_ok": engine_health.high_quality_model_dummy_inference_ok,
        "high_quality_model_dummy_inference_error": engine_health.high_quality_model_dummy_inference_error,
        "input_path": input_path,
        "output_dir": output_dir.to_string_lossy(),
        "debug_log_path": debug_log.to_string_lossy(),
        "command_file_path": command_file.to_string_lossy(),
    });

    let _ = fs::write(
        &progress_file,
        serde_json::json!({
            "percent": 5,
            "message": message,
        })
        .to_string(),
    );
    append_text_log(
        &debug_log,
        &format_log_block(
            "onnx separation skeleton",
            &[
                ("song_id", song_id.clone()),
                ("job_token", job_token.clone()),
                ("input_path", input_path),
                ("output_dir", output_dir.to_string_lossy().to_string()),
                ("requested_providers", requested_providers.join(",")),
                (
                    "onnxruntime_available",
                    engine_health.onnxruntime_available.to_string(),
                ),
                (
                    "default_model_ready",
                    engine_health.default_model_ready.to_string(),
                ),
                (
                    "default_model_session_load_ok",
                    engine_health.default_model_session_load_ok.to_string(),
                ),
                (
                    "default_model_metadata_ok",
                    engine_health.default_model_metadata_ok.to_string(),
                ),
            ],
        ),
    );
    let _ = fs::write(
        &command_file,
        serde_json::json!({
            "engine": "onnx",
            "status": "phase_onnx_a_skeleton",
            "onnx_mainline": true,
        })
        .to_string(),
    );
    let payload_text = payload.to_string();
    let _ = fs::write(&result_file, &payload_text);
    let _ = fs::write(&debug_result_file, payload_text);

    emit_error_for_job(&app, &song_id, &job_token, "checking_onnx", message);
    update_song_status_for_job(
        &song_id,
        &job_token,
        "error",
        0,
        Some("checking_onnx"),
        Some(message),
    );
}

pub(crate) fn process_song_background(
    app: AppHandle,
    song_id: String,
    job_token: String,
    input_path: String,
    output_dir: PathBuf,
    _song_duration_ms: u64,
    _prefer_onnx_provider: bool,
) {
    process_song_with_onnx_skeleton(app, song_id, job_token, input_path, output_dir);
}

#[tauri::command]
async fn get_songs() -> Result<Vec<Song>, String> {
    let songs = SONGS.lock().unwrap();
    Ok(songs
        .as_ref()
        .map(|m| m.values().cloned().collect())
        .unwrap_or_default())
}

#[tauri::command]
async fn get_song(song_id: String) -> Result<Option<Song>, String> {
    let songs = SONGS.lock().unwrap();
    Ok(songs.as_ref().and_then(|m| m.get(&song_id).cloned()))
}

#[tauri::command]
async fn delete_song(id: String) -> Result<(), String> {
    let song_to_delete = {
        let songs = SONGS.lock().unwrap();
        songs.as_ref().and_then(|m| m.get(&id).cloned())
    };

    if let Some(song) = song_to_delete.as_ref() {
        if song.status == "queued" || song.status == "processing" || song.status == "cancelling" {
            return Err("Cannot delete a song that is queued or being processed".to_string());
        }
    }

    {
        let mut songs = SONGS.lock().unwrap();
        if let Some(ref mut map) = *songs {
            map.remove(&id);
        }
    }

    if let Some(song) = song_to_delete.as_ref() {
        cleanup_song_artifacts(song);
    } else {
        let songs_dir = get_songs_dir();
        let song_dir = songs_dir.join(&id);
        if song_dir.exists() {
            fs::remove_dir_all(&song_dir).map_err(|e| e.to_string())?;
        }
    }

    save_songs_to_disk();
    Ok(())
}

#[tauri::command]
async fn reprocess_song(
    app: AppHandle,
    song_id: String,
    prefer_onnx_provider: bool,
) -> Result<(), String> {
    let song = {
        let songs = SONGS.lock().unwrap();
        songs.as_ref().and_then(|m| m.get(&song_id).cloned())
    };

    let song = match song {
        Some(s) => s,
        None => return Err("Song not found".to_string()),
    };

    if song.status != "ready" {
        return Err(format!(
            "Cannot reprocess song with status: {}. Only 'ready' songs can be reprocessed.",
            song.status
        ));
    }

    if separation_queue::is_queued(&song_id) {
        return Err("Song is already queued for processing".to_string());
    }

    // Clear output paths for reprocess
    {
        let mut songs = SONGS.lock().unwrap();
        if let Some(ref mut map) = *songs {
            if let Some(s) = map.get_mut(&song_id) {
                s.vocals_path = None;
                s.instrumental_path = None;
                s.original_mix_path = None;
                s.lyrics_path = None;
                s.error_message = None;
            }
        }
    }

    let job_token = JobManager::prepare_song_job(&song_id);
    update_song_status(&song_id, "queued", 0, Some("queued"), None);

    let songs_dir = get_songs_dir();
    let song_dir = songs_dir.join(&song_id);
    ensure_dir(&song_dir).map_err(|e| e.to_string())?;

    let input_path = song.original_path.clone();
    let song_duration_ms = song.duration;

    separation_queue::submit_task(separation_queue::SeparationTask {
        app,
        song_id,
        job_token,
        input_path,
        output_dir: song_dir,
        song_duration_ms,
    });

    Ok(())
}

#[tauri::command]
async fn ensure_original_mix(song_id: String) -> Result<String, String> {
    let song = {
        let songs = SONGS.lock().unwrap();
        songs.as_ref().and_then(|m| m.get(&song_id).cloned())
    };

    let song = match song {
        Some(s) => s,
        None => return Err("Song not found".to_string()),
    };

    if let Some(existing) = song.original_mix_path.clone() {
        if PathBuf::from(&existing).exists() {
            return Ok(existing);
        }
    }

    let vocals_path = song
        .vocals_path
        .as_ref()
        .ok_or_else(|| "Vocals path not available".to_string())?
        .clone();
    let instrumental_path = song
        .instrumental_path
        .as_ref()
        .ok_or_else(|| "Instrumental path not available".to_string())?
        .clone();

    let mix_path = build_original_mix(&vocals_path, &instrumental_path)?;

    {
        let mut songs = SONGS.lock().unwrap();
        if let Some(ref mut map) = *songs {
            if let Some(s) = map.get_mut(&song_id) {
                s.original_mix_path = Some(mix_path.clone());
            }
        }
    }
    save_songs_to_disk();

    Ok(mix_path)
}

#[tauri::command]
async fn cancel_process(app: AppHandle, song_id: String) -> Result<(), String> {
    let status = {
        let songs = SONGS.lock().unwrap();
        songs
            .as_ref()
            .and_then(|m| m.get(&song_id).map(|s| s.status.clone()))
    };

    match status {
        Some(s) if s == "queued" => {
            // Task is in queue, not yet started - remove from queue and clean up
            if separation_queue::cancel_task(&song_id) {
                JobManager::clear_song_job(&song_id, "用户取消");
                update_song_status(
                    &song_id,
                    "cancelled",
                    0,
                    Some("cancelled"),
                    Some("用户取消"),
                );
                emit_progress(&app, &song_id, "cancelled", 0, "已取消", None);
                Ok(())
            } else if get_active_job_token(&song_id).is_some() {
                JobManager::clear_song_job(&song_id, "用户取消");
                set_cancel_flag(&song_id);
                update_song_status(
                    &song_id,
                    "cancelled",
                    0,
                    Some("cancelled"),
                    Some("用户取消"),
                );
                emit_progress(&app, &song_id, "cancelled", 0, "已取消", None);
                Ok(())
            } else {
                Err("Task not found in queue".to_string())
            }
        }
        Some(s) if s == "processing" || s == "cancelling" => {
            let cancel_job_token = get_active_job_token(&song_id);
            JobManager::clear_song_job(&song_id, "用户取消");
            update_song_status(
                &song_id,
                "cancelling",
                0,
                Some("cancelling"),
                Some("正在取消..."),
            );
            emit_progress(&app, &song_id, "cancelling", 0, "正在取消...", None);
            set_cancel_flag(&song_id);

            if let Some(job) = get_job(&song_id) {
                terminate_known_job(&job, false);
            }
            terminate_song_processes(&song_id, false);

            update_song_status(
                &song_id,
                "cancelled",
                0,
                Some("cancelled"),
                Some("用户取消"),
            );
            emit_progress(&app, &song_id, "cancelled", 0, "已取消", None);

            let app_clone = app.clone();
            let song_id_clone = song_id.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(250));

                if cancel_job_token.as_deref() != get_active_job_token(&song_id_clone).as_deref() {
                    return;
                }

                if let Some(job) = get_job(&song_id_clone) {
                    terminate_known_job(&job, true);
                }
                terminate_song_processes(&song_id_clone, true);

                update_song_status(&song_id_clone, "cancelled", 0, None, Some("用户取消"));
                emit_progress(&app_clone, &song_id_clone, "cancelled", 0, "已取消", None);
            });
            Ok(())
        }
        Some(s) => Err(format!("Cannot cancel song with status: {}", s)),
        None => Err("Song not found".to_string()),
    }
}

#[tauri::command]
async fn update_song_duration(song_id: String, duration: u64) -> Result<(), String> {
    let mut songs = SONGS.lock().unwrap();
    if let Some(ref mut map) = *songs {
        if let Some(song) = map.get_mut(&song_id) {
            song.duration = duration;
        }
    }
    drop(songs);
    save_songs_to_disk();
    Ok(())
}

#[tauri::command]
async fn rename_song(song_id: String, new_name: String) -> Result<(), String> {
    let trimmed = new_name.trim();
    if trimmed.is_empty() {
        return Err("Song name cannot be empty".to_string());
    }

    let mut songs = SONGS.lock().unwrap();
    if let Some(ref mut map) = *songs {
        if let Some(song) = map.get_mut(&song_id) {
            song.name = trimmed.to_string();
        } else {
            return Err("Song not found".to_string());
        }
    }
    drop(songs);
    save_songs_to_disk();
    Ok(())
}

#[tauri::command]
async fn set_song_folder(song_id: String, folder_name: Option<String>) -> Result<(), String> {
    let normalized = normalize_folder_name(folder_name);

    let mut songs = SONGS.lock().unwrap();
    if let Some(ref mut map) = *songs {
        if let Some(song) = map.get_mut(&song_id) {
            song.playlist_folder = normalized;
        } else {
            return Err("Song not found".to_string());
        }
    }
    drop(songs);
    save_songs_to_disk();
    Ok(())
}

#[tauri::command]
async fn rename_playlist_folder(old_name: String, new_name: String) -> Result<(), String> {
    let old_trimmed = old_name.trim();
    let new_trimmed = new_name.trim();
    if old_trimmed.is_empty() || new_trimmed.is_empty() {
        return Err("Folder name cannot be empty".to_string());
    }

    let mut songs = SONGS.lock().unwrap();
    if let Some(ref mut map) = *songs {
        for song in map.values_mut() {
            if song.playlist_folder.as_deref() == Some(old_trimmed) {
                song.playlist_folder = Some(new_trimmed.to_string());
            }
        }
    }
    drop(songs);
    save_songs_to_disk();
    Ok(())
}

#[tauri::command]
async fn get_file_storage_settings() -> Result<FileStorageSettings, String> {
    Ok(get_file_storage_settings_snapshot())
}

#[tauri::command]
async fn get_runtime_health(app: AppHandle) -> Result<RuntimeHealthReport, String> {
    Ok(detect_runtime_health(&app))
}

#[tauri::command]
async fn get_bootstrap_status(app: AppHandle) -> Result<BootstrapStatus, String> {
    Ok(detect_bootstrap_status(&app))
}

#[tauri::command]
async fn bootstrap_install_minimal(
    app: AppHandle,
    prefer_onnx_provider: bool,
) -> Result<BootstrapStatus, String> {
    let _preferred_provider_requested = prefer_onnx_provider;
    let deadline = Instant::now() + BOOTSTRAP_TOTAL_TIMEOUT;
    emit_bootstrap_progress(&app, "python_runtime", 8, "正在检查 Python 运行时...");
    bootstrap_install_python_runtime(&app).map_err(|e| format!("Python 运行时安装失败：{}", e))?;
    emit_bootstrap_progress(&app, "ffmpeg_runtime", 24, "正在检查 FFmpeg...");
    ensure_ffmpeg_runtime().map_err(|e| format!("FFmpeg 安装失败：{}", e))?;
    emit_bootstrap_progress(
        &app,
        "python_modules",
        32,
        "正在确认/安装 ONNX Runtime 分离路线的运行依赖...",
    );
    ensure_onnx_runtime_modules(&app, deadline)
        .map_err(|e| format!("运行依赖安装失败（ONNX 路线）：{}", e))?;
    emit_bootstrap_progress(&app, "models", 74, "正在检查 ONNX 模型...");
    bootstrap_install_models(&app)
        .map_err(|e| format!("模型安装失败（ONNX/whisper base）：{}", e))?;
    emit_bootstrap_progress(&app, "verify", 92, "正在做最终环境验证...");
    let status = detect_bootstrap_status(&app);
    if status.can_run_core {
        emit_bootstrap_progress(&app, "complete", 100, "安装完成，可运行。");
        Ok(status)
    } else {
        let health = detect_runtime_health(&app);
        let missing = format_missing_core_components_with_reason(&health);
        Err(format!(
            "安装未完成：{} 仍未就绪。请按缺失项补齐后重试。",
            missing
        ))
    }
}

#[tauri::command]
async fn update_file_storage_settings(
    settings: FileStorageSettings,
) -> Result<FileStorageSettings, String> {
    let normalized = normalize_file_storage_settings(settings);
    set_file_storage_settings(normalized.clone());
    migrate_library_assets();
    Ok(get_file_storage_settings_snapshot())
}

#[tauri::command]
async fn search_match_lyrics(
    song_id: String,
    query: Option<String>,
) -> Result<Vec<LyricsCandidate>, String> {
    let song = {
        let songs = SONGS.lock().unwrap();
        songs.as_ref().and_then(|m| m.get(&song_id).cloned())
    };

    let song = match song {
        Some(s) => s,
        None => return Err("Song not found".to_string()),
    };

    let (tx, rx) = mpsc::channel();
    let song_id_for_worker = song_id.clone();
    let song_for_worker = song.clone();
    let query_for_worker = query.clone();
    std::thread::spawn(move || {
        let song_duration = song_for_worker.duration;
        let mut candidates = Vec::new();
        let mut errors = Vec::new();
        let search_intent =
            build_lyrics_search_intent(&song_for_worker, query_for_worker.as_deref());
        let intent_track = search_intent.query_track.clone();
        let intent_artist = search_intent.query_artist.clone();
        let intent_variants = search_intent.variants.clone();
        let allow_weak_fallback = search_intent.allow_weak_fallback;

        if query_for_worker
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
        {
            let mut handles = Vec::new();

            {
                let song_id = song_id_for_worker.clone();
                let raw_query = intent_track.clone();
                let query_artist = intent_artist.clone();
                handles.push((
                    "LRCLib",
                    std::thread::spawn(move || {
                        let manual_query = if let Some(artist) = query_artist.as_deref() {
                            if artist.is_empty() {
                                raw_query.clone()
                            } else {
                                format!("{} - {}", artist, raw_query)
                            }
                        } else {
                            raw_query.clone()
                        };
                        fetch_lrclib_candidates_manual(&song_id, &manual_query)
                    }),
                ));
            }

            {
                let song_id = song_id_for_worker.clone();
                let query_track = intent_track.clone();
                let query_artist = intent_artist.clone();
                handles.push((
                    "163MusicLyrics",
                    std::thread::spawn(move || {
                        fetch_netease_candidates(
                            &song_id,
                            &query_track,
                            query_artist.as_deref(),
                            song_duration,
                        )
                    }),
                ));
            }

            {
                let song_id = song_id_for_worker.clone();
                let query_track = intent_track.clone();
                let query_artist = intent_artist.clone();
                handles.push((
                    "QQMusic",
                    std::thread::spawn(move || {
                        fetch_qq_candidates(
                            &song_id,
                            &query_track,
                            query_artist.as_deref(),
                            song_duration,
                        )
                    }),
                ));
            }

            for (label, handle) in handles {
                match handle.join() {
                    Ok(Ok(mut items)) => candidates.append(&mut items),
                    Ok(Err(err)) => errors.push(format!("{}: {}", label, err)),
                    Err(_) => errors.push(format!("{} candidate search panicked", label)),
                }
            }
        } else {
            let variants = if intent_variants.is_empty() {
                candidate_query_variants(&intent_track, &intent_track)
            } else {
                intent_variants.clone()
            };
            let mut lrclib_handles = Vec::new();
            for (query_artist, query_track) in variants.iter().take(3) {
                let song_id_lrclib = song_id_for_worker.clone();
                let query_track_lrclib = query_track.clone();
                let query_artist_lrclib = query_artist.clone();
                lrclib_handles.push(std::thread::spawn(move || {
                    fetch_lrclib_candidates(
                        &song_id_lrclib,
                        &query_track_lrclib,
                        query_artist_lrclib.as_deref(),
                        song_duration,
                    )
                }));
            }
            for handle in lrclib_handles {
                match handle.join() {
                    Ok(Ok(mut items)) => candidates.append(&mut items),
                    Ok(Err(err)) => errors.push(format!("LRCLib: {}", err)),
                    Err(_) => errors.push("LRCLib candidate search panicked".to_string()),
                }
            }

            let mut netease_handles = Vec::new();
            for (query_artist, query_track) in variants.iter().take(3) {
                let song_id_netease = song_id_for_worker.clone();
                let query_track_netease = query_track.clone();
                let query_artist_netease = query_artist.clone();
                netease_handles.push(std::thread::spawn(move || {
                    fetch_netease_candidates(
                        &song_id_netease,
                        &query_track_netease,
                        query_artist_netease.as_deref(),
                        song_duration,
                    )
                }));
            }
            for handle in netease_handles {
                match handle.join() {
                    Ok(Ok(mut items)) => candidates.append(&mut items),
                    Ok(Err(err)) => errors.push(format!("163MusicLyrics: {}", err)),
                    Err(_) => errors.push("163MusicLyrics candidate search panicked".to_string()),
                }
            }

            let mut qq_handles = Vec::new();
            for (query_artist, query_track) in variants.iter().take(3) {
                let song_id_qq = song_id_for_worker.clone();
                let query_track_qq = query_track.clone();
                let query_artist_qq = query_artist.clone();
                qq_handles.push(std::thread::spawn(move || {
                    fetch_qq_candidates(
                        &song_id_qq,
                        &query_track_qq,
                        query_artist_qq.as_deref(),
                        song_duration,
                    )
                }));
            }
            for handle in qq_handles {
                match handle.join() {
                    Ok(Ok(mut items)) => candidates.append(&mut items),
                    Ok(Err(err)) => errors.push(format!("163MusicLyrics: {}", err)),
                    Err(_) => errors.push("163MusicLyrics candidate search panicked".to_string()),
                }
            }
        }

        candidates = rank_lyrics_candidates(
            candidates,
            &intent_track,
            intent_artist.as_deref(),
            allow_weak_fallback,
        );
        candidates.truncate(8);

        let outcome = if candidates.is_empty() {
            if errors.is_empty() {
                Err("未找到匹配歌词候选".to_string())
            } else {
                Err(format!("未找到匹配歌词候选：{}", errors.join("；")))
            }
        } else {
            Ok(candidates)
        };

        let _ = tx.send(outcome);
    });

    match rx.recv_timeout(Duration::from_secs(30)) {
        Ok(result) => result,
        Err(_) => Err("搜索服务超时（30秒），请重试或更换关键词".to_string()),
    }
}

#[tauri::command]
async fn get_lyrics_document(song_id: String) -> Result<Option<LyricDocument>, String> {
    let lyrics_json_path = get_lyrics_json_path(&song_id);
    let legacy_lyrics_json = legacy_lyrics_json_path(&song_id);
    let target_path = if lyrics_json_path.exists() {
        Some(lyrics_json_path)
    } else if legacy_lyrics_json.exists() {
        Some(legacy_lyrics_json)
    } else {
        None
    };

    let Some(path) = target_path else {
        return Ok(None);
    };

    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let document = serde_json::from_str::<LyricDocument>(&content).map_err(|e| e.to_string())?;
    Ok(Some(document))
}

#[tauri::command]
async fn save_lyrics_document(song_id: String, document: LyricDocument) -> Result<(), String> {
    persist_lyrics_document(&song_id, &document)?;
    Ok(())
}

fn persist_lyrics_document(song_id: &str, document: &LyricDocument) -> Result<String, String> {
    let settings = get_file_storage_settings_snapshot();
    let lyrics_json_path = resolve_lyrics_json_path(song_id, &settings);
    let lyrics_lrc_path = resolve_lyrics_lrc_path(song_id, &settings);
    if let Some(parent) = lyrics_json_path.parent() {
        ensure_dir(&parent.to_path_buf()).map_err(|e| e.to_string())?;
    }
    let mut updated_document = document.clone();
    updated_document.updated_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let json = serde_json::to_string_pretty(&updated_document).map_err(|e| e.to_string())?;
    fs::write(&lyrics_json_path, json).map_err(|e| e.to_string())?;
    let lrc = lyric_document_to_lrc(&updated_document);
    fs::write(&lyrics_lrc_path, lrc).map_err(|e| e.to_string())?;
    {
        let mut songs = SONGS.lock().unwrap();
        if let Some(ref mut map) = *songs {
            if let Some(song) = map.get_mut(song_id) {
                song.lyrics_path = Some(lyrics_lrc_path.to_string_lossy().to_string());
            }
        }
    }
    save_songs_to_disk();
    Ok(lyrics_lrc_path.to_string_lossy().to_string())
}

#[tauri::command]
async fn generate_whisper_base_lyrics(
    app: AppHandle,
    song_id: String,
) -> Result<GeneratedLyricsDraftResult, String> {
    let song = {
        let songs = SONGS.lock().unwrap();
        songs
            .as_ref()
            .and_then(|m| m.get(&song_id).cloned())
            .ok_or_else(|| "Song not found".to_string())?
    };

    let audio_path = song
        .vocals_path
        .clone()
        .filter(|path| Path::new(path).exists())
        .unwrap_or_else(|| song.original_path.clone());

    if !Path::new(&audio_path).exists() {
        return Err("找不到可用于转录的音频文件".to_string());
    }

    let python_bin = runtime::python::get_python_path(&app);
    if !python_bin.exists() {
        return Err("找不到 Python 运行时，无法生成 Whisper 草稿".to_string());
    }

    let model_dir = ensure_whisper_runtime_ready(&app)?;
    let song_dir = get_songs_dir().join(&song_id);
    ensure_dir(&song_dir).map_err(|e| e.to_string())?;
    let transcription_result_file = song_dir.join("whisper_transcription.json");

    let transcription_json =
        tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
            let script = r#"
import json
import os
import sys

from faster_whisper import WhisperModel

audio_path = os.environ["WHISPER_AUDIO_PATH"]
model_dir = os.environ["WHISPER_MODEL_DIR"]
result_file = os.environ["WHISPER_RESULT_PATH"]
device = os.environ.get("WHISPER_DEVICE", "cpu")
compute_type = os.environ.get("WHISPER_COMPUTE_TYPE", "int8")

model = WhisperModel(
    model_dir,
    device=device,
    compute_type=compute_type,
    local_files_only=True,
)

segments, info = model.transcribe(
    audio_path,
    beam_size=5,
    vad_filter=True,
    word_timestamps=True,
    condition_on_previous_text=False,
)

payload = {
    "language": getattr(info, "language", None),
    "language_probability": getattr(info, "language_probability", None),
    "segments": [],
}

for segment in segments:
    segment_words = []
    for word in getattr(segment, "words", None) or []:
        word_text = getattr(word, "word", "") or ""
        if not word_text.strip():
            continue
        segment_words.append({
            "start": getattr(word, "start", None),
            "end": getattr(word, "end", None),
            "word": word_text,
            "probability": getattr(word, "probability", None),
        })
    payload["segments"].append({
        "start": getattr(segment, "start", 0.0),
        "end": getattr(segment, "end", getattr(segment, "start", 0.0)),
        "text": getattr(segment, "text", "") or "",
        "words": segment_words,
    })

with open(result_file, "w", encoding="utf-8") as f:
    json.dump(payload, f, ensure_ascii=False)
print(json.dumps(payload, ensure_ascii=False))
"#;

            let mut cmd = Command::new(&python_bin);
            cmd.arg("-X")
                .arg("utf8")
                .arg("-c")
                .arg(script)
                .env("WHISPER_AUDIO_PATH", &audio_path)
                .env("WHISPER_MODEL_DIR", &model_dir)
                .env("WHISPER_RESULT_PATH", &transcription_result_file)
                .env("WHISPER_DEVICE", "cpu")
                .env("WHISPER_COMPUTE_TYPE", "int8")
                .env("PYTHONUTF8", "1")
                .env("PYTHONIOENCODING", "utf-8")
                .current_dir(&song_dir);
            process_control::configure_console_visibility(&mut cmd);
            let output = cmd
                .output()
                .map_err(|e| format!("Whisper base 运行失败: {}", e))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let detail = if !stderr.is_empty() { stderr } else { stdout };
                return Err(if detail.is_empty() {
                    "Whisper base 转录失败".to_string()
                } else {
                    format!("Whisper base 转录失败: {}", detail)
                });
            }

            if transcription_result_file.exists() {
                fs::read_to_string(&transcription_result_file)
                    .map_err(|e| format!("Whisper base 输出读取失败: {}", e))
            } else {
                String::from_utf8(output.stdout).map_err(|e| e.to_string())
            }
        })
        .await
        .map_err(|e| e.to_string())??;

    let transcription = serde_json::from_str::<WhisperTranscriptionResult>(&transcription_json)
        .map_err(|e| format!("Whisper base 输出解析失败: {}", e))?;

    let document = build_document_from_whisper_segments(
        &song_id,
        "whisper_base",
        "whisper_base",
        transcription.language.clone(),
        transcription.segments,
    )
    .ok_or_else(|| "Whisper base 没有生成可用歌词".to_string())?;

    let lyrics_path = persist_lyrics_document(&song_id, &document)?;

    Ok(GeneratedLyricsDraftResult {
        lyrics_path,
        document,
    })
}

#[tauri::command]
async fn read_file(path: String) -> Result<String, String> {
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn read_file_bytes(path: String) -> Result<Vec<u8>, String> {
    fs::read(&path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_audio_url(path: String) -> Result<String, String> {
    let path_buf = PathBuf::from(&path);
    if path_buf.exists() {
        let canonical = path_buf.canonicalize().unwrap_or(path_buf);
        // Use to_string_lossy and manually encode special characters
        let path_str = canonical.to_string_lossy();
        // Encode special characters: space, #, %, etc.
        let encoded: String = path_str
            .chars()
            .map(|c| match c {
                ' ' => "%20".to_string(),
                '#' => "%23".to_string(),
                '%' => "%25".to_string(),
                '<' => "%3C".to_string(),
                '>' => "%3E".to_string(),
                '"' => "%22".to_string(),
                '\'' => "%27".to_string(),
                '{' => "%7B".to_string(),
                '}' => "%7D".to_string(),
                '[' => "%5B".to_string(),
                ']' => "%5D".to_string(),
                '`' => "%60".to_string(),
                '\\' => "%5C".to_string(),
                '^' => "%5E".to_string(),
                '|' => "%7C".to_string(),
                '?' => "%3F".to_string(),
                '&' => "%26".to_string(),
                '=' => "%3D".to_string(),
                '+' => "%2B".to_string(),
                '$' => "%24".to_string(),
                '@' => "%40".to_string(),
                ':' => "%3A".to_string(),
                ';' => "%3B".to_string(),
                ',' => "%2C".to_string(),
                '(' => "%28".to_string(),
                ')' => "%29".to_string(),
                _ => c.to_string(),
            })
            .collect();
        Ok(format!("file://{}", encoded))
    } else {
        Err("File not found".to_string())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    ensure_file_storage_settings_loaded();
    load_songs_from_disk();
    migrate_library_assets();
    cleanup_interrupted_processing_jobs();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|_app| {
            let songs_dir = get_songs_dir();
            let _ = ensure_dir(&songs_dir);
            let data_dir = get_data_dir();
            let _ = ensure_dir(&data_dir);
            Ok(())
        })
        .on_window_event(|_window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                cancel_active_processing_jobs("窗口关闭，处理已停止");
            }
        })
        .invoke_handler(tauri::generate_handler![
            import_songs,
            start_process,
            reprocess_song,
            cancel_process,
            get_songs,
            get_song,
            delete_song,
            update_song_duration,
            rename_song,
            ensure_original_mix,
            set_song_folder,
            rename_playlist_folder,
            get_file_storage_settings,
            get_runtime_health,
            get_bootstrap_status,
            bootstrap_install_minimal,
            update_file_storage_settings,
            search_match_lyrics,
            generate_whisper_base_lyrics,
            get_lyrics_document,
            save_lyrics_document,
            read_file,
            read_file_bytes,
            get_audio_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
