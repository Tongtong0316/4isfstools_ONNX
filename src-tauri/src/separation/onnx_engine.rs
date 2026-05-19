use std::path::Path;
use std::process::{Command, Stdio};

use tauri::AppHandle;

use crate::models::SeparationEngineHealth;
use crate::runtime::python::get_python_path;

use super::engine::{ProviderStrategy, SeparationEngine, SeparationEngineKind};
use super::legacy_demucs;
use super::model_registry::{ModelRegistry, HIGH_QUALITY_ONNX_MODEL_ID};

fn run_onnx_probe_value(
    python_path: &Path,
    model_path: &Path,
    requested: &[String],
) -> serde_json::Value {
    let script = r#"
import json
import sys
from pathlib import Path

requested = json.loads(sys.argv[1])
model_path = Path(sys.argv[2])
payload = {
    "onnxruntimeAvailable": False,
    "availableProviders": ["unavailable"],
    "selectedProvider": "CPUExecutionProvider",
    "providerFallbackReason": None,
    "probeError": None,
    "modelReady": False,
    "inputShape": None,
    "outputShape": None,
}
try:
    import onnxruntime as ort
    payload["onnxruntimeAvailable"] = True
    try:
        available = list(ort.get_available_providers())
        payload["availableProviders"] = available or ["unavailable"]
    except Exception:
        payload["availableProviders"] = ["unavailable"]
    chosen = None
    for provider in requested:
        if provider in payload["availableProviders"]:
            chosen = provider
            break
    if chosen is None:
        chosen = "CPUExecutionProvider"
        payload["providerFallbackReason"] = f"provider_fallback_to_cpu:{requested[0] if requested else 'CPUExecutionProvider'}"
    else:
        payload["providerFallbackReason"] = None
    payload["selectedProvider"] = chosen
    if not model_path.exists():
        payload["probeError"] = f"model_missing:{model_path}"
        print(json.dumps(payload, ensure_ascii=False))
        raise SystemExit(0)
    try:
        session = ort.InferenceSession(str(model_path), providers=[chosen])
        payload["modelReady"] = True
        inputs = session.get_inputs()
        outputs = session.get_outputs()
        def shape_of(item):
            if not item:
                return None
            shape = []
            for dim in item[0].shape:
                if dim is None:
                    shape.append("dynamic")
                else:
                    shape.append(str(dim))
            return shape
        payload["inputShape"] = shape_of(inputs)
        payload["outputShape"] = shape_of(outputs)
    except Exception as exc:
        payload["probeError"] = str(exc)
    print(json.dumps(payload, ensure_ascii=False))
except Exception as exc:
    payload["probeError"] = str(exc)
    print(json.dumps(payload, ensure_ascii=False))
"#;

    let output = Command::new(python_path)
        .args([
            "-X",
            "utf8",
            "-c",
            script,
            &serde_json::to_string(requested).unwrap_or_else(|_| "[]".to_string()),
            &model_path.to_string_lossy(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            serde_json::from_str::<serde_json::Value>(&stdout).unwrap_or_else(|_| {
                serde_json::json!({
                    "probeError": format!("onnxruntime probe parse failed: {}", stdout),
                    "onnxruntimeAvailable": false,
                    "availableProviders": ["unavailable"],
                    "selectedProvider": "CPUExecutionProvider",
                })
            })
        }
        Err(err) => serde_json::json!({
            "probeError": format!("onnxruntime probe spawn failed: {}", err),
            "onnxruntimeAvailable": false,
            "availableProviders": ["unavailable"],
            "selectedProvider": "CPUExecutionProvider",
        }),
    }
}

fn run_onnx_probe(
    python_path: &Path,
    model_path: &Path,
    requested: &[String],
) -> crate::models::OnnxRuntimeProbeResult {
    let json = run_onnx_probe_value(python_path, model_path, requested);
    crate::models::OnnxRuntimeProbeResult {
        onnxruntime_available: json
            .get("onnxruntimeAvailable")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        available_providers: json
            .get("availableProviders")
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_else(|| vec!["unavailable".to_string()]),
        selected_provider: json
            .get("selectedProvider")
            .and_then(|v| v.as_str())
            .unwrap_or("CPUExecutionProvider")
            .to_string(),
        provider_fallback_reason: json
            .get("providerFallbackReason")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        probe_error: json
            .get("probeError")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    }
}

fn probe_model_metadata(
    python_path: &Path,
    model_path: &Path,
    requested: &[String],
) -> crate::models::OnnxModelProbeResult {
    let json = run_onnx_probe_value(python_path, model_path, requested);
    let input_shape = json
        .get("inputShape")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        });
    let output_shape = json
        .get("outputShape")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        });
    crate::models::OnnxModelProbeResult {
        model_path: model_path.to_string_lossy().to_string(),
        model_ready: json
            .get("onnxruntimeAvailable")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            && json.get("probeError").and_then(|v| v.as_str()).is_none()
            && model_path.exists(),
        input_shape,
        output_shape,
        probe_error: json
            .get("probeError")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                if !json
                    .get("onnxruntimeAvailable")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    Some("ONNX Runtime unavailable".to_string())
                } else if !model_path.exists() {
                    Some("model_missing".to_string())
                } else {
                    None
                }
            }),
    }
}

