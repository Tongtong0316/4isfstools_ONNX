# forisfstools Windows 环境 Debug / 构建交接文档（交给 Agent 直接执行）

本文用于在 **Windows 环境**继续开发、调试、打包 `forisfstools`（App 展示名 `Macaron Singer`）。

## 0. 项目身份与命名（务必先对齐）

- 项目目录名：`4isfstools`
- Rust 包/可执行名：`forisfstools`
- 最终 App 展示名：`Macaron Singer`
- 当前维护策略：Windows 是默认开发目标；macOS 已冻结，见 `MACOS_FROZEN_2026-05-14.md`
- 交付目标：
  - macOS：`.app` / `.dmg`
  - Windows：**便携 ZIP（非安装器）**

## 0.1 平台分治规则

- 未明确说明平台的修改，默认按 Windows 处理。
- 不要在 Windows 调试中重新打 macOS 包。
- 修改共享文件前先判断是否需要平台分支。
- Windows 分离管线验收必须看新生成的 `song_*` 目录和里面的 `separator.py`，不要用旧任务目录判断新版本。
- 详细规则见 `PLATFORM_MAINTENANCE.md`。

## 1. 当前功能基线（Windows 调试时不可回退）

1. 人声分离：Demucs（主链路）
2. 歌词：在线候选（LRCLib/163 等）+ LRC 导入 + 手动编辑
3. AI 听写：Whisper base 草稿（独立入口，不与分离强耦合）
4. 播放模式：原唱/伴奏/人声 三模式
5. 偏好设置：
   - 自定义路径（伴奏/人声/歌词）
   - 依赖与模型（最小壳环境检测 + 一键安装）
6. 最小壳交付策略：
   - 首次运行后在本地补齐运行时依赖与模型
   - 国内源优先，海外源 fallback

## 2. 关键文件地图（先读这些再动代码）

### 前端核心
- `/Users/suntong/文件夹/4isfstools/src/App.tsx`
- `/Users/suntong/文件夹/4isfstools/src/components/Playlist.tsx`
- `/Users/suntong/文件夹/4isfstools/src/components/lyrics/LyricsPanel.tsx`

### 后端核心
- `/Users/suntong/文件夹/4isfstools/src-tauri/src/lib.rs`
- `/Users/suntong/文件夹/4isfstools/src-tauri/src/process_control.rs`

### 构建/交付脚本
- `/Users/suntong/文件夹/4isfstools/scripts/build-windows-portable.ps1`
- `/Users/suntong/文件夹/4isfstools/scripts/build-from-nas.ps1`

### 运行时与源配置
- `/Users/suntong/文件夹/4isfstools/runtime-manifest.json`

### 文档基线（改代码必须同步）
- `/Users/suntong/文件夹/4isfstools/README.md`
- `/Users/suntong/文件夹/4isfstools/SPEC.md`
- `/Users/suntong/文件夹/4isfstools/PROGRESS.md`
- `/Users/suntong/文件夹/4isfstools/HANDOFF.md`

## 3. Windows 一键构建（推荐路径）

在 Windows PowerShell 打开项目根目录后执行：

1. 只打便携包（推荐）
```powershell
npm run win:portable
```

2. 全流程（检查 + 构建 + 便携）
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build-from-nas.ps1 -ProjectPath "\\ST-HomeNAS\DataTransFile\4isfstools"
```

3. 只跑全流程里的便携打包分支
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build-from-nas.ps1 -ProjectPath "\\ST-HomeNAS\DataTransFile\4isfstools" -PortableOnly
```

### 产物路径

- 便携 ZIP：
  - `/Users/suntong/文件夹/4isfstools/dist-portable/Macaron-Singer-Windows-Portable.zip`
- 便携目录：
  - `/Users/suntong/文件夹/4isfstools/dist-portable/Macaron Singer Portable`

> 说明：Windows 上实际路径为你本机项目路径（上面是仓库规范路径示例）。

## 4. 便携包运行机制（必须理解）

`scripts/build-windows-portable.ps1` 做的事情：

1. `npm run build`
2. `npm run tauri build -- --bundles none`（只要 exe，不要安装器）
3. 找 `forisfstools.exe`
4. 组装 `dist-portable/Macaron Singer Portable/`
5. 若存在 `python/python-standalone.tar.gz`，拷入 `resources/python/`
6. 压缩成 `Macaron-Singer-Windows-Portable.zip`

运行说明（面向最终用户）：

