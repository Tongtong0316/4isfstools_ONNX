# Windows 兼容性修复日志 (2026-05-11 ~ 2026-05-12)

## 问题描述

在 Windows 上运行 Macaron Singer 时遇到四个核心问题：
1. **人声分离（demucs）进度卡死** — 进度卡在 20%/40%，UI 无响应
2. **Python 控制台窗口频繁弹出** — 多个空的 cmd/python 窗口不断出现
3. **依赖下载时 FFmpeg 显示"状态未知"** — 应用无法定位已安装的 FFmpeg
4. **进度文件死锁** — demucs 完成后 Rust 端进度线程永不退出，阻塞主流程

## 根本原因分析

### 问题 1：demucs 进度卡死（两个子问题）

**子问题 A：torchaudio 2.11.0 不兼容**

错误链：
```
demucs 调用 torchaudio.load()
  → torchaudio 2.11.0 改用 torchcodec 后端
    → torchcodec 需要 FFmpeg 共享库 DLL（libtorchcodec_core*.dll）
      → 运行时只有 ffmpeg.exe CLI 工具，没有 DLL
        → ImportError: TorchCodec is required
          → demucs 退出，separator.py 报错
```

关键发现：
- `torchaudio 2.11.0` 不再支持 `set_audio_backend()` API
- `torchaudio 2.11.0` 的 `load()` 和 `save()` 直接调用 torchcodec，不 fallback 到 soundfile
- 安装 torchcodec 后仍然失败，因为需要 FFmpeg 共享库 DLL
- 卸载 torchcodec 后仍然失败，因为 torchaudio 直接 raise 而不是 fallback

尝试过的方案（均失败）：
1. 安装 torchcodec → 需要 FFmpeg 共享库 DLL，运行时没有
2. `sitecustomize.py` import hook → `__builtins__` 在非主模块中是 dict，hook 无法生效
3. `builtins.__import__` hook → 即使修复了 builtins 问题，import hook 不会传播到子进程
4. `.pth` 文件 → 此 Python 构建不处理 .pth 文件
5. `sitecustomize.py` → 只对当前进程生效，不传播到 demucs 子进程

最终方案：直接修补 `torchaudio/__init__.py` 源码，将 `load_with_torchcodec` 和 `save_with_torchcodec` 调用替换为 soundfile 实现。

**子问题 B：进度 ramp 封顶过低**

demucs 在 CPU 上处理 72MB WAV 文件（4 模型集成）需要约 5 分钟，但 progress ramp 只从 20% 线性增长到 40%（40 秒后封顶）。用户看到进度永远停在 40%，误以为程序卡死。

### 问题 2：控制台窗口

原因：Windows 上从 GUI 应用（Tauri）spawn 子进程时，每个子进程都会分配一个新的控制台窗口。

涉及的进程：
- Rust → separator.py（`Command::new(python_path)`）
- Python → demucs（`subprocess.Popen()`）
- Rust → `cmd /C where python`（Python 路径检测）
- Rust → `tar`、`powershell` 等系统命令

### 问题 3：FFmpeg 检测失败

原因：`resolve_ffmpeg_binary_path()` 只检查 PATH 和 macOS 路径，不检查 Windows 运行时目录 `runtime\ffmpeg\bin\ffmpeg.exe`。

### 问题 4：进度文件死锁

**根因**：Python 端写入 `separator_progress.json` 但从未删除，Rust 端进度监控线程靠检测文件消失来退出循环，但文件删除操作在 `progress_handle.join()` 之后才执行 → 线程永远不退出 → `join()` 永远阻塞 → 主流程卡死。

```
Python 写入 progress_file → demucs 完成 → Python 未删除 progress_file
Rust: child.wait() 返回 → fs::remove_file(在 join 之后) → 永远执行不到
Rust: progress_handle.join() → 线程循环读取 progress_file → 文件仍在 → 永不退出
```

## 修复方案

### 修复 1：torchaudio 兼容性（源码直接修补）

方案：在 `lib.rs` 中新增 `install_torchaudio_compat_patch()` 函数，直接修改 `torchaudio/__init__.py` 文件。

为什么用直接修补：
- import hook（sitecustomize.py / .pth）不会传播到子进程
- 直接修改源码对所有后续 Python 进程（包括 demucs 子进程）都生效
- 自动备份原文件为 `__init__.py.bak`

修补逻辑：
```rust
fn install_torchaudio_compat_patch(python_path: &Path) -> Result<(), String> {
    // 1. 通过 Python 找到 torchaudio/__init__.py 路径
    // 2. 检查是否已修补（包含 "soundfile as sf" 和 "sf.read"）
    // 3. 备份原文件为 __init__.py.bak
    // 4. 替换 `return load_with_torchcodec(...)` 为 soundfile 实现
    // 5. 替换 `return save_with_torchcodec(...)` 为 soundfile 实现
}
```

