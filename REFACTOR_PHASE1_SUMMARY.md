# Phase 1 Refactor Summary

## 概述

将 `src-tauri/src/lib.rs` 从 7325 行机械拆分为 7 个文件，共 7376 行（含新增模块声明和导入）。行为完全不变。

## 拆分 Commit 列表

| Commit | 描述 | 新增文件 | lib.rs 行变化 |
|--------|------|----------|---------------|
| `1fbac02` | models → models.rs | models.rs (390行) | -376 |
| `b6d4afe` | manifest → runtime/manifest.rs | runtime/manifest.rs (72行) | -64 |
| `0dc8b59` | capability → runtime/capability.rs | runtime/capability.rs (342行) | -372 |
| `437d9f2` | python → runtime/python.rs | runtime/python.rs (137行) | -142 |
| `ccc5ff6` | storage → storage/mod.rs | storage/mod.rs (64行) | -61 |
| `faabfd3` | events → events.rs | events.rs (99行) | -99 |

## 验证结果

- `cargo check`: 通过（仅 process_control.rs 预存 `unused variable: command` warning）
- `cargo test`: 4 passed, 0 failed
- `npm run build`: 通过（38 modules, 1.47s）

## 模块职责

| 模块 | 职责 | 行数 |
|------|------|------|
| `lib.rs` | Tauri commands、安装逻辑、Demucs/Whisper 运行、全局状态 | 6272 |
| `models.rs` | 纯数据结构/枚举定义 | 390 |
| `events.rs` | 进度/错误事件发送、cancel/job 状态读取 | 99 |
| `runtime/manifest.rs` | runtime-manifest.json 读取/解析 | 72 |
| `runtime/capability.rs` | GPU/CUDA/Torch 能力检测 | 342 |
| `runtime/python.rs` | Python 路径解析、命令执行 | 137 |
| `storage/mod.rs` | 路径计算、文件存储辅助 | 64 |

## 保持不变的业务规则

1. **GPU 三条件门控**: 用户勾选 + NVIDIA GPU 检测 + `torch.cuda.is_available()` 缺一不可
2. **Whisper CPU-only**: `WHISPER_DEVICE=cpu` 硬编码，不复用 Demucs GPU 状态
3. **Demucs CUDA 运行**: 仅在 GPU 三条件均满足时启用
4. **Windows 进程控制**: 分离管线 stdout/stderr 处理不回退
5. **不使用旧 song_* 目录**: 遵循新路径规范
6. **macOS freeze**: 不触碰 macOS 相关逻辑

## 后续禁止直接碰的高风险模块

| 模块/函数 | 原因 |
|-----------|------|
| `install_torch_with_cuda_detection` | GPU 安装核心决策，三条件门控在此实现 |
| `ensure_core_runtime_modules` | 运行时依赖安装，影响启动流程 |
| `generate_whisper_base_lyrics` | Whisper 调用入口，必须保持 CPU-only |
| `start_demucs_separation` / `run_demucs_separation` | Demucs 运行核心，CUDA 设备选择在此 |
| `process_control::spawn_in_own_process_group` | Windows 进程组管理，已知复杂度 |
| `SONGS` / `CANCEL_FLAGS` / `ACTIVE_JOB_TOKENS` | 全局状态，修改需全链路验证 |
| `src/App.tsx` 中 GPU checkbox 逻辑 | 前端三条件门控 UI，已修复 useCallback 依赖 |
