# Macaron Singer

本地音频处理桌面应用。核心能力是人声分离、AI 听写草稿、歌词搜索/导入、播放与歌词编辑。

当前仓库的单一文档入口只有本文件，其他 `.md` 已移除。文档必须跟随当前代码状态，不保留过期的旧分离路线描述。

## 当前状态

- 分离主线已切到 ONNX Runtime。
- 旧动态脚本分离链路已移除。
- AI 听写保持独立，仍依赖 `faster-whisper`，默认 CPU-only。
- Windows 是当前主开发线；macOS 仍作为冻结参考线。

## 架构

- 前端：React + TypeScript
- 桌面壳：Tauri 2.x
- 后端：Rust
- 运行时：嵌入式 Python
- 分离引擎：sherpa-onnx UVR 兼容路径
- 听写引擎：faster-whisper

### 分离主线

- 默认模型：`UVR_MDXNET_9482.onnx`
- 高质量可选模型：`UVR-MDX-NET-Inst_HQ_5.onnx`
- Provider 策略：
  - Windows: `DmlExecutionProvider -> CPUExecutionProvider`
  - macOS: `CoreMLExecutionProvider -> CPUExecutionProvider`
  - fallback: `CPUExecutionProvider`
- 目前实现是完整音频分离执行，不再只是探测层。
- `RuntimeHealthReport.separationEngine` 记录 `requestedProviders`、`selectedProvider`、`providerFallbackReason`、`defaultModelReady`、`highQualityModelReady`、`onnxruntimeAvailable` 等信息。

### 听写主线

- faster-whisper 保持独立
- 不复用分离引擎的 GPU 勾选状态
- 不把分离主线故障扩散为听写故障

## 依赖策略

- PyPI / Python 运行时 / 模型下载走大陆优先策略 + 官方兜底
- ONNX 分离主线依赖：
  - `onnxruntime` 或 Windows 的 `onnxruntime-directml`
  - `numpy`
  - `soundfile`
- 听写额外依赖：
  - `faster-whisper`
- 旧分离路线 / CUDA 不再是分离路线依赖

## 目录

```text
src/
  App.tsx
  components/
  stores/
  types/
  utils/

src-tauri/src/
  lib.rs
  models.rs
  events.rs
  process_control.rs
  runtime/
  separation/
  storage/
  separation_queue.rs

src-tauri/python/
src-tauri/python_workers/
python/
runtime-manifest.json
```

## 开发

```bash
npm install
npm run tauri dev
npm run tauri build
```

### 隔离开发

```bash
npm run tauri:dev:isolated
```

- `FORISFSTOOLS_ISOLATED=1`
- `FORISFSTOOLS_DATA_DIR=/tmp/forisfstools-isolated-runtime`
- 不读取开发目录内的 `python/models`

### Windows 便携版

```bash
npm run win:portable
```

产物：

```text
dist-portable/Macaron-Singer-Windows-Portable.zip
```

## 后端命令

- `import_songs(paths)`
- `start_process(song_id)`
- `cancel_process(song_id)`
- `reprocess_song(song_id)`
- `get_songs()`
- `get_song(song_id)`
- `get_runtime_health()`
- `get_bootstrap_status()`
- `get_lyrics_document(song_id)`
- `save_lyrics_document(song_id, document)`

## 验证

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
npm run build
```

## 说明

- 当前文档以仓库现状为准，不再保留旧的阶段性说明文件。
- 代码级约束以当前 Rust / 前端实现为准。
