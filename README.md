# Macaron Singer

本地歌曲处理与练唱应用：导入音频后做人声分离、歌词候选匹配、歌词编辑与同步播放，支持边播边改歌词时间轴。

## 交接摘要

- 当前处理链维持 Demucs 分离 + LRCLib 歌词候选 + 本地歌词文档（`lyrics.json` / `lyrics.lrc`）
- 取消逻辑已改为终止整个进程组，不再只杀单个 PID
- 如需给新 agent 接手，优先阅读 `HANDOFF.md`
- 平台维护策略：macOS 已冻结为可用基线，短期不再默认维护；后续未额外说明的平台工作均按 Windows 优先处理。详见 `MACOS_FROZEN_2026-05-14.md` 与 `PLATFORM_MAINTENANCE.md`。


## 技术栈

- Tauri 2.x（Rust + React）
- Demucs（htdemucs_ft）
- TailwindCSS
- Python 3.10 embedded

## 开发

```bash
npm install
npm run tauri dev
npm run tauri build
```

### 隔离测试环境（无依赖/无模型）

```bash
npm run tauri:dev:isolated
```
- 启动时会启用 `FORISFSTOOLS_ISOLATED=1`
- 使用独立数据目录 `FORISFSTOOLS_DATA_DIR=/tmp/forisfstools-isolated-runtime`
- 不会读取开发目录内的 `python/models`，用于验证“最小壳 + 首次自部署”链路

### Windows 便携版交付（ZIP，无安装器）

在 Windows 机器上执行：

```bash
npm run win:portable
```

产物：

- `dist-portable/Macaron-Singer-Windows-Portable.zip`

使用方式：

1. 解压 ZIP
2. 运行 `forisfstools.exe`
3. 打开「偏好设置 -> 依赖与模型」
4. 点击「一键安装运行环境」补齐依赖

说明：

- 该便携包不走安装器，可直接解压运行
- 依赖补齐优先本地离线源，在线补齐仅使用中国大陆可达软件源
- 模型补齐读取 `runtime-manifest.json`（优先 `runtime/runtime-manifest.json`，其次应用资源目录，最后项目根目录）
- manifest 支持按平台配置（`platforms.macos` / `platforms.windows`），模型条目支持 `url + sha256 + note`
- 下载源仅接受大陆优先主机策略；模型归档下载后会执行 SHA256 校验（若已填写），再解压到本地 runtime
- 当前已验证可访问源（示例）：
  - Whisper base: `https://hf-mirror.com/Systran/faster-whisper-base/resolve/main/...`
  - Demucs htdemucs_ft: `https://dl.fbaipublicfiles.com/demucs/hybrid_transformer/...`

## 主要目录

```text
src/
  App.tsx
  components/
    Playlist.tsx
    lyrics/LyricsPanel.tsx
  types/
    index.ts
    lyrics.ts
  utils/lyrics.ts

src-tauri/src/lib.rs
README.md
SPEC.md
PROGRESS.md
HANDOFF.md
```

## 后端命令（核心）

- `import_songs(paths)`
- `start_process(song_id)`
- `cancel_process(song_id)`
- `reprocess_song(song_id)`
- `get_songs()`
- `get_lyrics_document(song_id)`
- `save_lyrics_document(song_id, document)`
