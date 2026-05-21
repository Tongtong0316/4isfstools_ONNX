# Current Baseline

This document is the sole current baseline for the project as of 2026-05-21.

Superseded documents:
- `docs/BASELINE_2026-05-19.md`
- `docs/ONNX_AGENT_HANDOFF.md`

Use this file as the source of truth for the ONNX separation mainline, current model presets, and the runtime contract.

## 1. Project State

- Separation mainline is ONNX-only.
- Demucs, legacy Demucs, `python -m demucs`, `demucs_worker.py`, `demucs_wrapper.py`, `separator.py`, `torch`, `torchaudio`, and `CUDAExecutionProvider` are not part of the separation mainline.
- Python remains in the app for AI transcription / faster-whisper only.
- Provider strategy is fixed by platform:
  - macOS: `CoreMLExecutionProvider -> CPUExecutionProvider`
  - Windows: `DmlExecutionProvider -> CPUExecutionProvider`
  - fallback / other: `CPUExecutionProvider`

## 2. Current Separation Presets

The app currently exposes two presets only:

| Preset | Model ID | Model File | Branch | segment_size | overlap_ratio | vocals_first | Notes |
|---|---|---|---|---:|---:|---|---|
| Default | `uvr_mdxnet_9482` | `UVR_MDXNET_9482.onnx` | `sherpa_uvr` | 256 | 0.5 | true | Fast, stable, low-resource baseline |
| HQ5 | `uvr_mdx_net_inst_hq_5` | `UVR-MDX-NET-Inst_HQ_5.onnx` | `hq5_direct_mdx` | 256 | 0.5 | false | Higher quality preset with conservative cross-platform settings |

### Default preset details

- `num_threads = 1`
- `debug = false`
- Uses the hidden `sherpa_uvr` tuning path
- Separation output is interpreted as vocals first
- `vocals_path` remains the vocal stem
- `instrumental_path` remains the accompaniment stem

### HQ5 preset details

- `n_fft = 5120`
- `hop_length = 1024`
- `dim_f = 2560`
- `trim = 2560`
- `batch_size = 1`
- `compensate = 1.010`
- Uses the hidden `hq5_direct_mdx` path
- Output order is interpreted in reverse for this model
- `vocals_path` remains the vocal stem
- `instrumental_path` remains the accompaniment stem

## 3. Output Semantics

- `vocals_path` always means the vocal stem.
- `instrumental_path` always means the accompaniment stem.
- `original_mix_path` and `lyrics_path` keep their existing meanings.
- The output order for a model may differ internally, but file writing must preserve the above field semantics.

## 4. Model Sources

- Default model is bundled with the app.
- HQ5 is optional and can be fetched from remote sources.
- HQ5 remote fetch uses mainland-first mirrors with the official source as fallback.

## 5. Runtime / UI Notes

- Runtime health surfaces ONNX Runtime, the default model, ONNX Session, ONNX Metadata, and the HQ model.
- The playlist card may show an `HQ` badge for HQ5 processed tasks.
- UI, lyrics, transcription, queue semantics, cancel semantics, and save-path semantics are not changed by this baseline.

## 6. Verification State

At the time this baseline was captured:

- `cargo fmt --manifest-path src-tauri/Cargo.toml` passed
- `cargo check --manifest-path src-tauri/Cargo.toml` passed
- `cargo test --manifest-path src-tauri/Cargo.toml` passed
- `npm run build` passed
- Demucs audit under `src-tauri/src` and `src` returned no matches

## 7. Non-goals

- Do not reintroduce Demucs or legacy Demucs.
- Do not reintroduce torch / torchaudio as separation dependencies.
- Do not reintroduce `CUDAExecutionProvider`.
- Do not use the older baseline docs as source of truth.

