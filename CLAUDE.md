# CLAUDE.md

面向后续 Claude Code 维护者的最小交接文档。先读完再动手。

## 项目定位
- 项目名：`4isfstools`
- 应用名：`Macaron Singer`
- 定位：本地音频处理桌面应用，核心能力是人声分离、AI 听写草稿、歌词搜索/导入、播放与歌词编辑。
- 交付目标：macOS `.dmg`，Windows 便携 ZIP（非安装器）
- 平台策略：
  - **macOS 是稳定参考线**：已冻结（2026-05-14），作为行为基准和回归对照，不应随意修改。
  - **Windows 是当前主开发线**：新功能、bugfix、重构均以 Windows 为目标平台。
  - **共享逻辑改动必须避免无意破坏 macOS**：涉及 `#[cfg]` 平台分支、路径解析、进程控制等共享代码时，必须确认 macOS 侧行为不变。

## 主要文件
- [src/App.tsx](/Users/suntong/文件夹/4isfstools-refactor/src/App.tsx)：前端主界面、运行状态、偏好设置、GPU 勾选、启动安装入口。
- [src-tauri/src/lib.rs](/Users/suntong/文件夹/4isfstools-refactor/src-tauri/src/lib.rs)：Tauri commands、安装逻辑、Demucs/Whisper 运行、全局状态（~6200 行）。
- [src-tauri/src/models.rs](/Users/suntong/文件夹/4isfstools-refactor/src-tauri/src/models.rs)：纯数据结构/枚举定义。
- [src-tauri/src/events.rs](/Users/suntong/文件夹/4isfstools-refactor/src-tauri/src/events.rs)：进度/错误事件发送、cancel/job 状态读取。
- [src-tauri/src/runtime/](/Users/suntong/文件夹/4isfstools-refactor/src-tauri/src/runtime/)：运行时检测、manifest 解析、Python 路径、GPU/CUDA 能力检测。
- [src-tauri/src/storage/](/Users/suntong/文件夹/4isfstools-refactor/src-tauri/src/storage/)：路径计算、文件存储辅助。
- [src-tauri/src/process_control.rs](/Users/suntong/文件夹/4isfstools-refactor/src-tauri/src/process_control.rs)：进程控制，Windows 不能回退到会阻塞的旧做法。
- [runtime-manifest.json](/Users/suntong/文件夹/4isfstools-refactor/runtime-manifest.json)：模型来源清单，只管模型源，不负责 torch/profile 决策。

## 当前功能基线（不可回退）

1. 人声分离：Demucs（主链路）
2. 歌词：在线候选（LRCLib）+ LRC 导入 + 手动编辑
3. AI 听写：Whisper base 草稿（独立入口，不与分离强耦合）
4. 播放模式：原唱/伴奏/人声 三模式
5. 偏好设置：自定义路径（伴奏/人声/歌词）、依赖与模型（最小壳环境检测 + 一键安装）
6. 最小壳交付策略：首次运行后在本地补齐运行时依赖与模型，国内源优先海外源 fallback

## 当前 GPU / CPU 设计
- 运行时检测以项目实际 Python 环境中的 `torch.cuda.is_available()` 为准，不以驱动存在代替。
- `RuntimeHealthReport` / `BootstrapStatus` 必须保留并回写 GPU/CUDA 字段：
  - `has_nvidia_gpu`
  - `torch_cuda_available`
  - `torch_version`
  - `torch_cuda_version`
  - `torch_cuda_device_name`
  - `selected_device`
- Windows 一键安装会根据 GPU / torch CUDA 状态选择 CPU 或 CUDA torch profile。

## Whisper 规则
- Whisper / 听写固定 CPU-only。
- 不要把 Demucs 的 GPU 勾选状态传进 Whisper。
- 听写路径如果需要改，只能保持 `device=cpu` / CPU compute type。

## Demucs GPU 勾选规则
- 顶部栏的 `GPU 运行` checkbox 只控制 Demucs 分离是否请求 GPU。
- 默认不勾选。
- 无 NVIDIA GPU 时 checkbox disabled。
- 只有同时满足以下三项，Demucs 才能用 CUDA：
  1. 用户勾选 GPU
  2. 检测到 NVIDIA GPU
  3. 项目 Python 环境 `torch.cuda.is_available() == true`
- 其他情况必须回退 `cpu`，并在结果/日志里保留 fallback 证据。

## 安装逻辑规则
- `ensure_core_runtime_modules` 不能在未勾 GPU 时因 NVIDIA GPU 自动安装/重装 CUDA torch。
- 勾选 GPU 时，才允许走 CUDA torch 安装/验证逻辑。
- CUDA torch 安装后必须二次验证，验证仍失败时不能假装 GPU 已就绪。
- Whisper 安装/验证仍保持 CPU-only。

