# Macaron Singer

本地歌曲处理与练唱应用：导入音频后做人声分离、歌词候选匹配、歌词编辑与同步播放，支持边播边改歌词时间轴。

- 技术栈：Tauri 2.x（Rust + React）、Demucs（htdemucs_ft）、TailwindCSS、Python 3.10 embedded
- 产品规格：[SPEC.md](./SPEC.md)
- Agent 交接与接手：[CLAUDE.md](./CLAUDE.md)
- 开发锚点与历史基线：[DEVELOPMENT_ANCHORS.md](./DEVELOPMENT_ANCHORS.md)

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
- 不会读取开发目录内的 `python/models`，用于验证"最小壳 + 首次自部署"链路

### Windows 便携版交付（ZIP，无安装器）

```bash
npm run win:portable
```

产物：`dist-portable/Macaron-Singer-Windows-Portable.zip`

使用方式：解压 ZIP → 运行 `forisfstools.exe` → 偏好设置 → 依赖与模型 → 一键安装运行环境。

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

src-tauri/src/
  lib.rs
  models.rs
  events.rs
  separation_queue.rs
  process_control.rs
  runtime/
  storage/

CLAUDE.md
DEVELOPMENT_ANCHORS.md
SPEC.md
README.md
```

## 后端命令（核心）

- `import_songs(paths)`
- `start_process(song_id)`
- `cancel_process(song_id)`
- `reprocess_song(song_id)`
- `get_songs()`
- `get_lyrics_document(song_id)`
- `save_lyrics_document(song_id, document)`
