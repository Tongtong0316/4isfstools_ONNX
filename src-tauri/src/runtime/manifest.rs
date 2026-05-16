use std::fs;
use std::path::Path;

use tauri::{AppHandle, Manager};

use crate::models::{RuntimeManifest, RuntimeManifestArtifact, RuntimeManifestPlatform};

pub fn parse_manifest(path: &Path) -> Option<RuntimeManifest> {
    if !path.exists() {
        return None;
    }
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str::<RuntimeManifest>(&raw).ok()
}

pub fn load_runtime_manifest(
    app: &AppHandle,
    runtime_dir: &Path,
    project_root: &Path,
) -> RuntimeManifest {
    let runtime_manifest = runtime_dir.join("runtime-manifest.json");
    if let Some(manifest) = parse_manifest(&runtime_manifest) {
        return manifest;
    }
    let resource_manifest = app
        .path()
        .resource_dir()
        .unwrap_or_default()
        .join("runtime-manifest.json");
    if let Some(manifest) = parse_manifest(&resource_manifest) {
        return manifest;
    }
    let project_manifest = project_root.join("runtime-manifest.json");
    parse_manifest(&project_manifest).unwrap_or_default()
}

pub fn current_platform_manifest(manifest: &RuntimeManifest) -> RuntimeManifestPlatform {
    if cfg!(windows) {
        let mut platform = manifest.platforms.windows.clone();
        if platform.models.demucs.is_empty() {
            platform.models.demucs = manifest.platforms.macos.models.demucs.clone();
        }
        if platform.models.whisper_base.is_empty() {
            platform.models.whisper_base = manifest.platforms.macos.models.whisper_base.clone();
        }
        platform
    } else {
        manifest.platforms.macos.clone()
    }
}

pub fn legacy_model_artifacts(
    manifest: &RuntimeManifest,
    model_name: &str,
) -> Vec<RuntimeManifestArtifact> {
    let urls = if model_name == "demucs" {
        manifest.model_sources.demucs.clone()
    } else {
        manifest.model_sources.whisper_base.clone()
    };
    urls.into_iter()
        .map(|url| RuntimeManifestArtifact {
            url,
            sha256: None,
            note: Some("legacy modelSources".to_string()),
            target_relpath: None,
            inline_text: None,
        })
        .collect()
}