load 补丁内容：
```python
import soundfile as sf
data, samplerate = sf.read(str(uri), dtype="float32", start=frame_offset, stop=(frame_offset + num_frames if num_frames > 0 else None))
data = torch.from_numpy(data)
if data.ndim == 1:
    data = data.unsqueeze(0)
elif channels_first:
    data = data.T
return data, samplerate
```

save 补丁内容：
```python
import soundfile as sf
if src.ndim == 1:
    src = src.unsqueeze(0)
data = src.numpy()
if channels_first and data.shape[0] > 1:
    data = data.T
sf.write(str(uri), data, sample_rate, format=format)
```

集成点（在 `ensure_core_runtime_modules()` 中）：
- 所有三个返回路径（模块已存在 / 本地离线源成功 / pip 安装成功）都调用此函数
- soundfile 自动加入 pip 安装列表

### 修复 2：控制台窗口隐藏

方案：在 `process_control.rs` 中添加 Windows `CREATE_NO_WINDOW` 标志。

```rust
// process_control.rs
#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn configure_console_visibility(command: &mut Command) {
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
}

pub fn spawn_in_own_process_group(command: &mut Command) -> io::Result<Child> {
    // ... unix pre_exec ...
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    command.spawn()
}
```

应用范围：

| 调用位置 | 说明 |
|---------|------|
| `python_module_is_available` | Python 模块检查 |
| `whisper_model_probe` | Whisper 模型校验 |
| `has_nvidia_gpu` | GPU 检测 |
| `detect_windows_python_path` | `cmd /C where python` |
| `detect_nvidia_cuda_version` | `nvidia-smi` |
| FFmpeg 解压 | `powershell Expand-Archive` |
| Python 运行时解压 | `tar -xzf` |
| 模型解压 | `powershell Expand-Archive` |
| separator.py 内部 | `subprocess.Popen` 添加 `creationflags=0x08000000` |
| 扩散模型检查 | Python 扩散模型模块检查 |

### 修复 3：FFmpeg 路径检测

方案：在 `resolve_ffmpeg_binary_path()` 中增加 Windows 运行时目录检测。

```rust
fn resolve_ffmpeg_binary_path() -> Option<PathBuf> {
    let mut candidates = vec![PathBuf::from("ffmpeg")];
    // Windows: 优先检查运行时目录
    if cfg!(windows) {
        let runtime_ffmpeg = get_runtime_dir().join("ffmpeg").join("bin").join("ffmpeg.exe");
        candidates.insert(0, runtime_ffmpeg);
    }
    // macOS / Linux
    candidates.extend_from_slice(&[
        PathBuf::from("/opt/homebrew/bin/ffmpeg"),
        PathBuf::from("/usr/local/bin/ffmpeg"),
        PathBuf::from("/opt/local/bin/ffmpeg"),
    ]);
    // ... 逐个测试 ...
}
```

### 修复 4：实时进度跟踪（解析 demucs 输出）

问题：progress ramp 从 20% 线性增长到 40% 后封顶，demucs 实际需要 5+ 分钟。

方案：解析 demucs stdout 的实时进度条，映射到 app 的 20-90% 范围。

```python
import re as _re
_demucs_pct_re = _re.compile(r"^\s*(\d+)%\|")

# 逐行读取 demucs stdout
while True:
    if demucs_child.poll() is not None:
        break
    line = demucs_child.stdout.readline()  # 非阻塞读取
    if line:
        m = _demucs_pct_re.match(line)
        if m:
            pct = int(m.group(1))  # demucs 实际进度 0-100%
            app_pct = min(90, 20 + int(pct * 0.7))  # 映射到 20-90%
            write_progress(app_pct, f"人声分离中 ({app_pct}%)")
```

demucs 输出格式（progress bar）：
```
 42%|████████████████████████████████████████| 87.75/210.6 [02:10<03:07, 1.52s/seconds]
```

映射关系：
- demucs 0% → app 20%
- demucs 100% → app 90%
- demucs 完成后 → app 95%（后处理阶段）

Windows 兼容：使用 `threading.Thread` + 1 秒超时实现非阻塞读取（Windows 不支持 `select` 对管道操作）。

### 修复 5：进度文件死锁修复

问题：Python 端不删除 progress_file，Rust 端在 `join()` 之后才删除 → 死锁。

修复（双保险）：

**Python 端**：demucs `communicate()` 返回后立即删除 progress_file
```python
stdout, stderr = demucs_child.communicate()

# Remove progress file so Rust-side progress monitor thread exits its loop
try:
    os.remove(progress_file)
except Exception:
    pass
```