1. 解压 ZIP
2. 启动 `forisfstools.exe`
3. 进入「偏好设置 -> 依赖与模型」
4. 点击「一键安装运行环境」

## 5. 运行时依赖与模型安装链路（后端）

核心在 `src-tauri/src/lib.rs`：

1. Python 运行时安装：
   - `bootstrap_install_python_runtime(app)`
2. FFmpeg 检测/安装：
   - `ensure_ffmpeg_runtime()`
3. Python 核心模块（torch/demucs/faster_whisper）：
   - `ensure_core_runtime_modules(app)`
4. 模型安装（demucs + whisper base）：
   - `bootstrap_install_models(app)`
5. Whisper 可用性自愈：
   - `ensure_whisper_runtime_ready(app)`
6. 健康检查：
   - `detect_runtime_health(app)` / `check_runtime_health(app)`

## 6. `runtime-manifest.json` 结构要点（Windows 常见误区）

### 注意字段是 camelCase

- `pythonRuntimeSources`
- `ffmpegSources`
- `models.demucs`
- `models.whisperBase`
- `modelSources.demucs`
- `modelSources.whisperBase`

### 现状要点

1. 模型源已配置为：
   - 国内优先（如 hf-mirror / 代理）
   - 海外 fallback（如 huggingface 原站）
2. `targetRelpath` + `sha256` 要完整，缺一会导致“下载后仍判定未就绪”
3. 如果 Agent 改了字段名（例如写成 snake_case），安装链会读不到

## 7. Windows 进程终止风险点（已做平台隔离）

文件：`src-tauri/src/process_control.rs`

- Unix：进程组 + `SIGTERM/SIGKILL`
- Windows：`taskkill /PID ... /T`（必要时 `/F`）

Agent 禁止把 Unix `libc::kill` 逻辑直接回灌到 Windows 分支。

## 8. 你最可能遇到的 8 个故障与定位

1. `powershell: command not found`
   - 说明不是在 Windows 或环境变量缺失
2. `forisfstools.exe` 找不到
   - `tauri build -- --bundles none` 未成功
3. 一键安装后仍“Whisper base 未就绪”
   - 重点查 `runtime-manifest.json` 的 whisper 条目是否完整
   - 查 `tokenizer.json` 是否损坏/空文件
4. “安装完成但仍组件缺失”
   - 这是合理状态：某些组件装成、某些失败
   - 应继续提示具体缺失项，不可笼统写“可运行”
5. 模型下载成功但自检失败
   - 看 `whisper_model_probe` 的错误细节
6. Windows 构建通过但启动崩溃
   - 优先查 runtime 目录权限与路径（含中文/空格）
7. 切歌后歌词丢失，重启后恢复
   - 通常是内存态未回填，非持久化损坏
8. 音频模式切换瞬时静音
   - 播放轨切换逻辑需保 currentTime，不可重启轨道

## 9. 建议的 Windows Debug 顺序（Agent 执行顺序）

1. `npm ci`
2. `cargo check`
3. `npm run build`
4. `npm run win:portable`
5. 解压便携包并运行
6. 打开「依赖与模型」执行一键安装
7. 再跑：
   - 导入歌曲 -> 分离
   - 搜索歌词 -> 应用
   - AI 听写（Whisper base）
8. 记录失败点：
   - UI 提示
   - 后端详细错误
   - runtime 目录结构

## 10. 验收清单（Windows 交付前必过）

1. App 可启动
2. 偏好设置可打开并保存路径
3. 一键安装后，健康检查正确反映状态
4. Demucs 可分离并生成伴奏/人声
5. FFmpeg 可用于复合/转码
6. Whisper base 可生成草稿
7. 歌词候选搜索可用，应用后可编辑
8. 播放三模式可切换，不崩
9. 便携 ZIP 解压即用，不依赖安装器

## 11. 文档同步规则（强制）

每次完成一个可验证阶段，必须同步：

1. `PROGRESS.md`（做了什么、验证结果、残留问题）
2. `SPEC.md`（行为边界、约束）
3. `README.md`（对外可见使用方式）
4. `HANDOFF.md`（下个 agent 的切入点）

## 12. 给接手 Agent 的一句话

先保证“最小壳 -> 一键安装 -> 全能力回归”这条主链稳定，再做额外优化；任何改动都不得破坏 Demucs 分离、人声/伴奏播放与 Whisper base 草稿三件核心能力。