#[derive(Debug, Clone)]
pub(crate) struct OnnxSeparationEngine {
    app: AppHandle,
    registry: ModelRegistry,
    provider_strategy: ProviderStrategy,
}

impl OnnxSeparationEngine {
    pub(crate) fn new(app: AppHandle, registry: ModelRegistry) -> Self {
        Self {
            app,
            registry,
            provider_strategy: ProviderStrategy::for_current_platform(),
        }
    }

    pub(crate) fn health(&self) -> SeparationEngineHealth {
        let python_path = get_python_path(&self.app);
        let default_model_path = self.registry.default_model_path().unwrap_or_default();
        let high_quality_model_path = self.registry.high_quality_model_path().unwrap_or_default();
        let requested = self.provider_strategy.requested_provider_names();
        let runtime_probe = if python_path.exists() {
            run_onnx_probe(&python_path, &default_model_path, &requested)
        } else {
            crate::models::OnnxRuntimeProbeResult {
                probe_error: Some("python_runtime_missing".to_string()),
                ..Default::default()
            }
        };
        let default_model_probe =
            probe_model_metadata(&python_path, &default_model_path, &requested);
        let high_quality_model_probe =
            probe_model_metadata(&python_path, &high_quality_model_path, &requested);

        SeparationEngineHealth {
            active_engine: self.kind().as_str().to_string(),
            legacy_fallback_engine: legacy_demucs::LEGACY_ENGINE_ID.to_string(),
            requested_providers: requested.clone(),
            available_providers: runtime_probe.available_providers.clone(),
            selected_provider: runtime_probe.selected_provider.clone(),
            provider_fallback_reason: runtime_probe.provider_fallback_reason.clone(),
            default_model_id: self
                .registry
                .default_model()
                .map(|model| model.id.to_string())
                .unwrap_or_default(),
            default_model_path: default_model_path.to_string_lossy().to_string(),
            default_model_ready: default_model_probe.model_ready,
            default_model_input_shape: default_model_probe.input_shape.clone(),
            default_model_output_shape: default_model_probe.output_shape.clone(),
            high_quality_model_id: Some(HIGH_QUALITY_ONNX_MODEL_ID.to_string()),
            high_quality_model_path: high_quality_model_path.to_string_lossy().to_string(),
            high_quality_model_ready: high_quality_model_probe.model_ready,
            high_quality_model_input_shape: high_quality_model_probe.input_shape.clone(),
            high_quality_model_output_shape: high_quality_model_probe.output_shape.clone(),
            onnxruntime_available: runtime_probe.onnxruntime_available,
            legacy_demucs_available: false,
            probe_error: runtime_probe
                .probe_error
                .clone()
                .or(default_model_probe.probe_error.clone())
                .or(high_quality_model_probe.probe_error.clone()),
        }
    }
}

impl SeparationEngine for OnnxSeparationEngine {
    fn kind(&self) -> SeparationEngineKind {
        SeparationEngineKind::Onnx
    }

    fn provider_strategy(&self) -> ProviderStrategy {
        self.provider_strategy.clone()
    }
}
