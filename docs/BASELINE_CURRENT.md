# Macaron Singer v1.0 macOS Release Baseline

This document is the sole current baseline for the project as of 2026-05-22.

Superseded documents:
- `docs/BASELINE_2026-05-19.md`
- `docs/ONNX_AGENT_HANDOFF.md`

Use this file as the source of truth for the current product state, ONNX separation mainline, runtime contract, optional dependency rules, and local delivery expectations.

This baseline is specifically the earliest macOS build that is acceptable as a release candidate for local delivery.

## 1. Version and Scope

- Product name: `Macaron Singer`
- App version: `1.0.0`
- Baseline label: `v1.0`
- Release lane: `macOS first shippable baseline`
- Repository baseline head at capture time: `f1e0783`
- Current branch at capture time: `main`

This baseline covers:
- desktop runtime behavior
- ONNX separation mainline
- default and HQ5 model presets
- optional AI transcription runtime
- runtime UI expectations
- macOS local build and packaging expectations

This baseline does not authorize:
- reintroducing the legacy separator stack
- reintroducing `Demucs`
- reintroducing `Legacy Demucs`
- reintroducing `CUDAExecutionProvider`
- reintroducing `python -m demucs`
- making optional transcription dependencies block core separation

## 2. Product State

- Separation mainline is ONNX-only.
- Default separation is the built-in ONNX preset.
- HQ separation is the optional HQ5 preset.
- Python remains in the app for local inference helpers and AI transcription.
- AI transcription remains optional and independent from separation core readiness.
- The app keeps existing song field semantics:
  - `vocals_path` always means the vocal stem
  - `instrumental_path` always means the accompaniment stem
  - `original_mix_path` keeps its original meaning
  - `lyrics_path` keeps its original meaning

## 3. Runtime Contract

Provider strategy is fixed by platform:
- macOS: `CoreMLExecutionProvider -> CPUExecutionProvider`
- Windows: `DmlExecutionProvider -> CPUExecutionProvider`
- fallback / other: `CPUExecutionProvider`

The current runtime contract is:
- default separation must be runnable without `torch`
- HQ5 may require `torch`
- `faster-whisper` is optional and must not block default separation readiness
- HQ5 model absence must not block default separation
- default model absence is allowed to block core separation readiness

## 4. Current Separation Presets

The app currently exposes two separation presets only.

| Preset | Model ID | Model File | Branch | segment_size | overlap_ratio | vocals_first | Notes |
|---|---|---|---|---:|---:|---|---|
| Default | `uvr_mdxnet_9482` | `UVR_MDXNET_9482.onnx` | `sherpa_uvr` | 256 | 0.5 | true | Built-in, stable, low-resource preset |
| HQ5 | `uvr_mdx_net_inst_hq_5` | `UVR-MDX-NET-Inst_HQ_5.onnx` | `hq5_direct_mdx` | 256 | 0.5 | false | Optional higher-quality preset |

### Default preset details

- `num_threads = 1`
- `debug = false`
- hidden branch: `sherpa_uvr`
- hidden tuning interprets `segment_size` as internal frame count
- output interpretation is vocals first
- `vocals_path` stays the vocal stem
- `instrumental_path` stays the accompaniment stem

### HQ5 preset details

- hidden branch: `hq5_direct_mdx`
- `n_fft = 5120`
- `hop_length = 1024`
- `dim_f = 2560`
- `trim = 2560`
- `batch_size = 1`
- `compensate = 1.010`
- output interpretation is reversed internally for this model
- file writing still preserves:
  - `vocals_path = vocals`
  - `instrumental_path = accompaniment`

## 5. Dependency Roles

Core separation path:
- `Python`: local runtime host for inference helpers
- `FFmpeg`: audio read / convert / resample support
- `onnxruntime`: ONNX session execution
- `SoundFile`: Python-side audio I/O
- `NumPy`: numeric processing
- `Sherpa ONNX`: default model execution helper
- bundled default model: `UVR_MDXNET_9482.onnx`

Optional HQ path:
- remote HQ5 model: `UVR-MDX-NET-Inst_HQ_5.onnx`
- `torch`: HQ5-only runtime dependency in the current implementation

Optional transcription path:
- `faster-whisper`: optional runtime package
- `Whisper base`: optional transcription model

