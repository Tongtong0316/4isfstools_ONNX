# Development Anchors

## Anchor: a5a52ee — Demucs FIFO Queue + Cancel-Restart Fix

**日期**: 2026-05-15
**分支**: `base/windows-refactor-phase1`

### 功能实现

| 规则 | 实现 |
|------|------|
| 同一时间只允许一个分离任务执行 | `separation_queue.rs` worker_loop 串行 pop_front |
| 其他任务导入/重开后显示 queued/排队中 | `types/index.ts` 新增 `'queued'` 状态；`Playlist.tsx` 显示"排队中..."；`App.tsx` invoke 成功后设 status="queued" |
| 当前任务完成或取消后自动启动下一个 | `separation_queue.rs` worker_loop 循环；process_song_background 返回后继续 pop |
| 取消任务后可重新启动，不再卡 checking_gpu | `lib.rs:2524` guard 允许 cancelled→pending；`App.tsx` 移除 progress handler 中 cancelled→processing 拦截 |

### Bug 修复

| Bug | 根因 | 修复 |
|-----|------|------|
| 分离取消后无法重开 | `update_song_status` guard 拦截 cancelled→pending | `lib.rs:2524` 增加 `status != "pending"` |
| 假启动（后端拒绝但前端显示 processing） | `App.tsx` invoke 前无条件设 status="processing" | 改为 invoke 成功后设 status="queued" |
| 取消后重启卡在 checking_gpu | progress handler 中 `cancelled && nextStatus==="processing"` 拦截 | 移除该 guard |

### 提交记录

```
a5a52ee fix: demucs queue UI/UX and cancel-restart fixes
2b028df fix: serialize demucs separation queue
```

### 人工测试通过项

- 连续导入 2 首歌 → 第 1 个 processing、第 2 个"排队中..."
- 取消排队中的任务 → 状态变为"已取消"→ 可重新启动
- 取消正在处理的任务 → 重新启动 → 不再卡 checking_gpu
- 第 1 个任务完成后 → 第 2 个自动开始

### 验证命令

```
cargo check ✓
cargo test ✓ (4 passed)
npm run build ✓
```

---

## Anchor: Phase 1 Refactor — Mechanical Module Split

**日期**: 2026-05-14
**分支**: `base/windows-refactor-phase1`（基于 `dec1414`）

### 概述

将 `src-tauri/src/lib.rs` 从 7325 行机械拆分为 7 个文件，共 7376 行（含新增模块声明和导入）。行为完全不变。

### 拆分 Commit 列表

| Commit | 描述 | 新增文件 | lib.rs 行变化 |
|--------|------|----------|---------------|
| `1fbac02` | models → models.rs | models.rs (390行) | -376 |
| `b6d4afe` | manifest → runtime/manifest.rs | runtime/manifest.rs (72行) | -64 |
| `0dc8b59` | capability → runtime/capability.rs | runtime/capability.rs (342行) | -372 |
| `437d9f2` | python → runtime/python.rs | runtime/python.rs (137行) | -142 |
| `ccc5ff6` | storage → storage/mod.rs | storage/mod.rs (64行) | -61 |
| `faabfd3` | events → events.rs | events.rs (99行) | -99 |

### 验证结果

- `cargo check`: 通过（仅 process_control.rs 预存 `unused variable: command` warning）
- `cargo test`: 4 passed, 0 failed
- `npm run build`: 通过（38 modules, 1.47s）

### 模块职责

| 模块 | 职责 | 行数 |
|------|------|------|
| `lib.rs` | Tauri commands、安装逻辑、Demucs/Whisper 运行、全局状态 | 6272 |
| `models.rs` | 纯数据结构/枚举定义 | 390 |
| `events.rs` | 进度/错误事件发送、cancel/job 状态读取 | 99 |
| `runtime/manifest.rs` | runtime-manifest.json 读取/解析 | 72 |
| `runtime/capability.rs` | GPU/CUDA/Torch 能力检测 | 342 |
| `runtime/python.rs` | Python 路径解析、命令执行 | 137 |
| `storage/mod.rs` | 路径计算、文件存储辅助 | 64 |

### 保持不变的业务规则

1. **GPU 三条件门控**: 用户勾选 + NVIDIA GPU 检测 + `torch.cuda.is_available()` 缺一不可
2. **Whisper CPU-only**: `WHISPER_DEVICE=cpu` 硬编码，不复用 Demucs GPU 状态
3. **Demucs CUDA 运行**: 仅在 GPU 三条件均满足时启用
4. **Windows 进程控制**: 分离管线 stdout/stderr 处理不回退
5. **不使用旧 song_* 目录**: 遵循新路径规范
6. **macOS freeze**: 不触碰 macOS 相关逻辑

### 后续禁止直接碰的高风险模块

| 模块/函数 | 原因 |
|-----------|------|
| `install_torch_with_cuda_detection` | GPU 安装核心决策，三条件门控在此实现 |
| `ensure_core_runtime_modules` | 运行时依赖安装，影响启动流程 |
| `generate_whisper_base_lyrics` | Whisper 调用入口，必须保持 CPU-only |
| `start_demucs_separation` / `run_demucs_separation` | Demucs 运行核心，CUDA 设备选择在此 |
| `process_control::spawn_in_own_process_group` | Windows 进程组管理，已知复杂度 |
| `SONGS` / `CANCEL_FLAGS` / `ACTIVE_JOB_TOKENS` | 全局状态，修改需全链路验证 |
| `src/App.tsx` 中 GPU checkbox 逻辑 | 前端三条件门控 UI，已修复 useCallback 依赖 |