## 平台分治规则

- 未明确说明平台的修改，默认按 Windows 处理。
- 不要在 Windows 调试中重新打 macOS 包。
- 修改共享文件前先判断是否需要平台分支。
- Windows 分离管线验收必须看新生成的 `song_*` 目录和里面的 `separator.py`，不要用旧任务目录判断新版本。
- 共享文件清单：`src/App.tsx`、`src/components/lyrics/LyricsPanel.tsx`、`src/components/VocalWaveformPreview.tsx`、`src-tauri/src/lib.rs`、`src-tauri/src/process_control.rs`、`runtime-manifest.json`。
- 共享文件的改动适用于两个平台，除非显式添加了平台分支。

## 取消逻辑要点
- 取消必须作用到整个进程组（`process_control.rs` 负责）。
- 新任务启动前要清理旧 cancel 标记和 job 状态。
- 取消后的状态不能被后台收尾逻辑覆盖回 ready/error。
- `update_song_status` guard 允许 `cancelled → pending` 转换。

## 歌词质量路线
- 当前为"候选歌词 + 人工校正"路线。
- 如需提升质量，优先考虑：更好的歌词文本来源、更强的文本切分、更细粒度对齐。

## Windows 一键构建

```powershell
# 只打便携包（推荐）
npm run win:portable

# 全流程（检查 + 构建 + 便携）
powershell -ExecutionPolicy Bypass -File .\scripts\build-from-nas.ps1 -ProjectPath "\\ST-HomeNAS\DataTransFile\4isfstools"

# 只跑便携打包分支
powershell -ExecutionPolicy Bypass -File .\scripts\build-from-nas.ps1 -ProjectPath "\\ST-HomeNAS\DataTransFile\4isfstools" -PortableOnly
```

产物路径：`dist-portable/Macaron-Singer-Windows-Portable.zip`

便携包运行说明：解压 ZIP → 运行 `forisfstools.exe` → 偏好设置 → 依赖与模型 → 一键安装运行环境。

## 运行时安装链路（后端）

核心在 `src-tauri/src/lib.rs`：

1. `bootstrap_install_python_runtime(app)` — Python 运行时安装
2. `ensure_ffmpeg_runtime()` — FFmpeg 检测/安装
3. `ensure_core_runtime_modules(app)` — Python 核心模块（torch/demucs/faster_whisper）
4. `bootstrap_install_models(app)` — 模型安装（demucs + whisper base）
5. `ensure_whisper_runtime_ready(app)` — Whisper 可用性自愈
6. `detect_runtime_health(app)` / `check_runtime_health(app)` — 健康检查

## `runtime-manifest.json` 要点

- 字段是 camelCase（`pythonRuntimeSources`、`ffmpegSources`、`models.demucs`、`modelSources.demucs` 等）
- 模型源已配置为国内优先（hf-mirror / 代理）+ 海外 fallback（huggingface 原站）
- `targetRelpath` + `sha256` 要完整，缺一会导致"下载后仍判定未就绪"

## 常用验证命令
- `cargo check`
- `cargo test`
- `npm run build`
- `git status --short`
- `git diff --stat`
- Windows 分离验收时，检查新 `song_*` 目录里的 `separator_result.json`，不要用旧目录误判。

## 禁止误改清单
- 不要把 `nvidia-smi` / 驱动存在当成 torch CUDA 可用。
- 不要让 Whisper 复用 Demucs 的 GPU 勾选状态。
- 不要回退 Windows 分离管线的 stdout/stderr 处理。
- 不要用旧 `song_*` 目录判断新逻辑是否生效。
- 不要修改 macOS 冻结线，除非明确要求。
- 不要新增与当前目标无关的大重构或新文档体系。
- 不要把 Unix `libc::kill` 逻辑直接回灌到 Windows 分支。

## 最近关键 commits
- `a5a52ee` `fix: demucs queue UI/UX and cancel-restart fixes`
- `2b028df` `fix: serialize demucs separation queue`
- `0ffe1ae` `docs: summarize phase 1 refactor`
- `faabfd3` `refactor: move event helpers out of lib`
- `ccc5ff6` `refactor: move storage helpers out of lib`
- `437d9f2` `refactor: move runtime python helpers out of lib`
- `0dc8b59` `refactor: move runtime capability helpers out of lib`
- `b6d4afe` `refactor: move runtime manifest helpers out of lib`
- `1fbac02` `refactor: move shared models out of lib`