Rules:
- default model must not require `torch`
- HQ5 readiness requires both model file and HQ runtime dependency readiness
- transcription readiness requires both `faster-whisper` and `Whisper base`

## 6. Model Source Rules

Default model:
- bundled with the app
- expected in Tauri resources
- expected to be copied into runtime when needed

HQ5:
- optional
- fetched remotely
- mainland-first mirrors are preferred
- official source remains fallback

Transcription model:
- optional
- downloaded only through the AI transcription path
- must not be auto-installed as part of minimum ONNX bootstrap

## 7. Runtime UI Contract

The runtime page is ONNX-oriented and must reflect the current dependency split.

Static runtime cards include:
- ONNX Runtime
- Python
- FFmpeg
- SoundFile
- NumPy
- Torch
- faster-whisper
- default model
- HQ5 model
- AI transcription

Semantic rules:
- `Torch` card means HQ5-only dependency
- `faster-whisper` card means optional AI transcription package
- default model card must reflect bundled/built-in behavior
- HQ5 card may reflect optional download behavior
- AI transcription card must not become ready from `Whisper base` alone

## 8. Playback and Waveform State

Current playback expectations:
- playback uses the existing instrumental/vocal/original routing semantics
- queue, cancel, save-path, and lyrics semantics are unchanged by this baseline

Current waveform state:
- original vocal waveform is optional in the player
- waveform no longer supports drag-to-seek
- waveform playhead behavior:
  - starts from the left edge
  - stays centered through the middle portion
  - moves toward the right edge near the end
- current low-risk optimization already applied:
  - waveform does not start new loading when hidden
  - waveform peaks are cached in memory for recent songs

## 9. Health and Readiness Semantics

Core separation readiness depends on:
- Python runtime
- FFmpeg
- ONNX Runtime
- default ONNX model
- SoundFile
- NumPy
- Sherpa ONNX

HQ5 readiness depends on:
- HQ5 model file
- `torch`
- ONNX execution path being usable for the HQ branch

Transcription readiness depends on:
- `faster-whisper`
- `Whisper base`

Important rule:
- optional readiness must not be inferred from partial state
- AI transcription readiness must not be inferred from `Whisper base` alone

## 10. Recent Fixes Included In This Baseline

Recent relevant commits at the time of this baseline:
- `9020144` `chore: sync cargo lock version`
- `738ee9a` `fix: remove infinity theme swatch border, drop AI 听写 runtime check`
- `b5ff463` `chore: checkpoint ONNX runtime dependency flow`
- `1b62645` `fix: align optional runtime readiness checks`
- `f1e0783` `perf: cache vocal waveform peaks`

Practical effect of the latest fixes:
- optional runtime readiness checks are aligned better
- AI transcription download validates both runtime package and model
- waveform hidden state no longer triggers fresh decode work
- recent waveform peaks are cached

## 11. Verification State

At the time this baseline was updated:

- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` passed
- `cargo check --manifest-path src-tauri/Cargo.toml` passed
- `cargo test --manifest-path src-tauri/Cargo.toml` last known passing state remains part of this baseline lineage
- `npm run build` passed
- legacy separator audit under `src-tauri/src` and `src` returned no intended mainline matches

## 12. Local Delivery Expectations

For local delivery on macOS:
- frontend build output is expected under `dist/`
- Tauri packaging uses `src-tauri/tauri.conf.json`
- packaged resources include:
  - `python/python-standalone.tar.gz`
  - `runtime-manifest.json`
  - `models/onnx/UVR_MDXNET_9482.onnx`

Expected local bundle artifacts are produced under the Tauri target bundle directories after a successful build.

Current local delivery artifact paths for this baseline:
- `.app`: `/Users/suntong/Library/Caches/banzou-master/cargo-target/release/bundle/macos/Macaron Singer.app`
- `.dmg`: `/Users/suntong/Library/Caches/banzou-master/cargo-target/release/bundle/dmg/Macaron Singer_1.0.0_aarch64.dmg`

## 13. Non-goals

- Do not reintroduce the legacy separator stack.
- Do not reintroduce `Demucs`.
- Do not reintroduce `Legacy Demucs`.
- Do not reintroduce `CUDAExecutionProvider`.
- Do not reintroduce `python -m demucs`.
- Do not make optional transcription dependencies block default ONNX separation.
- Do not treat superseded baseline docs as source of truth.
