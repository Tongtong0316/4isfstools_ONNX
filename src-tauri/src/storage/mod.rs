use std::fs;
use std::path::PathBuf;

use crate::FileStorageSettings;

pub(crate) fn get_data_dir() -> PathBuf {
    if let Ok(override_dir) = std::env::var("FORISFSTOOLS_DATA_DIR") {
        let trimmed = override_dir.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("4isfstools")
}

pub(crate) fn get_songs_dir() -> PathBuf {
    get_data_dir().join("songs")
}

pub(crate) fn get_library_path() -> PathBuf {
    get_data_dir().join("library.json")
}

pub(crate) fn get_lyrics_search_cache_path() -> PathBuf {
    get_data_dir().join("lyrics_search_cache.json")
}

pub(crate) fn get_file_storage_settings_path() -> PathBuf {
    get_data_dir().join("file_storage_settings.json")
}

pub(crate) fn get_default_asset_root(kind: &str) -> PathBuf {
    get_data_dir().join("assets").join(kind)
}

pub(crate) fn get_default_online_download_root() -> PathBuf {
    get_data_dir().join("online-downloads")
}

pub(crate) fn ensure_dir(path: &PathBuf) -> std::io::Result<()> {
    if !path.exists() {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

pub(crate) fn normalize_storage_root(value: &str, fallback: PathBuf) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string_lossy().to_string()
    } else {
        PathBuf::from(trimmed).to_string_lossy().to_string()
    }
}

pub(crate) fn normalize_file_storage_settings(
    mut settings: FileStorageSettings,
) -> FileStorageSettings {
    settings.instrumental_root = normalize_storage_root(
        &settings.instrumental_root,
        get_default_asset_root("instrumental"),
    );
    settings.vocals_root =
        normalize_storage_root(&settings.vocals_root, get_default_asset_root("vocals"));
    settings.lyrics_root =
        normalize_storage_root(&settings.lyrics_root, get_default_asset_root("lyrics"));
    settings.online_download_root = normalize_storage_root(
        &settings.online_download_root,
        get_default_online_download_root(),
    );
    settings
}