**Rust 端**：`child.wait()` 返回后、`progress_handle.join()` 之前删除 progress_file
```rust
let result = child.wait();

// Remove progress file first so the monitor thread exits its loop
let _ = fs::remove_file(&progress_file);

let stdout_bytes = stdout_handle.join().unwrap_or_default();
let stderr_bytes = stderr_handle.join().unwrap_or_default();
let _ = progress_handle.join();
```

## 修改的文件

### `src-tauri/src/process_control.rs`
- 添加 `#[cfg(windows)] use std::os::windows::process::CommandExt`
- 添加 `CREATE_NO_WINDOW` 常量（`0x08000000`）
- 新增 `configure_console_visibility()` 函数
- `spawn_in_own_process_group()` 在 Windows 上设置 `CREATE_NO_WINDOW`

### `src-tauri/src/lib.rs`

**新增函数**：
- `install_torchaudio_compat_patch(python_path)` — 直接修补 torchaudio 源码
- `find_return_block_end(content, start)` — 追踪括号深度找到 return 块结束位置

**修改函数**：
- `ensure_core_runtime_modules()` — 所有返回路径调用 `install_torchaudio_compat_patch`；soundfile 加入包列表
- `resolve_ffmpeg_binary_path()` — 增加 Windows 运行时目录检测

**separator.py 脚本（内嵌在 lib.rs 中）**：
- 添加 `creationflags=0x08000000` 到 demucs `subprocess.Popen`
- 添加 `separator_progress.json` 进度写入逻辑
- 添加 `write_progress()` 辅助函数
- 解析 demucs stdout 实时进度条（正则 `^\s*(\d+)%\|`）
- 映射 demucs 0-100% 到 app 20-90%
- demucs 结束后立即删除 progress_file
- Windows 使用 threading 实现非阻塞 stdout 读取

**Rust 端进度监控**：
- 进度文件监控线程（每 2 秒读取 `separator_progress.json`）
- `child.wait()` 返回后立即删除 progress_file（在 `join()` 之前）
- stdout/stderr 独立读取线程（避免管道死锁）

**`configure_console_visibility` 应用位置**：
- 所有 `Command::new()` 调用（python 检测、cmd /C where、tar、powershell 等）

## 验证结果

### 手动测试（Windows PowerShell）
```
# torchaudio 加载测试
Input: C:\Users\suntong\Desktop\isis_临渊_remix.wav
Output: shape=torch.Size([2, 9072432]), sr=44100

# demucs 分离测试
$ python -m demucs --two-stems=vocals -n htdemucs_ft -o output --device cpu test.wav
Exit: 0
Output: vocals.wav + no_vocals.wav
Duration: ~5 minutes (CPU, 72MB WAV, 4-model ensemble)
```

### 应用内测试
- Python 控制台窗口：已完全消除
- demucs 人声分离：成功完成（exit code 0）
- 依赖下载：soundfile 自动安装
- 进度显示：从 20% 实时增长到 90%（不再卡在 40%）

### 编译产物
```
Macaron Singer_0.1.0_x64-setup.exe (25.3 MB)
Macaron Singer_0.1.0_x64_en-US.msi
```

## 部署注意事项

1. **soundfile 依赖**：新安装时自动通过 pip 安装
2. **torchaudio 补丁**：在 `ensure_core_runtime_modules()` 首次运行时自动创建
3. **已安装用户**：需要重新运行 app，`install_torchaudio_compat_patch` 会在下次检查依赖时自动执行
4. **跨平台**：所有 Windows 特定代码用 `#[cfg(windows)]` 隔离，不影响 macOS
5. **备份文件**：torchaudio 原始 `__init__.py` 备份为 `__init__.py.bak`，如需恢复可手动替换

## 技术决策记录

| 决策 | 原因 |
|------|------|
| 直接修补 torchaudio 源码而非 import hook | import hook 不传播到子进程（demucs） |
| 使用 soundfile 而非 torchcodec | soundfile 纯 Python + C 库，不需要 FFmpeg DLL |
| `CREATE_NO_WINDOW` 而非隐藏窗口 | Windows GUI 应用 spawn 子进程时默认分配控制台 |
| 解析 demucs stdout 而非固定 ramp | demucs 处理时间差异大（30秒~10分钟），固定 ramp 无法反映真实进度 |
| 映射到 20-90% 而非 0-100% | 20% 之前是模型加载，90% 之后是文件写入，中间才是实际分离 |
| 进度文件双端删除 | Python 端先删 + Rust 端在 join 前删，防止任一端遗漏导致死锁 |
| Windows 用 threading 做非阻塞读取 | `select.select()` 不支持 Windows 管道 |
| stdout/stderr 独立线程读取 | 避免管道缓冲区满导致的死锁 |
