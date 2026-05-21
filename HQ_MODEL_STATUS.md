HQ 模型分离管线 — 交接文档
===========================

## 项目概述

Tauri 桌面应用（4isfstools / Macaron Singer），歌曲转卡拉 OK（人声/伴奏分离）。

## 涉及文件

| 文件 | 改动 |
|------|------|
| `src-tauri/src/separation/onnx_engine.rs` | 核心：自定义 ONNX 分离管线（替代 sherpa-onnx） |
| `src-tauri/src/lib.rs` | 模型路径选择、错误格式、队列 |
| `src-tauri/src/separation_queue.rs` | 队列添加 model_id 字段 |
| `src/App.tsx` | 模型选择持久化、传入 model_id |
| `python/bin/` (runtime) | onnxruntime、numpy、soundfile 等依赖 |

## 架构：分离管线

### 整体流程

```
前端 model_id → Rust 队列 → process_song_background →
  process_song_with_onnx_skeleton → run_onnx_separation
```

Rust 负责：模型选择、路径管理、Python 进程管理、状态通知。
Python 负责：音频加载、sherpa-onnx 分离、文件写回。

### 为什么现在走 sherpa_uvr

HQ 模型已切换为 `UVR-MDX-NET-Inst_HQ_5`。
当前主线默认与高级模型都走 `sherpa_uvr` 的官方 UVR 兼容路径，优先追平 UVR 听感。

### Python 脚本流程（`onnx_engine.rs:441-480`）

参数来源：`sys.argv[1]` ～ `sys.argv[7]`

1. `soundfile.read` 加载标准化后的音频（normalized_input.wav）
2. 单声道补为立体声
3. 构造 `sherpa_onnx.OfflineSourceSeparationConfig`
4. 选择 `provider` 并校验配置
5. 执行 `sp.process(sample_rate=..., samples=...)`
6. 将 `output.stems[0]` 写入 `vocals.wav`
7. 将 `output.stems[1]` 写入 `instrumental.wav`
8. 输出 `separator_result.json`，记录 `segmentCount` / `sampleRate` / `provider` 结果

## ModelConfig 配置系统

位置：`onnx_engine.rs:10-44`

```rust
struct ModelConfig {
    execution_backend: String, // "sherpa_uvr"
    output_target: String,  // "Voc" | "Inst"
    output_mode: String,    // "mask" | "direct" | "direct_plus_input_phase"
    chunk_size: u32,        // 256 (MDX-NET)
    n_fft: u32,
    hop_length: u32,
    dim_f: u32,
    model_id: String,
}
```

当前配置表（`get_model_config`）：

| model_id | execution_backend | output_target | output_mode | chunk_size | n_fft | hop_length | dim_f |
|----------|-------------------|---------------|-------------|------------|-------|------------|-------|
| `"default"` | sherpa_uvr | Voc | mask | 256 | 4096 | 1024 | 2048 |
| `"high_quality"` | sherpa_uvr | Inst | mask | 256 | 4096 | 1024 | 2048 |

默认模型 `UVR_MDXNET_9482.onnx` 的当前配置已固化：

- 执行器为 `sherpa_uvr`。
- 仍保持 `sample_rate=44100, n_fft=4096, hop_length=1024, win_length=4096, dim_f=2048, dim_t=256`。

### 新增模型步骤

1. 先把 ONNX 文件放到 runtime 目录
2. 在 `model_registry.rs` 注册路径
3. 在 `get_model_config()` 加一行配置
4. 前端加上选项，传入 model_id

## 参数推导策略

优先级：**metadata > ONNX 输入形状 > 硬编码 fallback**

| 参数 | 推导方式 |
|------|----------|
| `dim_f` | metadata 或 `sess.get_inputs()[0].shape[2]`，fallback 2560 |
| `n_fft` | metadata 或 `dim_f × 2` |
| `hop_length` | metadata 或 `n_fft / 4` |
| `chunk_size` | ModelConfig（不从文件推导） |
| `output_target` | ModelConfig（fallback：模型名含"Inst"→Inst，否则 Voc） |
| `output_mode` | ModelConfig（保留字段，当前 mainline 不再分支） |

## 元数据修复

`lib.rs::fix_onnx_model_metadata` — HQ 模型 ONNX 文件缺少 metadata，下载后注入。
- 触发时机：下载 HQ 模型时、模型已存在时重新点击下载
- 注入值：`n_fft=5120, hop_length=1280, dim_f=2560, win_length=5120` 等
- **注意**：win_length 曾错误为 4096，已修正为 5120。如果重新下载模型需确认

## 错误处理

- Python 脚本将结果 JSON 输出到 stdout，错误信息在 JSON 的 `error` 字段
- stderr 作为 fallback 兜底
- Rust 端提取 `error_code` + 第一行作为 UI 摘要（`lib.rs:5385-5406`）
- 完整错误日志写入 `<output_dir>/debug/separator_result.json`

## 已修复的其他问题

- **模型选择不生效**：model_id 从队列→worker→process 全链路传递（lib.rs、separation_queue.rs）
- **HQ 模型下载卡住**：`reqwest::blocking` 与 tokio 冲突，用 `spawn_blocking` 包裹
- **下载无超时**：connect_timeout=15s, timeout=300s
- **ready 歌曲无法删除**：delete_song 添加 ready 状态
- **导入歌曲 stuck pending**：前端 start_process 缺 modelId 参数
- **模型选择不持久化**：localStorage 读写

## 待办

- [ ] 验证默认模型分离质量是否与 sherpa-onnx 一致
- [ ] 验证 HQ5 模型分离质量是否达标
- [ ] `bootstrap_install_minimal` 也有 tokio 运行时冲突，尚未修复
- [ ] 如果后续添加 `UVR_MDXNET_KARA` 等新模型，在 `get_model_config()` 加一行即可
