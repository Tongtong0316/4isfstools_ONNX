use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Reverse;
use std::collections::HashSet;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

use crate::{
    ensure_dir, process_control, resolve_ffmpeg_binary_path, runtime, FileStorageSettings, SONGS,
};

const YTDLP_INSTALL_TIMEOUT: Duration = Duration::from_secs(8 * 60);
const ONLINE_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);
const PIP_TIMEOUT_SECONDS: &str = "120";
const PIP_RETRIES: &str = "3";
const DESKTOP_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

static ONLINE_DOWNLOAD_CANCELLED: AtomicBool = AtomicBool::new(false);
static ONLINE_DOWNLOAD_PID: Mutex<Option<u32>> = Mutex::new(None);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnlineImportStatus {
    python_ready: bool,
    python_path: String,
    ffmpeg_ready: bool,
    ffmpeg_path: Option<String>,
    ytdlp_ready: bool,
    ytdlp_version: Option<String>,
    download_root: String,
    can_download: bool,
    detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnlineDownloadResult {
    path: String,
    filename: String,
    source_id: Option<String>,
    source_url: Option<String>,
    source_title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnlineMediaProbe {
    source_id: Option<String>,
    source_url: Option<String>,
    title: Option<String>,
    has_video: bool,
    video_heights: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OnlineImportProgress {
    stage: String,
    progress: u32,
    message: String,
    path: Option<String>,
}

pub(crate) fn emit_online_progress(
    app: &AppHandle,
    stage: &str,
    progress: u32,
    message: &str,
    path: Option<String>,
) {
    let _ = app.emit(
        "online-import-progress",
        OnlineImportProgress {
            stage: stage.to_string(),
            progress: progress.min(100),
            message: message.to_string(),
            path,
        },
    );
}

fn python_path(app: &AppHandle) -> PathBuf {
    runtime::python::get_python_path(app)
}

fn run_python_module(app: &AppHandle, module: &str, args: &[&str]) -> Result<String, String> {
    let python = python_path(app);
    if !python.exists() {
        return Err("Python 运行时未就绪".to_string());
    }
    let mut command = Command::new(python);
    command.arg("-m").arg(module).args(args);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    process_control::configure_console_visibility(&mut command);
    let output = command
        .output()
        .map_err(|e| format!("执行 {} 失败：{}", module, e))?;
    if !output.status.success() {
        return Err(format!(
            "{} 执行失败：{}",
            module,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn ytdlp_version(app: &AppHandle) -> Option<String> {
    run_python_module(app, "yt_dlp", &["--version"]).ok()
}

#[derive(Debug, Clone)]
struct OnlineMediaInfo {
    source_id: Option<String>,
    source_url: Option<String>,
    title: Option<String>,
}

fn normalize_source_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_ascii_lowercase()
}

fn normalize_input_url(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{}", trimmed.trim_start_matches('/'))
    }
}

fn is_bilibili_url(url: &str) -> bool {
    let normalized = normalize_source_url(url);
    normalized.contains("bilibili.com") || normalized.contains("b23.tv")
}

fn add_bilibili_headers(command: &mut Command) {
    command
        .arg("--user-agent")
        .arg(DESKTOP_USER_AGENT)
        .arg("--referer")
        .arg("https://www.bilibili.com/")
        .arg("--add-headers")
        .arg("Origin:https://www.bilibili.com")
        .arg("--add-headers")
        .arg("Accept-Language:zh-CN,zh;q=0.9,en;q=0.8")
        .arg("--add-headers")
        .arg("Accept:text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8");
}

fn build_bilibili_ytdlp_args(url: &str, dump_single_json: bool) -> Vec<String> {
    let mut args = vec!["--no-playlist".to_string()];
    if dump_single_json {
        args.push("--dump-single-json".to_string());
    }
    if is_bilibili_url(url) {
        args.extend([
            "--user-agent".to_string(),
            DESKTOP_USER_AGENT.to_string(),
            "--referer".to_string(),
            "https://www.bilibili.com/".to_string(),
            "--add-headers".to_string(),
            "Origin:https://www.bilibili.com".to_string(),
            "--add-headers".to_string(),
            "Accept-Language:zh-CN,zh;q=0.9,en;q=0.8".to_string(),
            "--add-headers".to_string(),
            "Accept:text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8".to_string(),
        ]);
    }
    args.push(url.to_string());
    args
}

fn fetch_online_media_payload(app: &AppHandle, url: &str) -> Option<Value> {
    let args = build_bilibili_ytdlp_args(url, true);
    let refs = args.iter().map(|value| value.as_str()).collect::<Vec<_>>();
    let output = run_python_module(app, "yt_dlp", &refs).ok()?;
    serde_json::from_str(&output).ok()
}

fn online_media_info_from_payload(payload: &Value) -> OnlineMediaInfo {
    let extractor = payload
        .get("extractor_key")
        .or_else(|| payload.get("extractor"))
        .and_then(|value| value.as_str())
        .unwrap_or("online");
    let id = payload.get("id").and_then(|value| value.as_str());
    let source_id = id.map(|value| format!("{}:{}", extractor, value));
    let source_url = payload
        .get("webpage_url")
        .or_else(|| payload.get("original_url"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let title = payload
        .get("title")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    OnlineMediaInfo {
        source_id,
        source_url,
        title,
    }
}

fn online_media_video_heights(payload: &Value) -> Vec<u32> {
    let Some(formats) = payload.get("formats").and_then(|value| value.as_array()) else {
        return Vec::new();
    };
    let mut heights = formats
        .iter()
        .filter_map(|format| {
            let has_video = format
                .get("vcodec")
                .and_then(|value| value.as_str())
                .map(|value| value != "none")
                .unwrap_or(false);
            if !has_video {
                return None;
            }
            format
                .get("height")
                .and_then(|value| value.as_u64())
                .and_then(|height| u32::try_from(height).ok())
                .filter(|height| *height > 0)
        })
        .collect::<Vec<_>>();
    heights.sort_unstable_by(|left, right| right.cmp(left));
    heights.dedup();
    heights
}

fn online_media_has_video(payload: &Value) -> bool {
    payload
        .get("formats")
        .and_then(|value| value.as_array())
        .map(|formats| {
            formats.iter().any(|format| {
                format
                    .get("vcodec")
                    .and_then(|value| value.as_str())
                    .map(|value| value != "none")
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn online_source_exists(source_id: Option<&str>, source_url: Option<&str>) -> bool {
    let normalized_url = source_url.map(normalize_source_url);
    let songs = SONGS.lock().unwrap();
    let Some(map) = songs.as_ref() else {
        return false;
    };
    map.values().any(|song| {
        if song.source_kind.as_deref() != Some("online") {
            return false;
        }
        if let Some(source_id) = source_id {
            if !source_id.is_empty() && song.source_id.as_deref() == Some(source_id) {
                return true;
            }
        }
        if let (Some(expected), Some(actual)) = (&normalized_url, song.source_url.as_deref()) {
            return normalize_source_url(actual) == *expected;
        }
        false
    })
}

fn fetch_online_media_info(app: &AppHandle, url: &str) -> Option<OnlineMediaInfo> {
    let payload = fetch_online_media_payload(app, url)?;
    Some(online_media_info_from_payload(&payload))
}

#[tauri::command]
pub async fn probe_online_media(app: AppHandle, url: String) -> Result<OnlineMediaProbe, String> {
    let trimmed_url = normalize_input_url(&url);
    let payload = fetch_online_media_payload(&app, &trimmed_url)
        .ok_or_else(|| "无法读取在线视频信息，请检查链接或网络".to_string())?;
    let info = online_media_info_from_payload(&payload);
    let video_heights = online_media_video_heights(&payload);
    Ok(OnlineMediaProbe {
        source_id: info.source_id,
        source_url: info.source_url,
        title: info.title,
        has_video: online_media_has_video(&payload),
        video_heights,
    })
}

#[tauri::command]
pub async fn get_online_import_status(
    app: AppHandle,
    settings: Option<FileStorageSettings>,
) -> Result<OnlineImportStatus, String> {
    let python = python_path(&app);
    let python_ready = python.exists();
    let ffmpeg_path = resolve_ffmpeg_binary_path();
    let ytdlp_version = ytdlp_version(&app);
    let download_root = settings.map(|s| s.online_download_root).unwrap_or_else(|| {
        crate::storage::get_default_online_download_root()
            .to_string_lossy()
            .to_string()
    });
    let ffmpeg_ready = ffmpeg_path.is_some();
    let ytdlp_ready = ytdlp_version.is_some();
    let can_download = python_ready && ffmpeg_ready && ytdlp_ready;
    let detail = if can_download {
        "在线导入组件已就绪".to_string()
    } else if !python_ready {
        "Python 运行时未就绪，请先完成核心环境部署".to_string()
    } else if !ffmpeg_ready {
        "FFmpeg 未就绪，请先完成核心环境部署".to_string()
    } else {
        "yt-dlp 未安装，请安装在线导入组件".to_string()
    };
    Ok(OnlineImportStatus {
        python_ready,
        python_path: python.to_string_lossy().to_string(),
        ffmpeg_ready,
        ffmpeg_path: ffmpeg_path.map(|p| p.to_string_lossy().to_string()),
        ytdlp_ready,
        ytdlp_version,
        download_root,
        can_download,
        detail,
    })
}

#[tauri::command]
pub async fn install_or_update_ytdlp(app: AppHandle) -> Result<OnlineImportStatus, String> {
    let python = python_path(&app);
    if !python.exists() {
        return Err("Python 运行时未就绪，请先完成核心环境部署".to_string());
    }

    let mirrors = [
        (
            "https://pypi.tuna.tsinghua.edu.cn/simple",
            "pypi.tuna.tsinghua.edu.cn",
        ),
        (
            "https://mirrors.aliyun.com/pypi/simple",
            "mirrors.aliyun.com",
        ),
        (
            "https://mirrors.cloud.tencent.com/pypi/simple",
            "mirrors.cloud.tencent.com",
        ),
        (
            "https://repo.huaweicloud.com/repository/pypi/simple",
            "repo.huaweicloud.com",
        ),
        ("https://pypi.org/simple", "pypi.org"),
    ];

    let mut errors = Vec::new();
    for (mirror, host) in mirrors {
        emit_online_progress(
            &app,
            "installing",
            18,
            &format!("正在从 {} 安装/更新 yt-dlp...", host),
            None,
        );
        let start = Instant::now();
        let mut command = Command::new(&python);
        command.args([
            "-m",
            "pip",
            "install",
            "-U",
            "--disable-pip-version-check",
            "--no-input",
            "--timeout",
            PIP_TIMEOUT_SECONDS,
            "--retries",
            PIP_RETRIES,
            "-i",
            mirror,
            "--trusted-host",
            host,
            "yt-dlp",
        ]);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        process_control::configure_console_visibility(&mut command);
        let mut child = command
            .spawn()
            .map_err(|e| format!("启动 yt-dlp 安装失败：{}", e))?;
        loop {
            if start.elapsed() >= YTDLP_INSTALL_TIMEOUT {
                let _ = child.kill();
                let _ = child.wait();
                errors.push(format!("[{}] 安装超时", mirror));
                break;
            }
            if let Some(status) = child
                .try_wait()
                .map_err(|e| format!("等待 yt-dlp 安装失败：{}", e))?
            {
                let output = child
                    .wait_with_output()
                    .map_err(|e| format!("读取 yt-dlp 安装输出失败：{}", e))?;
                if status.success() {
                    emit_online_progress(&app, "installed", 100, "yt-dlp 已就绪", None);
                    return get_online_import_status(app, None).await;
                }
                errors.push(format!(
                    "[{}] {} {}",
                    mirror,
                    String::from_utf8_lossy(&output.stderr).trim(),
                    String::from_utf8_lossy(&output.stdout).trim()
                ));
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    Err(format!("yt-dlp 安装/更新失败：{}", errors.join(" | ")))
}

#[tauri::command]
pub async fn cancel_online_download() -> Result<(), String> {
    ONLINE_DOWNLOAD_CANCELLED.store(true, Ordering::SeqCst);
    if let Some(pid) = *ONLINE_DOWNLOAD_PID.lock().unwrap() {
        #[cfg(unix)]
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
    Ok(())
}

fn newest_file_in(dir: &Path, after: Instant) -> Option<PathBuf> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let metadata = entry.metadata().ok()?;
        let modified = metadata.modified().ok()?;
        let elapsed_ok = modified
            .elapsed()
            .map(|elapsed| elapsed <= after.elapsed() + Duration::from_secs(5))
            .unwrap_or(true);
        if !elapsed_ok {
            continue;
        }
        if best
            .as_ref()
            .map(|(time, _)| modified > *time)
            .unwrap_or(true)
        {
            best = Some((modified, path));
        }
    }
    best.map(|(_, path)| path)
}

fn first_downloaded_file_in(dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    let mut candidates = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if file_name.ends_with(".part")
            || file_name.ends_with(".ytdl")
            || file_name.ends_with(".temp")
            || file_name.contains(".part.")
        {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        candidates.push((modified, path));
    }
    candidates.sort_by_key(|item| Reverse(item.0));
    candidates.into_iter().map(|(_, path)| path).next()
}

fn snapshot_files(dir: &Path) -> HashSet<PathBuf> {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_file())
                .collect()
        })
        .unwrap_or_default()
}

fn cleanup_new_online_files(dir: &Path, existing_files: &HashSet<PathBuf>, after: Instant) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || existing_files.contains(&path) {
            continue;
        }
        let modified_recently = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .map(|elapsed| elapsed <= after.elapsed() + Duration::from_secs(5))
            .unwrap_or(true);
        if modified_recently {
            let _ = fs::remove_file(path);
        }
    }
}

fn cleanup_download_work_dir(
    dir: &Path,
    is_temporary: bool,
    existing_files: &HashSet<PathBuf>,
    after: Instant,
) {
    if is_temporary {
        let _ = fs::remove_dir_all(dir);
    } else {
        cleanup_new_online_files(dir, existing_files, after);
    }
}

fn video_tail_progress(elapsed: Duration) -> u32 {
    let seconds = elapsed.as_secs_f32();
    let step = (seconds * 1.8).round() as u32;
    82u32.saturating_add(step.min(14))
}

#[tauri::command]
pub async fn download_online_audio(
    app: AppHandle,
    url: String,
    output_dir: String,
    check_duplicate: Option<bool>,
    download_kind: Option<String>,
    video_height: Option<u32>,
) -> Result<OnlineDownloadResult, String> {
    if url.trim().is_empty() {
        return Err("请输入视频链接".to_string());
    }
    let trimmed_url = normalize_input_url(&url);
    let target_dir = PathBuf::from(output_dir.trim());
    if target_dir.as_os_str().is_empty() {
        return Err("请选择下载目录".to_string());
    }
    ensure_dir(&target_dir).map_err(|e| format!("创建下载目录失败：{}", e))?;
    if ytdlp_version(&app).is_none() {
        return Err("yt-dlp 未安装，请先安装在线导入组件".to_string());
    }
    if resolve_ffmpeg_binary_path().is_none() {
        return Err("FFmpeg 未就绪，请先完成核心环境部署".to_string());
    }

    emit_online_progress(&app, "checking", 3, "正在读取视频信息...", None);
    let media_info = fetch_online_media_info(&app, &trimmed_url);
    if check_duplicate.unwrap_or(false) {
        if let Some(info) = media_info.as_ref() {
            if online_source_exists(info.source_id.as_deref(), info.source_url.as_deref()) {
                return Err("该在线视频已导入过，请在歌曲列表中处理已有歌曲".to_string());
            }
        }
    }

    ONLINE_DOWNLOAD_CANCELLED.store(false, Ordering::SeqCst);
    emit_online_progress(&app, "starting", 5, "正在准备在线下载...", None);
    let started_at = Instant::now();
    let use_temporary_dir = check_duplicate.unwrap_or(false);
    let download_dir = if use_temporary_dir {
        let task_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let task_dir = target_dir.join(".tmp").join(format!("online_{}", task_id));
        ensure_dir(&task_dir).map_err(|e| format!("创建在线下载临时目录失败：{}", e))?;
        task_dir
    } else {
        target_dir.clone()
    };
    let existing_files = snapshot_files(&download_dir);
    let output_template = download_dir.join("%(title).120B-%(id)s.%(ext)s");
    let mut command = Command::new(python_path(&app));
    command
        .arg("-m")
        .arg("yt_dlp")
        .arg("--newline")
        .arg("--no-playlist")
        .arg("--socket-timeout")
        .arg("30")
        .arg("--retries")
        .arg("3")
        .arg("-o")
        .arg(output_template);
    let download_kind = if check_duplicate.unwrap_or(false) {
        "audio".to_string()
    } else {
        download_kind
            .as_deref()
            .map(|value| value.to_ascii_lowercase())
            .unwrap_or_else(|| "audio".to_string())
    };
    if download_kind == "video" {
        let quality_expr = if let Some(height) = video_height.filter(|height| *height > 0) {
            format!(
                "bestvideo[height<={}]+bestaudio/best[height<={}]/best",
                height, height
            )
        } else {
            "bestvideo+bestaudio/best".to_string()
        };
        command
            .arg("--fragment-retries")
            .arg("3")
            .arg("-f")
            .arg(quality_expr)
            .arg("--recode-video")
            .arg("mp4");
    } else {
        command
            .arg("--fragment-retries")
            .arg("3")
            .arg("-x")
            .arg("--audio-format")
            .arg("m4a")
            .arg("--audio-quality")
            .arg("0");
    }
    if is_bilibili_url(&trimmed_url) {
        add_bilibili_headers(&mut command);
    }
    command
        .arg(&trimmed_url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    process_control::configure_console_visibility(&mut command);

    let mut child = command
        .spawn()
        .map_err(|e| format!("启动在线下载失败：{}", e))?;
    *ONLINE_DOWNLOAD_PID.lock().unwrap() = Some(child.id());
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let video_tail_started_at: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let last_online_progress = Arc::new(AtomicU32::new(0));
    let app_for_stdout = app.clone();
    let download_kind_for_stdout = download_kind.clone();
    let video_tail_started_at_for_stdout = Arc::clone(&video_tail_started_at);
    let last_online_progress_for_stdout = Arc::clone(&last_online_progress);
    let stdout_thread = stdout.map(|stream| {
        std::thread::spawn(move || {
            let reader = BufReader::new(stream);
            for line in reader.lines().map_while(Result::ok) {
                let is_video_mode = download_kind_for_stdout == "video";
                let mut progress = if line.contains("[download] 100%") {
                    if is_video_mode { 78 } else { 92 }
                } else if line.contains("[download]") {
                    50
                } else if line.contains("[ExtractAudio]") {
                    88
                } else if line.contains("[Merger]")
                    || line.contains("[VideoRemuxer]")
                    || line.contains("[ffmpeg]")
                    || line.contains("Merging formats")
                    || line.contains("Fixup")
                {
                    if is_video_mode {
                        let mut tail_guard = video_tail_started_at_for_stdout.lock().unwrap();
                        if tail_guard.is_none() {
                            *tail_guard = Some(Instant::now());
                        }
                        82
                    } else {
                        32
                    }
                } else {
                    32
                };
                if is_video_mode {
                    if let Some(started) = *video_tail_started_at_for_stdout.lock().unwrap() {
                        if line.contains("[download] 100%")
                            || line.contains("[Merger]")
                            || line.contains("[VideoRemuxer]")
                            || line.contains("[ffmpeg]")
                            || line.contains("Merging formats")
                            || line.contains("Fixup")
                        {
                            progress = progress.max(82);
                        } else if progress >= 82 {
                            progress = progress.max(video_tail_progress(started.elapsed()));
                        }
                    }
                }
                let previous = last_online_progress_for_stdout.load(Ordering::SeqCst);
                if progress > previous {
                    last_online_progress_for_stdout.store(progress, Ordering::SeqCst);
                } else {
                    progress = previous;
                }
                emit_online_progress(&app_for_stdout, "downloading", progress, line.trim(), None);
            }
        })
    });
    let stderr_thread = stderr.map(|stream| {
        std::thread::spawn(move || {
            let reader = BufReader::new(stream);
            reader
                .lines()
                .map_while(Result::ok)
                .collect::<Vec<_>>()
                .join("\n")
        })
    });

    loop {
        if ONLINE_DOWNLOAD_CANCELLED.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            *ONLINE_DOWNLOAD_PID.lock().unwrap() = None;
            cleanup_download_work_dir(
                &download_dir,
                use_temporary_dir,
                &existing_files,
                started_at,
            );
            emit_online_progress(&app, "cancelled", 0, "在线下载已取消", None);
            return Err("在线下载已取消".to_string());
        }
        if started_at.elapsed() >= ONLINE_DOWNLOAD_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            *ONLINE_DOWNLOAD_PID.lock().unwrap() = None;
            cleanup_download_work_dir(
                &download_dir,
                use_temporary_dir,
                &existing_files,
                started_at,
            );
            emit_online_progress(&app, "error", 0, "在线下载超时", None);
            return Err("在线下载超时，请检查网络或链接有效性".to_string());
        }
        if download_kind == "video" {
            if let Some(started) = *video_tail_started_at.lock().unwrap() {
                let progress = video_tail_progress(started.elapsed());
                let previous = last_online_progress.load(Ordering::SeqCst);
                if progress > previous && progress < 100 {
                    last_online_progress.store(progress, Ordering::SeqCst);
                    emit_online_progress(
                        &app,
                        "downloading",
                        progress,
                        "正在生成可播放视频文件...",
                        None,
                    );
                }
            }
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|e| format!("等待在线下载失败：{}", e))?
        {
            let _ = stdout_thread.map(|handle| handle.join());
            let stderr_text = stderr_thread
                .and_then(|handle| handle.join().ok())
                .unwrap_or_default();
            *ONLINE_DOWNLOAD_PID.lock().unwrap() = None;
            if !status.success() {
                cleanup_download_work_dir(
                    &download_dir,
                    use_temporary_dir,
                    &existing_files,
                    started_at,
                );
                emit_online_progress(&app, "error", 0, "在线下载失败", None);
                return Err(format!("在线下载失败：{}", stderr_text.trim()));
            }
            let path = if use_temporary_dir {
                first_downloaded_file_in(&download_dir)
            } else {
                newest_file_in(&download_dir, started_at)
                    .or_else(|| first_downloaded_file_in(&download_dir))
            }
            .ok_or_else(|| "下载完成但未找到输出文件".to_string())?;
            let filename = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("online-audio.m4a")
                .to_string();
            let path_string = path.to_string_lossy().to_string();
            emit_online_progress(
                &app,
                "complete",
                100,
                "在线音频下载完成",
                Some(path_string.clone()),
            );
            return Ok(OnlineDownloadResult {
                path: path_string,
                filename,
                source_id: media_info.as_ref().and_then(|info| info.source_id.clone()),
                source_url: media_info
                    .as_ref()
                    .and_then(|info| info.source_url.clone())
                    .or_else(|| Some(trimmed_url.clone())),
                source_title: media_info.as_ref().and_then(|info| info.title.clone()),
            });
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}
