use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use tauri::AppHandle;

use crate::models::SeparationEngineHealth;
use crate::runtime::python::get_python_path;

use super::audio_io::normalize_source_audio;
use super::engine::{ProviderStrategy, SeparationEngine, SeparationEngineKind};
use super::model_registry::{ModelRegistry, HIGH_QUALITY_ONNX_MODEL_ID};

/// Per-model configuration for the separation pipeline.
/// Each model_id maps to one of these; the config is passed to the Python script.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct ModelConfig {
    /// "sherpa_uvr" uses the upstream UVR implementation; "direct_ort" uses the local sidecar config path.
    pub execution_backend: String,
    /// "Voc" if the model outputs vocal masks, "Inst" if it outputs instrumental masks
    pub output_target: String,
    /// "mask" if output should multiply the input spectrogram, "direct" if output is raw STFT
    pub output_mode: String,
    /// Internal chunk size along the time axis (256 for standard MDX-NET)
    pub chunk_size: u32,
    pub n_fft: u32,
    pub hop_length: u32,
    pub dim_f: u32,
    pub model_id: String,
}

fn get_model_config(model_id: &str) -> ModelConfig {
    match model_id {
        // HQ model: outputs instrumental mask, n_fft=5120
        "high_quality" => ModelConfig {
            execution_backend: "direct_ort".into(),
            output_target: "Inst".into(),
            output_mode: "mask".into(),
            chunk_size: 256,
            n_fft: 5120,
            hop_length: 1280,
            dim_f: 2560,
            model_id: "high_quality".into(),
        },
        // Default / Karaoke / standard MDX-NET: output vocal mask, n_fft=4096
        _ => ModelConfig {
            execution_backend: "sherpa_uvr".into(),
            output_target: "Voc".into(),
            output_mode: "mask".into(),
            chunk_size: 256,
            n_fft: 4096,
            hop_length: 1024,
            dim_f: 2048,
            model_id: model_id.into(),
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
    let model_config = get_model_config(model_id);
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

    let script = r#"
import json
import sys
from pathlib import Path

import numpy as np
import soundfile as sf
import onnxruntime as ort

payload = {
    "success": False,
    "error": None,
    "error_code": None,
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
}

def write_payload():
    print(json.dumps(payload, ensure_ascii=False))

def provider_to_sherpa(provider):
    return {
        "CoreMLExecutionProvider": "coreml",
        "DmlExecutionProvider": "dml",
        "CPUExecutionProvider": "cpu",
    }.get(provider, "cpu")

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

def separate_via_sherpa(model_path, input_path, vocals_path, instrumental_path, model_cfg=None):
    import sherpa_onnx

    provider = model_cfg.get("provider", "CPUExecutionProvider") if model_cfg else "CPUExecutionProvider"
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

    sp = sherpa_onnx.OfflineSourceSeparation(config)
    samples, sample_rate = sf.read(str(input_path), dtype="float32", always_2d=True)
    samples = np.transpose(samples)
    if samples.shape[0] == 1:
        samples = np.repeat(samples, 2, axis=0)
    if samples.shape[0] != 2:
        payload["error_code"] = "ONNX_AUDIO_PREP_FAILED"
        payload["error"] = f"expected_stereo_input:{samples.shape}"
        return

    output = sp.process(sample_rate=sample_rate, samples=np.ascontiguousarray(samples))
    payload["sampleRate"] = int(output.sample_rate)
    if len(output.stems) != 2:
        payload["error_code"] = "ONNX_SEPARATION_FAILED"
        payload["error"] = f"unexpected_stem_count:{len(output.stems)}"
        return

    vocals = np.transpose(output.stems[0].data)
    non_vocals = np.transpose(output.stems[1].data)
    sf.write(str(vocals_path), vocals, samplerate=output.sample_rate)
    sf.write(str(instrumental_path), non_vocals, samplerate=output.sample_rate)

    payload["segmentCount"] = 1
    payload["outputSampleCount"] = int(vocals.shape[0])

def stft_np(signal, n_fft, hop, window):
    pad = n_fft // 2
    if signal.shape[0] == 0:
        raise ValueError("empty_audio")
    padded = np.pad(signal, (pad, pad), mode="reflect" if signal.shape[0] > 1 else "constant")
    if padded.shape[0] < n_fft:
        padded = np.pad(padded, (0, n_fft - padded.shape[0]), mode="constant")
    frame_count = 1 + max(0, (padded.shape[0] - n_fft) // hop)
    frames = np.empty((n_fft // 2 + 1, frame_count), dtype=np.complex64)
    for idx in range(frame_count):
        start = idx * hop
        frame = padded[start:start + n_fft]
        if frame.shape[0] < n_fft:
            frame = np.pad(frame, (0, n_fft - frame.shape[0]), mode="constant")
        frames[:, idx] = np.fft.rfft(frame * window).astype(np.complex64)
    return frames

def istft_np(spec, n_fft, hop, window, original_len):
    pad = n_fft // 2
    frame_count = spec.shape[1]
    out_len = n_fft + hop * max(0, frame_count - 1)
    output = np.zeros(out_len, dtype=np.float32)
    window_sum = np.zeros(out_len, dtype=np.float32)
    for idx in range(frame_count):
        start = idx * hop
        frame = np.fft.irfft(spec[:, idx], n=n_fft).astype(np.float32)
        output[start:start + n_fft] += frame * window
        window_sum[start:start + n_fft] += window * window
    nonzero = window_sum > 1e-8
    output[nonzero] /= window_sum[nonzero]
    end = pad + original_len
    if output.shape[0] < end:
        output = np.pad(output, (0, end - output.shape[0]), mode="constant")
    return output[pad:end].astype(np.float32)

def separate_via_ort(model_path, input_path, vocals_path, instrumental_path, model_cfg=None):
    """Use onnxruntime directly with numpy STFT/ISTFT."""
    if model_cfg is None:
        model_cfg = {}
    provider = model_cfg.get("provider", "CPUExecutionProvider")
    sess = make_session(model_path, provider)
    meta = sess.get_modelmeta()
    in_name = sess.get_inputs()[0].name
    out_name = sess.get_outputs()[0].name

    samples, sr = sf.read(str(input_path), dtype="float32", always_2d=True)
    samples_t = np.transpose(samples)
    if samples_t.shape[0] == 1:
        samples_t = np.repeat(samples_t, 2, axis=0)
    original_len = samples_t.shape[1]

    payload["sampleRate"] = sr
    # Derive dim_f from ONNX model's input shape: [batch, 4, dim_f, 256]
    in_shape = sess.get_inputs()[0].shape
    inferred_dim_f = 2560
    if len(in_shape) >= 3:
        d = in_shape[2]
        if isinstance(d, int) and d > 0:
            inferred_dim_f = d
    n_fft = int(model_cfg.get("n_fft") or meta.custom_metadata_map.get("n_fft", str(inferred_dim_f * 2)))
    hop = int(model_cfg.get("hop_length") or meta.custom_metadata_map.get("hop_length", str(n_fft // 4)))
    dim_f = int(model_cfg.get("dim_f") or meta.custom_metadata_map.get("dim_f", str(inferred_dim_f)))
    fft_bins = n_fft // 2 + 1
    if dim_f > fft_bins:
        raise ValueError(f"invalid_dim_f:{dim_f}>fft_bins:{fft_bins}")

    window = np.hanning(n_fft).astype(np.float32)
    specs = []
    for ch in range(2):
        specs.append(stft_np(samples_t[ch].astype(np.float32), n_fft, hop, window))

    n_frames = min(s.shape[-1] for s in specs)
    stack = []
    for s in specs:
        s = s[:dim_f, :n_frames]
        stack.append(s.real)
        stack.append(s.imag)
    model_in = np.stack(stack, axis=0)[None, :, :, :].astype(np.float32)  # [1, 4, dim_f, n_frames]

    chunk_size = int(model_cfg.get("chunk_size", 256))
    t = model_in.shape[3]
    if t == 0:
        raise ValueError("empty_spectrogram")

    pad_len = (chunk_size - t % chunk_size) % chunk_size
    if pad_len:
        model_in = np.pad(model_in, ((0, 0), (0, 0), (0, 0), (0, pad_len)), mode="constant")
        t_padded = model_in.shape[3]
    else:
        t_padded = t

    num_chunks = t_padded // chunk_size
    ort_parts = []
    for i in range(num_chunks):
        chunk = model_in[:, :, :, i * chunk_size : (i + 1) * chunk_size]
        part = sess.run([out_name], {in_name: chunk.astype(np.float32)})[0]
        ort_parts.append(part)

    ort_out = np.concatenate(ort_parts, axis=3)
    if pad_len:
        ort_out = ort_out[:, :, :, :-pad_len]
    ort_out = ort_out[:, :, :dim_f, :n_frames]

    output_mode = model_cfg.get("output_mode", "mask")
    out_chs = []
    for ch_idx in range(2):
        ch_real = ort_out[0, ch_idx * 2].astype(np.float32)
        ch_imag = ort_out[0, ch_idx * 2 + 1].astype(np.float32)
        spec = specs[ch_idx][:, :n_frames]
        if output_mode == "direct":
            spec_out = np.zeros((fft_bins, n_frames), dtype=np.complex64)
            spec_out[:dim_f] = ch_real + 1j * ch_imag
        else:
            spec_out = spec.copy()
            mask = ch_real + 1j * ch_imag
            spec_out[:dim_f] = spec[:dim_f] * mask
        out_chs.append(istft_np(spec_out, n_fft, hop, window, original_len))

    max_len = max(len(c) for c in out_chs)
    out_stereo = np.zeros((max_len, 2), dtype=np.float32)
    for i, c in enumerate(out_chs):
        out_stereo[:len(c), i] = c

    # Normalize output to prevent clipping
    peak = np.max(np.abs(out_stereo))
    if peak > 1.0:
        out_stereo /= peak

    # Output target: "Voc" or "Inst", from model config (fallback: detect from model name)
    output_target = model_cfg.get("output_target", None)
    if output_target is None:
        output_target = "Inst" if "Inst" in meta.custom_metadata_map.get("model_name", "") else "Voc"

    inp = np.transpose(samples_t)
    min_len = min(inp.shape[0], out_stereo.shape[0])
    if output_target == "Inst":
        # Model outputs instrumental → instrumental = model_out, vocals = mix - instrumental
        sf.write(str(instrumental_path), out_stereo, samplerate=sr)
        voc = inp[:min_len] - out_stereo[:min_len]
        sf.write(str(vocals_path), voc, samplerate=sr)
    else:
        # Model outputs vocals → vocals = model_out, instrumental = mix - vocals
        sf.write(str(vocals_path), out_stereo, samplerate=sr)
        instr = inp[:min_len] - out_stereo[:min_len]
        sf.write(str(instrumental_path), instr, samplerate=sr)
    payload["segmentCount"] = num_chunks
    payload["outputSampleCount"] = int(out_stereo.shape[0])

try:
    requested = json.loads(sys.argv[1])
    model_path = Path(sys.argv[2])
    input_path = Path(sys.argv[3])
    vocals_path = Path(sys.argv[4])
    instrumental_path = Path(sys.argv[5])
    provider = sys.argv[6]
    model_cfg = json.loads(sys.argv[7]) if len(sys.argv) > 7 else {}
    model_cfg["provider"] = provider

    payload["requestedProviders"] = requested
    payload["modelPath"] = str(model_path)
    payload["inputPath"] = str(input_path)
    payload["vocalsPath"] = str(vocals_path)
    payload["instrumentalPath"] = str(instrumental_path)
    payload["selectedProvider"] = provider

    if not model_path.is_file():
        payload["error_code"] = "ONNX_ENGINE_NOT_READY"
        payload["error"] = f"model_missing:{model_path}"
        write_payload()
        raise SystemExit(0)

    if requested and provider == "CPUExecutionProvider" and requested[0] != "CPUExecutionProvider":
        payload["providerFallbackReason"] = f"provider_fallback_to_cpu:{requested[0]}"

    payload["onnxruntimeAvailable"] = True

    if model_cfg.get("execution_backend") == "sherpa_uvr":
        separate_via_sherpa(model_path, input_path, vocals_path, instrumental_path, model_cfg)
    else:
        separate_via_ort(model_path, input_path, vocals_path, instrumental_path, model_cfg)

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

    let config_json = serde_json::to_string(&model_config).unwrap_or_else(|_| "{}".to_string());
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
            &config_json,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = crate::spawn_in_own_process_group(&mut command)
        .map_err(|e| format!("onnx separation spawn failed: {}", e))?;
    crate::register_separator_job(song_id, child.id());

    let output = child
        .wait_with_output()
        .map_err(|e| format!("onnx separation wait failed: {}", e))?;
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
