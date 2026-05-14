use serde::{Deserialize, Serialize};

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
    #[serde(default)]
    pub demucs: Vec<String>,
    #[serde(default)]
    pub whisper_base: Vec<String>,
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
