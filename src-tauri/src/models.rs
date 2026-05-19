use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

// ── Core domain types ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Song {
    pub id: String,
    pub name: String,
    pub original_path: String,
    #[serde(default)]
    pub playlist_folder: Option<String>,
    #[serde(default)]
    pub vocals_path: Option<String>,
    #[serde(default)]
    pub instrumental_path: Option<String>,
    #[serde(default)]
    pub original_mix_path: Option<String>,
    #[serde(default)]
    pub lyrics_path: Option<String>,
    pub duration: u64,
    pub status: String,
    pub progress: u32,
    #[serde(default)]
    pub processing_stage: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
    pub added_at: u64,
}

// ── Lyric document types ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricToken {
    pub id: String,
    pub line_id: String,
    pub index: u32,
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub confidence: f32,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricLineDoc {
    pub id: String,
    pub index: u32,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub confidence: f32,
    pub edited: bool,
    pub locked: bool,
    pub tokens: Vec<LyricToken>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricDocument {
    pub song_id: String,
    pub version: u32,
    pub language: Option<String>,
    pub source: String,
    pub alignment_engine: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub global_offset_ms: i64,
    pub dirty: bool,
    pub quality_score: f32,
    pub lines: Vec<LyricLineDoc>,
}

// ── Whisper transcription types ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhisperWordResult {
    pub start: Option<f64>,
    pub end: Option<f64>,
    pub word: String,
    pub probability: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhisperSegmentResult {
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub words: Option<Vec<WhisperWordResult>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhisperTranscriptionResult {
    pub language: Option<String>,
    pub language_probability: Option<f64>,
    pub segments: Vec<WhisperSegmentResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedLyricsDraftResult {
    pub lyrics_path: String,
    pub document: LyricDocument,
}

// ── Runtime / bootstrap status types ────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeHealthCheck {
    pub name: String,
    pub ok: bool,
    pub severity: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeHealthReport {
    pub level: String,
    pub label: String,
    pub detail: String,
    #[serde(default)]
    pub separation_engine: SeparationEngineHealth,
    pub torch_cuda_available: bool,
    pub selected_device: String,
    pub torch_version: Option<String>,
    pub torch_cuda_version: Option<String>,
    pub torch_cuda_device_name: Option<String>,
    pub has_nvidia_gpu: bool,
    pub nvidia_driver_visible: bool,
    pub nvidia_driver_cuda_version: Option<String>,
    pub checks: Vec<RuntimeHealthCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeparationEngineHealth {
    pub active_engine: String,
    pub legacy_fallback_engine: String,
    pub requested_providers: Vec<String>,
    pub available_providers: Vec<String>,
    pub selected_provider: String,
    pub provider_fallback_reason: Option<String>,
    pub default_model_id: String,
    pub default_model_path: String,
    pub default_model_ready: bool,
    pub default_model_session_load_ok: bool,
    pub default_model_session_load_error: Option<String>,
    pub default_model_metadata_ok: bool,
    pub default_model_metadata_error: Option<String>,
    pub default_model_input_shape: Option<Vec<String>>,
    pub default_model_output_shape: Option<Vec<String>>,
    pub default_model_dummy_inference_ok: Option<bool>,
    pub default_model_dummy_inference_error: Option<String>,
    pub high_quality_model_id: Option<String>,
    pub high_quality_model_path: String,
    pub high_quality_model_ready: bool,
    pub high_quality_model_session_load_ok: bool,
    pub high_quality_model_session_load_error: Option<String>,
    pub high_quality_model_metadata_ok: bool,
    pub high_quality_model_metadata_error: Option<String>,
    pub high_quality_model_input_shape: Option<Vec<String>>,
    pub high_quality_model_output_shape: Option<Vec<String>>,
    pub high_quality_model_dummy_inference_ok: Option<bool>,
    pub high_quality_model_dummy_inference_error: Option<String>,
    pub onnxruntime_available: bool,
    pub legacy_demucs_available: bool,
    pub probe_error: Option<String>,
}

impl Default for SeparationEngineHealth {
    fn default() -> Self {
        Self {
            active_engine: "onnx".to_string(),
            legacy_fallback_engine: "legacy_demucs".to_string(),
            requested_providers: vec!["CPUExecutionProvider".to_string()],
            available_providers: vec!["unavailable".to_string()],
            selected_provider: "CPUExecutionProvider".to_string(),
            provider_fallback_reason: Some("ONNX Runtime API not initialized".to_string()),
            default_model_id: "uvr_mdxnet_9482".to_string(),
            default_model_path: String::new(),
            default_model_ready: false,
            default_model_session_load_ok: false,
            default_model_session_load_error: Some("ONNX Runtime API not initialized".to_string()),
            default_model_metadata_ok: false,
            default_model_metadata_error: Some("ONNX Runtime API not initialized".to_string()),
            default_model_input_shape: None,
            default_model_output_shape: None,
            default_model_dummy_inference_ok: None,
            default_model_dummy_inference_error: None,
            high_quality_model_id: Some("bs_polarformer_fp16".to_string()),
            high_quality_model_path: String::new(),
            high_quality_model_ready: false,
            high_quality_model_session_load_ok: false,
            high_quality_model_session_load_error: None,
            high_quality_model_metadata_ok: false,
            high_quality_model_metadata_error: None,
            high_quality_model_input_shape: None,
            high_quality_model_output_shape: None,
            high_quality_model_dummy_inference_ok: None,
            high_quality_model_dummy_inference_error: None,
            onnxruntime_available: false,
            legacy_demucs_available: false,
            probe_error: Some("ONNX Runtime API not initialized".to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnnxRuntimeProbeResult {
    pub onnxruntime_available: bool,
    pub available_providers: Vec<String>,
    pub selected_provider: String,
    pub provider_fallback_reason: Option<String>,
    pub session_load_ok: bool,
    pub session_load_error: Option<String>,
    pub model_metadata_ok: bool,
    pub model_metadata_error: Option<String>,
    pub probe_error: Option<String>,
}

impl Default for OnnxRuntimeProbeResult {
    fn default() -> Self {
        Self {
            onnxruntime_available: false,
            available_providers: vec!["unavailable".to_string()],
            selected_provider: "CPUExecutionProvider".to_string(),
            provider_fallback_reason: Some("ONNX Runtime probe not initialized".to_string()),
            session_load_ok: false,
            session_load_error: None,
            model_metadata_ok: false,
            model_metadata_error: None,
            probe_error: Some("ONNX Runtime probe not initialized".to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnnxModelProbeResult {
    pub model_path: String,
    pub model_ready: bool,
    pub session_load_ok: bool,
    pub session_load_error: Option<String>,
    pub model_metadata_ok: bool,
    pub model_metadata_error: Option<String>,
    pub input_shape: Option<Vec<String>>,
    pub output_shape: Option<Vec<String>>,
    pub dummy_inference_ok: Option<bool>,
    pub dummy_inference_error: Option<String>,
    pub probe_error: Option<String>,
}

impl Default for OnnxModelProbeResult {
    fn default() -> Self {
        Self {
            model_path: String::new(),
            model_ready: false,
            session_load_ok: false,
            session_load_error: None,
            model_metadata_ok: false,
            model_metadata_error: None,
            input_shape: None,
            output_shape: None,
            dummy_inference_ok: None,
            dummy_inference_error: None,
            probe_error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapStatus {
    pub runtime_ready: bool,
    pub demucs_models_ready: bool,
    pub whisper_base_ready: bool,
    pub ffmpeg_ready: bool,
    pub can_run_core: bool,
    pub torch_cuda_available: bool,
    pub selected_device: String,
    pub torch_version: Option<String>,
    pub torch_cuda_version: Option<String>,
    pub torch_cuda_device_name: Option<String>,
    pub has_nvidia_gpu: bool,
    pub nvidia_driver_visible: bool,
    pub nvidia_driver_cuda_version: Option<String>,
    pub detail: String,
}

// ── Runtime manifest types ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeManifest {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub platforms: RuntimeManifestPlatforms,
    #[serde(default)]
    pub model_sources: RuntimeManifestModelSources,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeManifestModelSources {
    #[serde(default, deserialize_with = "deserialize_model_source_urls")]
    pub demucs: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_model_source_urls")]
    pub whisper_base: Vec<String>,
}

fn deserialize_model_source_urls<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let items = Vec::<Value>::deserialize(deserializer)?;
    let mut urls = Vec::new();
    for item in items {
        match item {
            Value::String(url) if !url.is_empty() => urls.push(url),
            Value::Object(map) => {
                if let Some(url) = map.get("url").and_then(|value| value.as_str()) {
                    if !url.is_empty() {
                        urls.push(url.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    Ok(urls)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeManifestPlatforms {
    #[serde(default)]
    pub macos: RuntimeManifestPlatform,
    #[serde(default)]
    pub windows: RuntimeManifestPlatform,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeManifestPlatform {
    #[serde(default)]
    pub python_runtime_sources: Vec<RuntimeManifestArtifact>,
    #[serde(default)]
    pub ffmpeg_sources: Vec<RuntimeManifestArtifact>,
    #[serde(default)]
    pub models: RuntimeManifestPlatformModels,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeManifestPlatformModels {
    #[serde(default)]
    pub demucs: Vec<RuntimeManifestArtifact>,
    #[serde(default)]
    pub whisper_base: Vec<RuntimeManifestArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeManifestArtifact {
    pub url: String,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub target_relpath: Option<String>,
    #[serde(default)]
    pub inline_text: Option<String>,
}

// ── GPU / CUDA capability type ──────────────────────────────────────

#[derive(Default)]
pub struct TorchCudaCapability {
    pub has_nvidia_gpu: bool,
    pub nvidia_driver_visible: bool,
    pub nvidia_gpu_name: Option<String>,
    pub nvidia_driver_cuda_version: Option<String>,
    pub torch_installed: bool,
    pub torch_version: Option<String>,
    pub torch_cuda_available: bool,
    pub torch_cuda_version: Option<String>,
    pub torch_cuda_device_name: Option<String>,
    pub selected_device: String,
}

// ── External lyrics API response types ──────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct LrclibTrack {
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    #[serde(rename = "trackName", alias = "track_name")]
    pub track_name: Option<String>,
    #[serde(default)]
    #[serde(rename = "artistName", alias = "artist_name")]
    pub artist_name: Option<String>,
    #[serde(default)]
    #[serde(rename = "albumName", alias = "album_name")]
    pub album_name: Option<String>,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub instrumental: Option<bool>,
    #[serde(default)]
    #[serde(rename = "plainLyrics", alias = "plain_lyrics")]
    pub plain_lyrics: Option<String>,
    #[serde(default)]
    #[serde(rename = "syncedLyrics", alias = "synced_lyrics")]
    pub synced_lyrics: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NeteaseSearchResponse {
    #[serde(default)]
    pub result: Option<NeteaseSearchResult>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NeteaseSearchResult {
    #[serde(default)]
    pub songs: Vec<NeteaseSong>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NeteaseSong {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub duration: Option<u64>,
    #[serde(default)]
    pub artists: Vec<NeteaseArtist>,
    #[serde(default)]
    pub album: Option<NeteaseAlbum>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NeteaseArtist {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NeteaseAlbum {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NeteaseLyricBlock {
    #[serde(default)]
    pub lyric: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NeteaseLyricResponse {
    #[serde(default)]
    pub lrc: Option<NeteaseLyricBlock>,
    #[serde(default)]
    pub tlyric: Option<NeteaseLyricBlock>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QqSearchResponse {
    pub data: Option<QqSearchData>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QqSearchData {
    #[serde(default)]
    pub song: Option<QqSongContainer>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QqSongContainer {
    #[serde(default)]
    pub list: Vec<QqSong>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QqSinger {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QqSong {
    #[serde(default)]
    pub songmid: Option<String>,
    #[serde(default)]
    pub songname: Option<String>,
    #[serde(default)]
    pub singer: Vec<QqSinger>,
    #[serde(default)]
    pub albumname: Option<String>,
    #[serde(default)]
    pub interval: Option<u64>,
}

// ── Lyrics search / candidate types ─────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsCandidate {
    pub id: String,
    pub source: String,
    pub source_label: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub score: i32,
    pub synced: bool,
    pub preview: String,
    pub document: LyricDocument,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedLyricsCandidateBundle {
    pub cached_at: u64,
    pub candidates: Vec<LyricsCandidate>,
}

pub struct LyricsSearchIntent {
    pub query_track: String,
    pub query_artist: Option<String>,
    pub variants: Vec<(Option<String>, String)>,
    pub allow_weak_fallback: bool,
}

pub enum LyricsCandidateTier {
    Strong = 2,
    Acceptable = 1,
    Weak = 0,
}
