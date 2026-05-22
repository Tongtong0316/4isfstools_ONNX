use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::AppHandle;

use crate::models::SeparationEngineHealth;
use crate::runtime::python::get_python_path;

use super::audio_io::normalize_source_audio;
use super::engine::{ProviderStrategy, SeparationEngine, SeparationEngineKind};
use super::model_registry::{ModelRegistry, HIGH_QUALITY_ONNX_MODEL_ID};

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct HiddenSeparationTuning {
    pub segment_size: u32,
    pub overlap_ratio: f32,
    pub vocals_first: bool,
}

fn hidden_separation_tuning(model_id: &str) -> HiddenSeparationTuning {
    match model_id {
        "high_quality" => HiddenSeparationTuning {
            segment_size: 256,
            overlap_ratio: 0.5,
            vocals_first: false,
        },
        _ => HiddenSeparationTuning {
            segment_size: 256,
            overlap_ratio: 0.5,
            vocals_first: true,
        },
    }
}

fn run_onnx_probe_value(
    python_path: &Path,
    model_path: &Path,
    requested: &[String],
) -> serde_json::Value {
    let script = r#"
import json
import sys
import time
from pathlib import Path

requested = json.loads(sys.argv[1])
model_path = Path(sys.argv[2])
payload = {
    "onnxruntimeAvailable": False,
    "availableProviders": ["unavailable"],
    "selectedProvider": "CPUExecutionProvider",
    "providerFallbackReason": None,
    "probeError": None,
    "modelExists": model_path.exists(),
    "modelReady": model_path.exists(),
    "sessionLoadOk": False,
    "sessionLoadError": None,
    "modelMetadataOk": False,
    "modelMetadataError": None,
    "inputShape": None,
    "outputShape": None,
    "dummyInferenceOk": None,
    "dummyInferenceError": None,
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
    try:
        if not model_path.exists():
            payload["probeError"] = f"model_missing:{model_path}"
            print(json.dumps(payload, ensure_ascii=False))
            raise SystemExit(0)
        session = ort.InferenceSession(str(model_path), providers=[chosen])
        payload["sessionLoadOk"] = True
        inputs = session.get_inputs()
        outputs = session.get_outputs()
        payload["modelMetadataOk"] = True

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

        safe_dummy = False
        if len(inputs) == 1:
            dtype_map = {
                "tensor(float)": "float32",
                "tensor(float16)": "float16",
                "tensor(double)": "float64",
                "tensor(int32)": "int32",
                "tensor(int64)": "int64",
            }
            input_type = getattr(inputs[0], "type", None)
            if input_type in dtype_map:
                input_shape = getattr(inputs[0], "shape", None) or []
                concrete_shape = []
                for dim in input_shape:
                    if isinstance(dim, int) and dim > 0:
                        concrete_shape.append(dim)
                    else:
                        concrete_shape = []
                        break
                if concrete_shape:
                    safe_dummy = True

        if safe_dummy:
            try:
                import numpy as np

                dtype_map = {
                    "tensor(float)": np.float32,
                    "tensor(float16)": np.float16,
                    "tensor(double)": np.float64,
                    "tensor(int32)": np.int32,
                    "tensor(int64)": np.int64,
                }
                input_def = inputs[0]
                array = np.zeros(tuple(int(dim) for dim in input_def.shape), dtype=dtype_map[input_def.type])
                feed = {input_def.name: array}
                _ = session.run(None, feed)
                payload["dummyInferenceOk"] = True
            except Exception as exc:
                payload["dummyInferenceOk"] = False
                payload["dummyInferenceError"] = str(exc)
                payload["probeError"] = f"dummy_inference_failed:{exc}"
        else:
            payload["dummyInferenceOk"] = False
            payload["dummyInferenceError"] = "dummy_inference_unsupported:dynamic_or_multi_input_or_unsupported_dtype"
    except Exception as exc:
        payload["sessionLoadOk"] = False
        payload["sessionLoadError"] = str(exc)
        payload["probeError"] = f"session_load_failed:{exc}"
    print(json.dumps(payload, ensure_ascii=False))
except Exception as exc:
    payload["probeError"] = str(exc)
    print(json.dumps(payload, ensure_ascii=False))
"#;

    let mut command = Command::new(python_path);
    command
        .args([
            "-X",
            "utf8",
            "-c",
            script,
            &serde_json::to_string(requested).unwrap_or_else(|_| "[]".to_string()),
            &model_path.to_string_lossy(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::process_control::configure_console_visibility(&mut command);
    let output = command.output();

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
    runtime_probe_from_json(&json)
}

fn runtime_probe_from_json(json: &serde_json::Value) -> crate::models::OnnxRuntimeProbeResult {
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
        session_load_ok: json
            .get("sessionLoadOk")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        session_load_error: json
            .get("sessionLoadError")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        model_metadata_ok: json
            .get("modelMetadataOk")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        model_metadata_error: json
            .get("modelMetadataError")
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
    model_probe_from_json(&json, model_path)
}

fn model_probe_from_json(
    json: &serde_json::Value,
    model_path: &Path,
) -> crate::models::OnnxModelProbeResult {
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
            .get("modelExists")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| model_path.exists()),
        session_load_ok: json
            .get("sessionLoadOk")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        session_load_error: json
            .get("sessionLoadError")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        model_metadata_ok: json
            .get("modelMetadataOk")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        model_metadata_error: json
            .get("modelMetadataError")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        input_shape,
        output_shape,
        dummy_inference_ok: json.get("dummyInferenceOk").and_then(|v| v.as_bool()),
        dummy_inference_error: json
            .get("dummyInferenceError")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
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

pub(crate) fn run_onnx_separation(
    app: &AppHandle,
    song_id: &str,
    input_path: &Path,
    output_dir: &Path,
    model_path: &Path,
    requested: &[String],
    selected_provider: &str,
    model_id: &str,
) -> Result<serde_json::Value, String> {
    let python_path = get_python_path(app);
    if !python_path.exists() {
        return Err("Python runtime not found".to_string());
    }

    fs::create_dir_all(output_dir)
        .map_err(|e| format!("Failed to create output directory {:?}: {}", output_dir, e))?;

    let debug_dir = output_dir.join("debug");
    fs::create_dir_all(&debug_dir)
        .map_err(|e| format!("Failed to create debug directory {:?}: {}", debug_dir, e))?;

    let normalized_input_path = debug_dir.join("normalized_input.wav");
    normalize_source_audio(input_path, &normalized_input_path)?;

    let vocals_output_path = output_dir.join("vocals.wav");
    let instrumental_output_path = output_dir.join("instrumental.wav");
    let provider = if matches!(
        selected_provider,
        "CoreMLExecutionProvider" | "DmlExecutionProvider" | "CPUExecutionProvider"
    ) {
        selected_provider
    } else {
        requested
            .iter()
            .find(|provider| {
                matches!(
                    provider.as_str(),
                    "CoreMLExecutionProvider" | "DmlExecutionProvider" | "CPUExecutionProvider"
                )
            })
            .map(|provider| provider.as_str())
            .unwrap_or("CPUExecutionProvider")
    };
    let hidden_tuning = hidden_separation_tuning(model_id);

    let script = r#"
import json
import sys
import time
from pathlib import Path

import numpy as np
import soundfile as sf
import onnxruntime as ort

payload = {
    "success": False,
    "error": None,
    "error_code": None,
    "modelTuning": None,
    "onnxruntimeAvailable": False,
    "availableProviders": ["unavailable"],
    "requestedProviders": [],
    "selectedProvider": "CPUExecutionProvider",
    "providerFallbackReason": None,
    "modelPath": None,
    "inputPath": None,
    "vocalsPath": None,
    "instrumentalPath": None,
    "sampleRate": None,
    "segmentCount": 0,
    "outputSampleCount": None,
    "timingsMs": {},
}

def write_payload():
    print(json.dumps(payload, ensure_ascii=False))

def provider_to_sherpa(provider):
    return {
        "CoreMLExecutionProvider": "coreml",
        "DmlExecutionProvider": "dml",
        "CPUExecutionProvider": "cpu",
    }.get(provider, "cpu")

def coreml_provider_options():
    return {
        "MLComputeUnits": "CPUAndGPU",
        "ModelFormat": "NeuralNetwork",
        "RequireStaticInputShapes": "0",
        "EnableOnSubgraphs": "0",
    }

class MDXSTFT:
    def __init__(self, n_fft, hop_length, dim_f):
        self.n_fft = n_fft
        self.hop_length = hop_length
        self.dim_f = dim_f
        self.window = torch.hann_window(window_length=self.n_fft, periodic=True)

    def __call__(self, input_tensor):
        batch_dimensions = input_tensor.shape[:-2]
        channel_dim, time_dim = input_tensor.shape[-2:]
        reshaped_tensor = input_tensor.reshape([-1, time_dim])
        stft_window = self.window.to(input_tensor.device)
        stft_output = torch.stft(
            reshaped_tensor,
            n_fft=self.n_fft,
            hop_length=self.hop_length,
            window=stft_window,
            center=True,
            return_complex=False,
        )
        permuted_stft_output = stft_output.permute([0, 3, 1, 2])
        final_output = permuted_stft_output.reshape(
            [*batch_dimensions, channel_dim, 2, -1, permuted_stft_output.shape[-1]]
        ).reshape([*batch_dimensions, channel_dim * 2, -1, permuted_stft_output.shape[-1]])
        return final_output[..., : self.dim_f, :]

    def inverse(self, input_tensor):
        batch_dimensions = input_tensor.shape[:-3]
        channel_dim, freq_dim, time_dim = input_tensor.shape[-3:]
        num_freq_bins = self.n_fft // 2 + 1
        if freq_dim < num_freq_bins:
            freq_padding = torch.zeros(
                [*batch_dimensions, channel_dim, num_freq_bins - freq_dim, time_dim],
                device=input_tensor.device,
            )
            input_tensor = torch.cat([input_tensor, freq_padding], -2)
        reshaped_tensor = input_tensor.reshape([*batch_dimensions, channel_dim // 2, 2, num_freq_bins, time_dim])
        flattened_tensor = reshaped_tensor.reshape([-1, 2, num_freq_bins, time_dim])
        permuted_tensor = flattened_tensor.permute([0, 2, 3, 1])
        complex_tensor = permuted_tensor[..., 0] + permuted_tensor[..., 1] * 1.0j
        stft_window = self.window.to(input_tensor.device)
        istft_result = torch.istft(
            complex_tensor,
            n_fft=self.n_fft,
            hop_length=self.hop_length,
            window=stft_window,
            center=True,
        )
        return istft_result.reshape([*batch_dimensions, 2, -1])

class NumpyMDXSTFT:
    def __init__(self, n_fft, hop_length, dim_f):
        self.n_fft = n_fft
        self.hop_length = hop_length
        self.dim_f = dim_f
        self.trim = n_fft // 2
        # Match torch.hann_window(periodic=True) closely without requiring torch.
        self.window = np.hanning(n_fft + 1).astype(np.float32)[:-1]

    def __call__(self, input_array):
        batch_size, channels, time_dim = input_array.shape
        padded = np.pad(
            input_array,
            ((0, 0), (0, 0), (self.trim, self.trim)),
            mode="reflect" if time_dim > 1 else "constant",
        )
        frame_count = 1 + max(0, (padded.shape[-1] - self.n_fft) // self.hop_length)
        output = np.empty((batch_size, channels * 2, self.dim_f, frame_count), dtype=np.float32)
        for batch_index in range(batch_size):
            for channel_index in range(channels):
                frames = np.empty((self.n_fft, frame_count), dtype=np.float32)
                for frame_index in range(frame_count):
                    start = frame_index * self.hop_length
                    frames[:, frame_index] = (
                        padded[batch_index, channel_index, start:start + self.n_fft] * self.window
                    )
                spectrum = np.fft.rfft(frames, axis=0)[: self.dim_f]
                base = channel_index * 2
                output[batch_index, base] = spectrum.real.astype(np.float32, copy=False)
                output[batch_index, base + 1] = spectrum.imag.astype(np.float32, copy=False)
        return output

    def inverse(self, input_array):
        batch_size, channel_dim, freq_dim, frame_count = input_array.shape
        channels = channel_dim // 2
        num_freq_bins = self.n_fft // 2 + 1
        padded_length = self.n_fft + self.hop_length * max(0, frame_count - 1)
        output = np.zeros((batch_size, channels, padded_length), dtype=np.float32)
        divider = np.zeros(padded_length, dtype=np.float32)
        window_square = self.window * self.window
        for frame_index in range(frame_count):
            start = frame_index * self.hop_length
            divider[start:start + self.n_fft] += window_square
        safe_divider = np.where(divider > 1e-8, divider, 1.0)

        for batch_index in range(batch_size):
            for channel_index in range(channels):
                base = channel_index * 2
                complex_spec = np.zeros((num_freq_bins, frame_count), dtype=np.complex64)
                complex_spec[:freq_dim] = (
                    input_array[batch_index, base].astype(np.float32)
                    + 1j * input_array[batch_index, base + 1].astype(np.float32)
                )
                for frame_index in range(frame_count):
                    start = frame_index * self.hop_length
                    frame = np.fft.irfft(complex_spec[:, frame_index], n=self.n_fft).astype(np.float32)
                    output[batch_index, channel_index, start:start + self.n_fft] += frame * self.window
                output[batch_index, channel_index] /= safe_divider

        if output.shape[-1] > self.trim * 2:
            output = output[:, :, self.trim:-self.trim]
        return output

def make_session(model_path, provider):
    available = list(ort.get_available_providers())
    payload["availableProviders"] = available or ["unavailable"]
    chosen = provider if provider in available else "CPUExecutionProvider"
    if chosen not in available:
        chosen = available[0] if available else "CPUExecutionProvider"
    if chosen != provider:
        payload["providerFallbackReason"] = f"provider_fallback:{provider}->{chosen}"
    payload["selectedProvider"] = chosen
    return ort.InferenceSession(str(model_path), providers=[chosen])

def make_direct_session(model_path, provider):
    available = list(ort.get_available_providers())
    payload["availableProviders"] = available or ["unavailable"]
    provider_specs = []
    chosen = provider if provider in available else "CPUExecutionProvider"
    if chosen != provider:
        payload["providerFallbackReason"] = f"provider_fallback:{provider}->{chosen}"
    if chosen == "CoreMLExecutionProvider":
        provider_specs.append(("CoreMLExecutionProvider", coreml_provider_options()))
        provider_specs.append("CPUExecutionProvider")
    elif chosen == "DmlExecutionProvider":
        provider_specs.append("DmlExecutionProvider")
        provider_specs.append("CPUExecutionProvider")
    else:
        chosen = "CPUExecutionProvider"
        provider_specs.append("CPUExecutionProvider")
    try:
        session = ort.InferenceSession(str(model_path), providers=provider_specs)
        payload["selectedProvider"] = session.get_providers()[0] if session.get_providers() else chosen
        return session
    except Exception as exc:
        if chosen != "CPUExecutionProvider":
            payload["providerFallbackReason"] = f"provider_exec_failed:{provider}->{chosen}: {exc}"
            session = ort.InferenceSession(str(model_path), providers=["CPUExecutionProvider"])
            payload["selectedProvider"] = "CPUExecutionProvider"
            return session
        raise

def separate_hq5_mdx(model_path, input_path, vocals_path, instrumental_path, provider):
    import torch as _torch

    globals()["torch"] = _torch

    started_at = time.perf_counter()
    payload["modelBranch"] = "hq5_direct_mdx"
    samples, sample_rate = sf.read(str(input_path), dtype="float32", always_2d=True)
    audio_loaded_at = time.perf_counter()
    samples = np.transpose(samples)
    original_sample_count = int(samples.shape[1])
    if samples.shape[0] == 1:
        samples = np.repeat(samples, 2, axis=0)
    if samples.shape[0] != 2:
        payload["error_code"] = "ONNX_AUDIO_PREP_FAILED"
        payload["error"] = f"expected_stereo_input:{samples.shape}"
        return

    segment_size = int((model_cfg or {}).get("segment_size") or 512)
    overlap_ratio = float((model_cfg or {}).get("overlap_ratio") or 0.25)
    compensate = 1.010
    hop_length = 1024
    n_fft = 5120
    dim_f = 2560
    trim = n_fft // 2
    batch_size = 1
    chunk_size = max(hop_length * (segment_size - 1), hop_length)
    gen_size = chunk_size - 2 * trim
    if gen_size <= 0:
        payload["error_code"] = "ONNX_AUDIO_PREP_FAILED"
        payload["error"] = f"invalid_gen_size:{gen_size}"
        return

    pad = gen_size + trim - (samples.shape[1] % gen_size)
    mixture = np.concatenate(
        (
            np.zeros((2, trim), dtype="float32"),
            samples,
            np.zeros((2, pad), dtype="float32"),
            np.zeros((2, trim), dtype="float32"),
        ),
        axis=1,
    )

    step = max(1, int((1 - overlap_ratio) * chunk_size))
    total_segments = max(1, (mixture.shape[-1] + step - 1) // step)
    stft = MDXSTFT(n_fft=n_fft, hop_length=hop_length, dim_f=dim_f)
    session = make_direct_session(model_path, provider)
    session_ready_at = time.perf_counter()
    input_name = session.get_inputs()[0].name
    output_name = session.get_outputs()[0].name
    progress_path = vocals_path.parent / "separator_progress.json"
    result = np.zeros((1, 2, mixture.shape[-1]), dtype=np.float32)
    divider = np.zeros((1, 2, mixture.shape[-1]), dtype=np.float32)
    segment_count = 0
    pending_chunks = []
    pending_meta = []

    def flush_pending():
        nonlocal segment_count
        if not pending_chunks:
            return
        batch_tensor = torch.from_numpy(np.stack(pending_chunks, axis=0)).to(torch.float32)
        spek = stft(batch_tensor)
        spek[:, :, :3, :] *= 0
        spec_pred = session.run([output_name], {input_name: spek.cpu().numpy()})[0]
        tar_waves = stft.inverse(torch.tensor(spec_pred, dtype=torch.float32)).cpu().numpy()
        for index, meta in enumerate(pending_meta):
            start = meta["start"]
            chunk_len = meta["chunk_len"]
            target = tar_waves[index, :, :chunk_len].astype(np.float32, copy=False)
            if overlap_ratio != 0:
                window = np.hanning(chunk_len).astype(np.float32)
                target *= window[np.newaxis, :]
                divider[0, :, start:start + chunk_len] += window[np.newaxis, :]
            else:
                divider[0, :, start:start + chunk_len] += 1
            result[0, :, start:start + chunk_len] += target
            segment_count += 1
        pending_chunks.clear()
        pending_meta.clear()

    for start in range(0, mixture.shape[-1], step):
        end = min(start + chunk_size, mixture.shape[-1])
        if end <= start:
            break
        chunk = mixture[:, start:end]
        chunk_len = chunk.shape[1]
        if chunk_len <= 0:
            continue
        if end != start + chunk_size:
            pad_size = (start + chunk_size) - end
            chunk = np.concatenate((chunk, np.zeros((2, pad_size), dtype="float32")), axis=-1)
        pending_chunks.append(chunk)
        pending_meta.append({"start": start, "chunk_len": chunk_len})
        if len(pending_chunks) >= batch_size:
            flush_pending()
        if progress_path:
            pct = min(int(segment_count / total_segments * 74) + 18, 92)
            with open(progress_path, 'w') as pf:
                json.dump({"percent": pct, "message": f"分离中... ({segment_count}/{total_segments})"}, pf)

    flush_pending()
    infer_done_at = time.perf_counter()

    if segment_count == 0:
        payload["error_code"] = "ONNX_SEPARATION_FAILED"
        payload["error"] = "no_segments_processed"
        return

    safe_divider = np.where(divider > 1e-8, divider, 1.0)
    primary_source = (result / safe_divider)[:, :, trim:-trim]
    primary_source = primary_source[:, :, :original_sample_count]
    primary_source = np.transpose(primary_source[0], (1, 0))
    primary_source = primary_source.astype("float32")
    secondary_source = (np.transpose(samples, (1, 0)) - (primary_source * compensate)).astype("float32")
    sf.write(str(instrumental_path), primary_source, samplerate=sample_rate)
    sf.write(str(vocals_path), secondary_source, samplerate=sample_rate)
    write_done_at = time.perf_counter()
    payload["sampleRate"] = int(sample_rate)
    payload["segmentCount"] = int(segment_count)
    payload["outputSampleCount"] = int(primary_source.shape[0])
    payload["timingsMs"] = {
        "audioLoadMs": round((audio_loaded_at - started_at) * 1000, 2),
        "sessionInitMs": round((session_ready_at - audio_loaded_at) * 1000, 2),
        "inferMs": round((infer_done_at - session_ready_at) * 1000, 2),
        "writeMs": round((write_done_at - infer_done_at) * 1000, 2),
        "totalMs": round((write_done_at - started_at) * 1000, 2),
    }

def separate_default_mdx_direct(model_path, input_path, vocals_path, instrumental_path, provider):
    started_at = time.perf_counter()
    payload["modelBranch"] = "default_direct_mdx"
    samples, sample_rate = sf.read(str(input_path), dtype="float32", always_2d=True)
    audio_loaded_at = time.perf_counter()
    samples = np.transpose(samples)
    original_sample_count = int(samples.shape[1])
    if samples.shape[0] == 1:
        samples = np.repeat(samples, 2, axis=0)
    if samples.shape[0] != 2:
        payload["error_code"] = "ONNX_AUDIO_PREP_FAILED"
        payload["error"] = f"expected_stereo_input:{samples.shape}"
        return

    segment_size = int((model_cfg or {}).get("segment_size") or 256)
    overlap_ratio = float((model_cfg or {}).get("overlap_ratio") or 0.5)
    vocals_first = bool((model_cfg or {}).get("vocals_first", True))
    compensate = 1.0
    hop_length = 1024
    n_fft = 4096
    dim_f = 2048
    trim = n_fft // 2
    chunk_size = max(hop_length * (segment_size - 1), hop_length)
    gen_size = chunk_size - 2 * trim
    if gen_size <= 0:
        payload["error_code"] = "ONNX_AUDIO_PREP_FAILED"
        payload["error"] = f"invalid_gen_size:{gen_size}"
        return

    pad = gen_size + trim - (samples.shape[1] % gen_size)
    mixture = np.concatenate(
        (
            np.zeros((2, trim), dtype="float32"),
            samples,
            np.zeros((2, pad), dtype="float32"),
            np.zeros((2, trim), dtype="float32"),
        ),
        axis=1,
    )

    step = max(1, int((1 - overlap_ratio) * chunk_size))
    total_segments = max(1, (mixture.shape[-1] + step - 1) // step)
    stft = NumpyMDXSTFT(n_fft=n_fft, hop_length=hop_length, dim_f=dim_f)
    session = make_direct_session(model_path, provider)
    session_ready_at = time.perf_counter()
    input_name = session.get_inputs()[0].name
    output_name = session.get_outputs()[0].name
    progress_path = vocals_path.parent / "separator_progress.json"
    result = np.zeros((1, 2, mixture.shape[-1]), dtype=np.float32)
    divider = np.zeros((1, 2, mixture.shape[-1]), dtype=np.float32)
    segment_count = 0

    for start in range(0, mixture.shape[-1], step):
        end = min(start + chunk_size, mixture.shape[-1])
        if end <= start:
            break
        chunk = mixture[:, start:end]
        chunk_len = chunk.shape[1]
        if chunk_len <= 0:
            continue
        if end != start + chunk_size:
            pad_size = (start + chunk_size) - end
            chunk = np.concatenate((chunk, np.zeros((2, pad_size), dtype="float32")), axis=-1)
        spek = stft(chunk[np.newaxis, :, :].astype(np.float32, copy=False))
        spek[:, :, :3, :] *= 0
        spec_pred = session.run([output_name], {input_name: spek})[0]
        target = stft.inverse(spec_pred.astype(np.float32, copy=False))[0, :, :chunk_len]
        if overlap_ratio != 0:
            window = np.hanning(chunk_len).astype(np.float32)
            target *= window[np.newaxis, :]
            divider[0, :, start:start + chunk_len] += window[np.newaxis, :]
        else:
            divider[0, :, start:start + chunk_len] += 1
        result[0, :, start:start + chunk_len] += target
        segment_count += 1
        if progress_path:
            pct = min(int(segment_count / total_segments * 74) + 18, 92)
            with open(progress_path, 'w') as pf:
                json.dump({"percent": pct, "message": f"分离中... ({segment_count}/{total_segments})"}, pf)

    infer_done_at = time.perf_counter()
    if segment_count == 0:
        payload["error_code"] = "ONNX_SEPARATION_FAILED"
        payload["error"] = "no_segments_processed"
        return

    safe_divider = np.where(divider > 1e-8, divider, 1.0)
    primary_source = (result / safe_divider)[:, :, trim:-trim]
    primary_source = primary_source[:, :, :original_sample_count]
    primary_source = np.transpose(primary_source[0], (1, 0)).astype("float32")
    secondary_source = (np.transpose(samples, (1, 0)) - (primary_source * compensate)).astype("float32")
    if vocals_first:
        sf.write(str(vocals_path), primary_source, samplerate=sample_rate)
        sf.write(str(instrumental_path), secondary_source, samplerate=sample_rate)
    else:
        sf.write(str(instrumental_path), primary_source, samplerate=sample_rate)
        sf.write(str(vocals_path), secondary_source, samplerate=sample_rate)
    write_done_at = time.perf_counter()
    payload["sampleRate"] = int(sample_rate)
    payload["segmentCount"] = int(segment_count)
    payload["outputSampleCount"] = int(primary_source.shape[0])
    payload["timingsMs"] = {
        "audioLoadMs": round((audio_loaded_at - started_at) * 1000, 2),
        "sessionInitMs": round((session_ready_at - audio_loaded_at) * 1000, 2),
        "inferMs": round((infer_done_at - session_ready_at) * 1000, 2),
        "writeMs": round((write_done_at - infer_done_at) * 1000, 2),
        "totalMs": round((write_done_at - started_at) * 1000, 2),
    }

def separate_via_sherpa(model_path, input_path, vocals_path, instrumental_path, provider):
    import sherpa_onnx

    try:
        started_at = time.perf_counter()
        payload["modelBranch"] = "sherpa_uvr"
        config = sherpa_onnx.OfflineSourceSeparationConfig(
            model=sherpa_onnx.OfflineSourceSeparationModelConfig(
                uvr=sherpa_onnx.OfflineSourceSeparationUvrModelConfig(model=str(model_path)),
                num_threads=1,
                debug=False,
                provider=provider_to_sherpa(provider),
            )
        )
        if not config.validate():
            payload["error_code"] = "ONNX_MODEL_METADATA_FAILED"
            payload["error"] = "invalid_offline_source_separation_config"
            return

        config_ready_at = time.perf_counter()
        sp = sherpa_onnx.OfflineSourceSeparation(config)
        samples, sample_rate = sf.read(str(input_path), dtype="float32", always_2d=True)
        audio_loaded_at = time.perf_counter()
        samples = np.transpose(samples)
        if samples.shape[0] == 1:
            samples = np.repeat(samples, 2, axis=0)
        if samples.shape[0] != 2:
            payload["error_code"] = "ONNX_AUDIO_PREP_FAILED"
            payload["error"] = f"expected_stereo_input:{samples.shape}"
            return
        original_sample_count = int(samples.shape[1])

        segment_size = int((model_cfg or {}).get("segment_size") or 0)
        overlap_ratio = float((model_cfg or {}).get("overlap_ratio") or 0.0)
        vocals_first = bool((model_cfg or {}).get("vocals_first", True))
        if segment_size <= 0:
            output = sp.process(sample_rate=sample_rate, samples=np.ascontiguousarray(samples))
            process_done_at = time.perf_counter()
            payload["sampleRate"] = int(output.sample_rate)
            if len(output.stems) != 2:
                payload["error_code"] = "ONNX_SEPARATION_FAILED"
                payload["error"] = f"unexpected_stem_count:{len(output.stems)}"
                return

            if vocals_first:
                vocals = np.transpose(output.stems[0].data)
                non_vocals = np.transpose(output.stems[1].data)
            else:
                vocals = np.transpose(output.stems[1].data)
                non_vocals = np.transpose(output.stems[0].data)
            vocals = vocals[:original_sample_count]
            non_vocals = non_vocals[:original_sample_count]
            sf.write(str(vocals_path), vocals, samplerate=output.sample_rate)
            sf.write(str(instrumental_path), non_vocals, samplerate=output.sample_rate)
            write_done_at = time.perf_counter()

            payload["segmentCount"] = 1
            payload["outputSampleCount"] = int(vocals.shape[0])
            payload["timingsMs"] = {
                "configMs": round((config_ready_at - started_at) * 1000, 2),
                "audioLoadMs": round((audio_loaded_at - config_ready_at) * 1000, 2),
                "processMs": round((process_done_at - audio_loaded_at) * 1000, 2),
                "writeMs": round((write_done_at - process_done_at) * 1000, 2),
                "totalMs": round((write_done_at - started_at) * 1000, 2),
            }
            return

        # Hidden tuning: segment_size is interpreted as an internal frame count,
        # and we translate it to a sample window using the model hop length.
        hop_length = 1024
        segment_samples = max(hop_length * segment_size, hop_length)
        overlap_samples = int(segment_samples * max(0.0, min(overlap_ratio, 0.9)))
        if overlap_samples >= segment_samples:
            overlap_samples = segment_samples // 4
        step_samples = max(1, segment_samples - overlap_samples)
        total_samples = samples.shape[1]
        if total_samples <= 0:
            payload["error_code"] = "ONNX_AUDIO_PREP_FAILED"
            payload["error"] = "empty_audio"
            return

        def chunk_weight(length, fade_in, fade_out):
            weight = np.ones(length, dtype=np.float32)
            if fade_in > 0:
                weight[:fade_in] *= np.linspace(0.0, 1.0, fade_in, endpoint=False, dtype=np.float32)
            if fade_out > 0:
                weight[-fade_out:] *= np.linspace(1.0, 0.0, fade_out, endpoint=False, dtype=np.float32)
            return weight

        vocals_acc = np.zeros((2, total_samples), dtype=np.float32)
        non_vocals_acc = np.zeros((2, total_samples), dtype=np.float32)
        weight_acc = np.zeros(total_samples, dtype=np.float32)
        segment_count = 0
        total_segments = max(1, (total_samples + step_samples - 1) // step_samples)
        progress_path = vocals_path.parent / "separator_progress.json"

        for start in range(0, total_samples, step_samples):
            end = min(total_samples, start + segment_samples)
            if end <= start:
                break
            segment = samples[:, start:end]
            segment_len = segment.shape[1]
            if segment_len <= 0:
                continue
            output = sp.process(sample_rate=sample_rate, samples=np.ascontiguousarray(segment))
            if len(output.stems) != 2:
                payload["error_code"] = "ONNX_SEPARATION_FAILED"
                payload["error"] = f"unexpected_stem_count:{len(output.stems)}"
                return

            if vocals_first:
                vocals_chunk = np.transpose(output.stems[0].data)
                non_vocals_chunk = np.transpose(output.stems[1].data)
            else:
                vocals_chunk = np.transpose(output.stems[1].data)
                non_vocals_chunk = np.transpose(output.stems[0].data)
            chunk_len = min(segment_len, vocals_chunk.shape[0], non_vocals_chunk.shape[0])
            if chunk_len <= 0:
                continue

            fade_in = overlap_samples // 2 if start > 0 else 0
            fade_out = overlap_samples // 2 if end < total_samples else 0
            fade_in = min(fade_in, chunk_len)
            fade_out = min(fade_out, max(0, chunk_len - fade_in))
            weight = chunk_weight(chunk_len, fade_in, fade_out)

            vocals_acc[:, start:start + chunk_len] += (
                vocals_chunk[:chunk_len].T * weight[np.newaxis, :]
            )
            non_vocals_acc[:, start:start + chunk_len] += (
                non_vocals_chunk[:chunk_len].T * weight[np.newaxis, :]
            )
            weight_acc[start:start + chunk_len] += weight
            segment_count += 1
            if progress_path:
                pct = min(int(segment_count / total_segments * 74) + 18, 92)
                with open(progress_path, 'w') as pf:
                    json.dump({"percent": pct, "message": f"分离中... ({segment_count}/{total_segments})"}, pf)

        if segment_count == 0:
            payload["error_code"] = "ONNX_SEPARATION_FAILED"
            payload["error"] = "no_segments_processed"
            return

        safe_weight = np.where(weight_acc > 1e-8, weight_acc, 1.0)
        vocals = (vocals_acc / safe_weight[np.newaxis, :]).T
        non_vocals = (non_vocals_acc / safe_weight[np.newaxis, :]).T
        vocals = vocals[:total_samples]
        non_vocals = non_vocals[:total_samples]
        process_done_at = time.perf_counter()
        sf.write(str(vocals_path), vocals, samplerate=sample_rate)
        sf.write(str(instrumental_path), non_vocals, samplerate=sample_rate)
        write_done_at = time.perf_counter()

        payload["sampleRate"] = int(sample_rate)
        payload["segmentCount"] = int(segment_count)
        payload["outputSampleCount"] = int(vocals.shape[0])
        payload["timingsMs"] = {
            "configMs": round((config_ready_at - started_at) * 1000, 2),
            "audioLoadMs": round((audio_loaded_at - config_ready_at) * 1000, 2),
            "processMs": round((process_done_at - audio_loaded_at) * 1000, 2),
            "writeMs": round((write_done_at - process_done_at) * 1000, 2),
            "totalMs": round((write_done_at - started_at) * 1000, 2),
        }
    except Exception as exc:
        payload["error_code"] = "ONNX_SEPARATION_FAILED"
        payload["error"] = str(exc)
        raise

try:
    requested = json.loads(sys.argv[1])
    model_path = Path(sys.argv[2])
    input_path = Path(sys.argv[3])
    vocals_path = Path(sys.argv[4])
    instrumental_path = Path(sys.argv[5])
    provider = sys.argv[6]
    model_cfg = json.loads(sys.argv[7]) if len(sys.argv) > 7 else {}
    model_id = sys.argv[8] if len(sys.argv) > 8 else ""

    payload["requestedProviders"] = requested
    payload["modelPath"] = str(model_path)
    payload["inputPath"] = str(input_path)
    payload["vocalsPath"] = str(vocals_path)
    payload["instrumentalPath"] = str(instrumental_path)
    payload["selectedProvider"] = provider
    payload["modelTuning"] = model_cfg
    payload["modelId"] = model_id

    if not model_path.is_file():
        payload["error_code"] = "ONNX_ENGINE_NOT_READY"
        payload["error"] = f"model_missing:{model_path}"
        write_payload()
        raise SystemExit(0)

    if requested and provider == "CPUExecutionProvider" and requested[0] != "CPUExecutionProvider":
        payload["providerFallbackReason"] = f"provider_fallback_to_cpu:{requested[0]}"

    payload["onnxruntimeAvailable"] = True

    def separate_default(provider_name):
        if provider_name == "DmlExecutionProvider":
            separate_default_mdx_direct(
                model_path,
                input_path,
                vocals_path,
                instrumental_path,
                provider_name,
            )
        else:
            separate_via_sherpa(
                model_path,
                input_path,
                vocals_path,
                instrumental_path,
                provider_name,
            )

    try:
        if model_id == "high_quality":
            separate_hq5_mdx(model_path, input_path, vocals_path, instrumental_path, provider)
        else:
            separate_default(provider)
    except Exception as first_exc:
        if provider != "CPUExecutionProvider":
            fallback_provider = "CPUExecutionProvider"
            fallback_reason = f"provider_exec_failed:{provider}->{fallback_provider}:{first_exc}"
            payload["providerFallbackReason"] = fallback_reason
            payload["selectedProvider"] = fallback_provider
            payload["error_code"] = None
            payload["error"] = None
            for path in (vocals_path, instrumental_path):
                try:
                    if path.exists():
                        path.unlink()
                except Exception:
                    pass
            try:
                if model_id == "high_quality":
                    separate_hq5_mdx(
                        model_path,
                        input_path,
                        vocals_path,
                        instrumental_path,
                        fallback_provider,
                    )
                else:
                    separate_default(fallback_provider)
            except Exception as second_exc:
                payload["error_code"] = payload["error_code"] or "ONNX_SEPARATION_FAILED"
                payload["error"] = str(second_exc)
                write_payload()
                raise SystemExit(0)
        else:
            payload["error_code"] = payload["error_code"] or "ONNX_SEPARATION_FAILED"
            payload["error"] = str(first_exc)
            write_payload()
            raise SystemExit(0)

    if payload["error_code"] is not None:
        write_payload()
        raise SystemExit(0)

    payload["success"] = True
    payload["error_code"] = None
    payload["error"] = None
    write_payload()
except Exception as exc:
    if payload["error_code"] is None:
        payload["error_code"] = "ONNX_SEPARATION_FAILED"
        payload["error"] = str(exc)
    write_payload()
"#;

    let mut command = Command::new(python_path);
    command
        .args([
            "-X",
            "utf8",
            "-c",
            script,
            &serde_json::to_string(requested).unwrap_or_else(|_| "[]".to_string()),
            &model_path.to_string_lossy(),
            &normalized_input_path.to_string_lossy(),
            &vocals_output_path.to_string_lossy(),
            &instrumental_output_path.to_string_lossy(),
            provider,
            &serde_json::to_string(&hidden_tuning).unwrap_or_else(|_| "{}".to_string()),
            model_id,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::process_control::configure_console_visibility(&mut command);
    let child = crate::spawn_in_own_process_group(&mut command)
        .map_err(|e| format!("onnx separation spawn failed: {}", e))?;
    crate::register_separator_job(song_id, child.id());

    let progress_file = output_dir.join("separator_progress.json");
    let done = Arc::new(AtomicBool::new(false));
    let done_clone = done.clone();
    let monitor_app = app.clone();
    let monitor_song_id = song_id.to_string();
    let monitor_progress = progress_file.clone();

    let monitor = std::thread::spawn(move || {
        let mut last_pct = 0u32;
        while !done_clone.load(Ordering::Relaxed) {
            if let Ok(content) = fs::read_to_string(&monitor_progress) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(pct) = val["percent"].as_u64() {
                        let pct = pct as u32;
                        if pct != last_pct {
                            last_pct = pct;
                            let msg = val["message"].as_str().unwrap_or("分离中...");
                            crate::emit_progress(
                                &monitor_app,
                                &monitor_song_id,
                                "separating",
                                pct,
                                msg,
                                None,
                            );
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    });

    let output = child
        .wait_with_output()
        .map_err(|e| format!("onnx separation wait failed: {}", e))?;
    done.store(true, Ordering::Relaxed);
    let _ = monitor.join();
    crate::clear_separator_job(song_id);

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let mut json = serde_json::from_str::<serde_json::Value>(&stdout).unwrap_or_else(|_| {
        serde_json::json!({
            "success": false,
            "error_code": "ONNX_SEPARATION_FAILED",
            "error": format!("onnx separation parse failed: [stdout]{} [stderr]{}", stdout, stderr),
            "selectedProvider": provider,
        })
    });
    if !output.status.success()
        && json
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    {
        json = serde_json::json!({
            "success": false,
            "error_code": "ONNX_SEPARATION_FAILED",
            "error": if stderr.is_empty() { "onnx separation failed".to_string() } else { stderr },
            "selectedProvider": provider,
        });
    }
    Ok(json)
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
        let (runtime_probe, default_model_probe) = if python_path.exists() {
            let default_probe_json =
                run_onnx_probe_value(&python_path, &default_model_path, &requested);
            (
                runtime_probe_from_json(&default_probe_json),
                model_probe_from_json(&default_probe_json, &default_model_path),
            )
        } else {
            (
                crate::models::OnnxRuntimeProbeResult {
                    probe_error: Some("python_runtime_missing".to_string()),
                    ..Default::default()
                },
                probe_model_metadata(&python_path, &default_model_path, &requested),
            )
        };
        let high_quality_model_probe =
            probe_model_metadata(&python_path, &high_quality_model_path, &requested);
        let high_quality_torch_ready = python_path.exists()
            && crate::runtime::capability::python_module_is_available(&python_path, "torch", 6)
                .unwrap_or(false);
        let high_quality_runtime_ready = high_quality_model_probe.model_ready
            && high_quality_torch_ready
            && high_quality_model_probe.session_load_ok
            && high_quality_model_probe.model_metadata_ok;

        SeparationEngineHealth {
            active_engine: self.kind().as_str().to_string(),
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
            default_model_session_load_ok: default_model_probe.session_load_ok,
            default_model_session_load_error: default_model_probe.session_load_error.clone(),
            default_model_metadata_ok: default_model_probe.model_metadata_ok,
            default_model_metadata_error: default_model_probe.model_metadata_error.clone(),
            default_model_input_shape: default_model_probe.input_shape.clone(),
            default_model_output_shape: default_model_probe.output_shape.clone(),
            default_model_dummy_inference_ok: default_model_probe.dummy_inference_ok,
            default_model_dummy_inference_error: default_model_probe.dummy_inference_error.clone(),
            high_quality_model_id: Some(HIGH_QUALITY_ONNX_MODEL_ID.to_string()),
            high_quality_model_path: high_quality_model_path.to_string_lossy().to_string(),
            high_quality_model_ready: high_quality_model_probe.model_ready,
            high_quality_model_file_ready: high_quality_model_probe.model_ready,
            high_quality_torch_ready,
            high_quality_runtime_ready,
            high_quality_model_session_load_ok: high_quality_model_probe.session_load_ok,
            high_quality_model_session_load_error: high_quality_model_probe
                .session_load_error
                .clone(),
            high_quality_model_metadata_ok: high_quality_model_probe.model_metadata_ok,
            high_quality_model_metadata_error: high_quality_model_probe
                .model_metadata_error
                .clone(),
            high_quality_model_input_shape: high_quality_model_probe.input_shape.clone(),
            high_quality_model_output_shape: high_quality_model_probe.output_shape.clone(),
            high_quality_model_dummy_inference_ok: high_quality_model_probe.dummy_inference_ok,
            high_quality_model_dummy_inference_error: high_quality_model_probe
                .dummy_inference_error
                .clone(),
            onnxruntime_available: runtime_probe.onnxruntime_available,
            gpu_vendor: None,
            gpu_name: None,
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