---

## Anchor: 2026-05-13 — Windows Compilation Baseline

**日期**: 2026-05-13
**用途**: Windows 编译与验收的冻结基线

### 关键文件指纹

- `src/App.tsx`: `e9671bd13fed009e2efe46c0ce61497d08d4c3471bc292f0b9a2ff687bb9b862`
- `src/components/lyrics/LyricsPanel.tsx`: `cafc4c870c0252e788e76dfb958f6bb932fb722021e545c7ae0b7b4a774be450`
- `src/components/VocalWaveformPreview.tsx`: `dddfc2daf14744e432e833f6f6c76a00765d58e8dd5dcc45371af590616c95ca`
- `runtime-manifest.json`: `7b0745884826322ad2eb4078dce445e52b75fff3b416e66cc4f8ca32cdef66b0`

### 验证重点

- 歌词面板使用居中布局，活动行有安全边距
- 新增可切换的原始人声波形预览
- 波形切换是进度条附近的浮动控件，不属于主模式按钮

### 备注

- 平台维护策略于 2026-05-14 更改，macOS 此后单独冻结
- 此快照作为 Windows 编译和验收的 source of truth

---

## Anchor: 2026-05-12 — Recovered Baseline

**日期**: 2026-05-12
**用途**: 标记恢复后的基线，继续跨平台交付前的追溯点

### 关键文件指纹

- `src/App.tsx`: `e0a676af3f6aec58cf57b39df95633bc2d93960947dd59e157ba0eced956c459`
- `src-tauri/src/lib.rs`: `b2517bfda8b5ef094aa2d1221d89f914f5500177e74c2af8c5f84ccebe25c364`
- `runtime-manifest.json`: `7b0745884826322ad2eb4078dce445e52b75fff3b416e66cc4f8ca32cdef66b0`

### 备注

- macOS 交付此前已本地验证通过
- Windows 交付仍需环境/安装/模型稳定性验证

---

## Anchor: Windows Vocal Separation Golden Baseline

**基线时间**: 2026-05-12 23:36（Windows 侧已验证通过）
**适用范围**: 仅用于 Windows 人声分离管线的追溯、回归对比与后续修复判定。

### 确认可用的行为

1. 能够完成一首短音频的分离
2. 能正确生成结果文件（`vocals.wav`、`no_vocals.wav`）
3. `separator_result.json` 可以写出
4. `success: true` 能回写

### 关键修法

1. **Demucs 子进程输出管道修正**: `stdout=PIPE, stderr=PIPE` → `stdout=PIPE, stderr=STDOUT`（避免 Windows 下 stdout/stderr 互相堵住）
2. **进度文件驱动**: 由 `separator_progress.json` 提供后续进度，任务结束后清理进度文件
3. **旧任务目录不能当新任务看**: 必须基于新建任务目录、新生成的 `separator.py`、新 `separator_progress.json`、新 `separator_result.json` 判断

### 回归排查清单

1. `separator.py` 是否还是旧版输出管道写法
2. 是否真的创建了新的 `song_*` 目录
3. `separator_progress.json` 是否停止更新
4. 是否缺少 `separator_result.json`
5. 任务是不是只完成了安装但没有真正跑到分离阶段
6. 运行时依赖是否完整

---

## Anchor: Windows Fix Log Summary (2026-05-11 ~ 2026-05-12)

### 已修复的核心问题

| 问题 | 根因 | 修复方案 |
|------|------|----------|
| demucs 进度卡死 (torchaudio 2.11.0) | torchaudio 改用 torchcodec 后端，需要 FFmpeg DLL | 直接修补 `torchaudio/__init__.py` 源码，用 soundfile 替代 |
| demucs 进度卡死 (ramp 封顶) | progress ramp 只从 20% 到 40% | 解析 demucs stdout 实时进度条，映射到 20-90% |
| Python 控制台窗口频繁弹出 | Windows GUI 应用 spawn 子进程时默认分配控制台 | `CREATE_NO_WINDOW` (0x08000000) 标志 |
| FFmpeg 检测失败 | `resolve_ffmpeg_binary_path()` 不检查 Windows 运行时目录 | 增加 `runtime\ffmpeg\bin\ffmpeg.exe` 检测 |
| 进度文件死锁 | Python 不删 progress_file，Rust 在 `join()` 后才删 | 双端删除：Python `communicate()` 后删 + Rust `wait()` 后 `join()` 前删 |

### 技术决策

| 决策 | 原因 |
|------|------|
| 直接修补 torchaudio 源码而非 import hook | import hook 不传播到子进程（demucs） |
| 使用 soundfile 而非 torchcodec | soundfile 纯 Python + C 库，不需要 FFmpeg DLL |
| `CREATE_NO_WINDOW` | Windows GUI 应用 spawn 子进程时默认分配控制台 |
| 解析 demucs stdout 而非固定 ramp | demucs 处理时间差异大（30秒~10分钟） |
| 映射到 20-90% 而非 0-100% | 20% 之前是模型加载，90% 之后是文件写入 |
| 进度文件双端删除 | 防止任一端遗漏导致死锁 |
| Windows 用 threading 做非阻塞读取 | `select.select()` 不支持 Windows 管道 |
