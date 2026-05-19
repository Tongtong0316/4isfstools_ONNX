#![allow(dead_code)]

pub(crate) mod audio_io;
pub(crate) mod dsp;
pub(crate) mod engine;
pub(crate) mod legacy_demucs;
pub(crate) mod model_registry;
pub(crate) mod onnx_engine;

use std::path::Path;

use tauri::AppHandle;

use crate::models::SeparationEngineHealth;

pub(crate) fn detect_engine_health(app: &AppHandle, models_dir: &Path) -> SeparationEngineHealth {
    let registry = model_registry::ModelRegistry::from_models_dir(models_dir);
    onnx_engine::OnnxSeparationEngine::new(app.clone(), registry).health()
}
