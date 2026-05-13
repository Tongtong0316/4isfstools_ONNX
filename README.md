# Macaron Singer

本地歌曲处理与练唱应用：导入音频后做人声分离、歌词候选匹配、歌词编辑与同步播放，支持边播边改歌词时间轴。

## 交接摘要

- 当前处理链维持 Demucs 分离 + LRCLib 歌词候选 + 本地歌词文档（`lyrics.json` / `lyrics.lrc`）
- 取消逻辑已改为终止整个进程组，不再只杀单个 PID
- 如需给新 agent 接手，优先阅读 `HANDOFF.md`
- 平台维护策略：macOS 已冻结为可用基线，短期不再默认维护；后续未额外说明的平台工作均按 Windows 优先处理。详见 `MACOS_FROZEN_2026-05-14.md` 与 `PLATFORM_MAINTENANCE.md`。

## 当前能力

- 当前功能基线已固化，后续以小修小补为主
- 全局拖拽导入音频（无需进入特定导入页）
- 歌词功能保留为“候选搜索 + 手动选择 + 编辑保存”路线，不再包含本地听写/转写生成入口
- “搜索匹配歌词”现在只负责打开手动搜索面板；输入关键词后会从 LRCLib 拉取候选并展示列表，随后可手动应用到本地歌词文档
- 歌曲处理链：Demucs 分离 + LRCLib 候选匹配
- 歌词来源优先尝试 lrclib.net 的候选结果；候选弹窗支持手动关键词重搜
- 播放列表支持可折叠文件夹、拖拽分组、右键重命名、移动到文件夹、搜索匹配歌词（候选列表）
- 播放列表编辑/删除动作已统一为自定义对话框，不再依赖 `window.prompt` / `window.confirm`
- 播放列表“新建文件夹”已改为显式输入弹窗，不再依赖原生提示框
- 新建文件夹弹窗已按标准纵向布局修正，使用独立输入框与按钮行分层，减少输入与按钮拥挤感
- 输出歌词双格式：`lyrics.lrc` + `lyrics.json`
- 歌词面板支持：点击定位、行内编辑、即时自动保存、±100ms 微调
- 无论歌词来源是 `lyrics.json` 还是 `lyrics.lrc`，都统一进入同一可编辑歌词面板
- 当前歌词来源以 LRCLib 候选为主，支持手动关键词重搜与候选应用
- 头部统计已简化为“已收录 X 首”，数值对应当前可唱歌曲数量
- 已消除“LRC 走简化视图”的分支差异：LRC 也支持回中、双击编辑、去抖保存、±100ms、首尾留白
- 歌词滚动：播放时自动居中当前行，手动拖动后短暂让位，静止后自动回中；暂停时不强制回中
- 歌词可视区支持前后留白，首尾歌词在居中逻辑下也可拥有呼吸空间
- 歌词回中逻辑已升级为容器级精确居中（避免 `scrollIntoView` 在复杂滚动场景下失效）
- 歌词水平居中已修正：文本主层不再受左右辅助信息挤压偏移
- 播放器支持原唱/伴奏切换与歌词高亮同步
- 底部模式已重构为三按钮单选耦合：`原唱`（伴奏+人声）、`伴奏`（仅伴奏）、`人声`（仅人声）
- 支持空格键快捷播放/暂停（编辑输入时不拦截）
- 底部模式切换胶囊已做饱满化微调（更高、更宽、更易读）
- 播放主按钮已缩小，减少底部视觉压迫感
- 进度条与播放控制区间距已增大，操作层级更清晰
- 控制区与进度条距离再次拉开，并修复音量条可点击调节
- 控制区高度已上调并采用硬性间距，确保按键区稳定远离时间轴
- 时间轴与按钮区之间新增固定隔离层，避免后续样式变化导致再贴近
- 底部冗余留白已按反馈减半，保留分离感同时避免过空
- 底部留白再次减半，进一步压缩空白区
- 播放器已处理切歌/切模式时的 `AbortError` 误报问题
- 播放切歌时会先停掉旧音频，降低叠播和残留状态导致的不稳定
- 选歌播放改为先启动音频、歌词后台加载，减少“点击无反应”的体感
- 播放键增加“音频未初始化时回补当前歌曲”的兜底
- 原唱模式现在会先把伴奏轨与人声轨对齐到同一起点，再开始播放
- 原唱 / 人声 / 伴奏切换时会保留当前播放进度，不再回到开头
- 取消处理已改为按进程组终止，避免只杀单个 PID 导致后台继续跑完

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

## 文档同步规则（强制）

- 每完成一个小阶段（可运行、可验证的最小增量）后，必须同步更新：
  - `PROGRESS.md`：记录已完成项、验证结果、遗留问题
  - `SPEC.md`：更新能力边界与行为定义
  - `README.md`：更新对外可见能力与使用说明
- 代码实现与文档描述不一致时，以“先修文档再继续开发”为默认策略。
