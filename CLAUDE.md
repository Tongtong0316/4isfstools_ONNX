# CLAUDE.md

面向后续 Claude Code 维护者的最小交接文档。先读完再动手。

## 项目定位
- 项目名：`4isfstools`
- 应用名：`Macaron Singer`
- 定位：本地音频处理桌面应用，核心能力是人声分离、AI 听写草稿、歌词搜索/导入、播放与歌词编辑。
- 平台策略：macOS 已冻结，后续默认按 Windows 主线维护；涉及共享逻辑时要明确平台影响。

## 主要文件
- [src/App.tsx](/Users/suntong/文件夹/4isfstools/src/App.tsx)：前端主界面、运行状态、偏好设置、GPU 勾选、启动安装入口。
- [src-tauri/src/lib.rs](/Users/suntong/文件夹/4isfstools/src-tauri/src/lib.rs)：运行时检测、安装、Demucs 分离、Whisper 听写、separator 任务生成。
- [src-tauri/src/process_control.rs](/Users/suntong/文件夹/4isfstools/src-tauri/src/process_control.rs)：进程控制，Windows 不能回退到会阻塞的旧做法。
- [runtime-manifest.json](/Users/suntong/文件夹/4isfstools/runtime-manifest.json)：模型来源清单，只管模型源，不负责 torch/profile 决策。

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

## 常用验证命令
- `cargo check`
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

## 最近关键 commits
- `7d49174` `runtime: gate cuda torch install behind gpu toggle`
- `016e7d7` `ui/runtime: gate demucs cuda behind gpu toggle`
- `5270440` `separation: record demucs selected device`
- `802cca6` `runtime: select cpu or cuda torch profile`
- `438a14b` `ui: show runtime cuda capability`

