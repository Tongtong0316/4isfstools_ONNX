# 固化记录 - v1.0.1 Windows 当前生产基线

## 日期

2026-05-18

## 固化基线

- 代码基线 commit: `fe6b7b2c5a358113670e3508418329d9b2d3ccd3`
- GitHub `main`: 本固化记录提交后应继续保持最新
- GitHub tag `v1.0.1`: 本固化记录提交后应移动到最新固化提交
- 版本号：`1.0.1`

## 本次固化范围

- Windows UI 主线当前状态
- 播放列表、偏好设置、播放器底部控制区、主题 token、全局文本安全区等前序 UI 优化
- 新增主题：`高级纲领`
- Windows 编译机已从 GitHub 拉取当前 `main` 并完成安装包构建

## Windows 产物

本地下载目录已拉取以下安装包：

- `/Users/suntong/Downloads/Macaron Singer_1.0.1_x64-setup.exe`
- `/Users/suntong/Downloads/Macaron Singer_1.0.1_x64_en-US.msi`

SHA-256：

- `Macaron Singer_1.0.1_x64-setup.exe`: `e28ff611001deda4b9ef6ae444db1f9a985cb68427a4691f0da5b203c96cf2bd`
- `Macaron Singer_1.0.1_x64_en-US.msi`: `0c1af111d9d94dd4b27c9dc258ee184aeb0802cf8f181002cc1c5327e2d1d9c3`

## 构建说明

- 编译机：`DESKTOP-LG6H7NK`
- 项目目录：`C:\Users\suntong\4isfstools`
- 编译机 `HEAD` 与 `v1.0.1` 均为 `fe6b7b2c5a358113670e3508418329d9b2d3ccd3`
- `npm run tauri build` 生成 MSI 成功，但后续 bundle 阶段一度因 `forisfstools.exe` / `msiexec.exe` 文件占用报错。
- 已结束相关进程后执行 `npm run tauri -- build --bundles nsis`，NSIS 安装包生成成功。
- Vite 在 Windows 编译机提示 Node `20.18.0` 低于推荐 `20.19+`，但未阻断本次构建。

## 当前注意事项

- macOS 仍按既定策略保持冻结参考线。
- Windows 是当前主开发线。
- 本次固化不包含本地未跟踪截图、临时 handoff 文件或构建产物。

---

# 交接文档 - 音频输出设备修复

## 修改概述

本次修改解决了两个问题：
1. **声音输出源下拉菜单** - 深色主题下选项文字与背景颜色接近导致看不清
2. **音频输出设备切换** - 选择了设备但声音仍从默认设备输出

## 修改文件

### `src/App.tsx`

#### 1. 下拉菜单样式修复 (约第1879行)

**问题**：原代码使用 Tailwind CSS 类名 `bg-white/[0.05]` 和 `text-[#f5f5f5]`，但在某些浏览器/系统组合下这些样式对 `<select>` 和 `<option>` 元素不生效，导致下拉菜单选项文字几乎是白色看不见。

**修复**：改用内联 `style` 属性直接设置背景色和文字颜色。

```tsx
// 修改前
<select
  className="... bg-white/[0.05] text-[#f5f5f5] ..."
>
  <option value="default">系统默认</option>
  <option key={d.deviceId} value={d.deviceId}>{d.label}</option>
</select>

// 修改后
<select
  style={{ backgroundColor: 'rgba(255,255,255,0.05)', color: '#f5f5f5' }}
  className="..."
>
  <option value="default" style={{ backgroundColor: '#1a1a2e', color: '#f5f5f5' }}>系统默认</option>
  <option key={d.deviceId} value={d.deviceId} style={{ backgroundColor: '#1a1a2e', color: '#f5f5f5' }}>{d.label}</option>
</select>
```

#### 2. `createAudioTrack` 函数修改 (约第401行)

**问题**：原代码通过 `applyAudioOutputDevice(audio)` 设置输出设备，但该函数依赖 `audioAnalyserContextRef.current`。在创建新音轨时，`audioAnalyserContext` 可能尚未创建（首次播放时），导致 `setSinkId` 无法被调用。

**修复**：在创建 `HTMLAudioElement` 时立即尝试调用 `audio.setSinkId()`，不依赖 AudioContext。

```tsx
// 修改前
const createAudioTrack = useCallback((src: string) => {
  const audio = new Audio();
  audio.src = src;
  audio.preload = "auto";
  audio.load();
  audio.volume = 1;
  void applyAudioOutputDevice(audio);  // 依赖尚未创建的 AudioContext
  return audio;
}, [applyAudioOutputDevice]);

// 修改后
const createAudioTrack = useCallback((src: string) => {
  const audio = new Audio();
  audio.src = src;
  audio.preload = "auto";
  audio.load();
  audio.volume = 1;
  const deviceId = audioOutputDeviceIdRef.current;
  if (deviceId && deviceId !== "default" && typeof audio.setSinkId === "function") {
    void audio.setSinkId(deviceId).catch((e) => console.warn("[audio] setSinkId failed:", e));
  }
  return audio;
}, []);
```

#### 3. `ensureAudioContextRunning` 函数修改 (约第518行)

**问题**：当 AudioContext 状态为 `suspended` 时调用 `context.resume()`，恢复后浏览器可能会将输出设备重置回系统默认，导致之前设置的输出设备失效。

**修复**：在 `context.resume()` 成功后重新应用 `setSinkId`。

```tsx
// 修改前
const ensureAudioContextRunning = useCallback(async () => {
  const context = audioAnalyserContextRef.current;
  if (!context) return;
  if (context.state === "suspended") {
    try {
      await context.resume();
    } catch (e) {
      console.error("Failed to resume audio context:", e);
    }
  }
}, []);

// 修改后
const ensureAudioContextRunning = useCallback(async () => {
  const context = audioAnalyserContextRef.current;
  if (!context) return;
  if (context.state === "suspended") {
    try {
      await context.resume();
    } catch (e) {
      console.error("Failed to resume audio context:", e);
    }
  }
  // Re-apply audio output device after context resumes
  const deviceId = audioOutputDeviceIdRef.current;
  const ctxWithSink = context as AudioContext & { setSinkId?: (id: string) => Promise<void> };
  if (deviceId && deviceId !== "default" && typeof ctxWithSink.setSinkId === "function") {
    try {
      await ctxWithSink.setSinkId(deviceId);
    } catch (e) {
      console.warn("[audio] setSinkId after resume failed:", e);
    }
  }
}, []);
```

## 技术背景

### Web Audio API 输出设备选择机制

1. **AudioContext.setSinkId()** - 可设置整个 AudioContext 的输出设备，但需要 AudioContext 已存在
2. **HTMLAudioElement.setSinkId()** - 可单独设置 HTMLAudioElement 的输出设备

### 问题根因

- 音轨创建时 AudioContext 不存在 → `applyAudioOutputDevice` 无法工作
- AudioContext.resume() 后浏览器重置输出设备 → 已设置的设备失效

### 修复策略

- 在 HTMLAudioElement 创建时立即调用 `setSinkId`
- 在 AudioContext 恢复后重新调用 `setSinkId`

## 相关代码位置

- `src/App.tsx:401-414` - `createAudioTrack`
- `src/App.tsx:518-538` - `ensureAudioContextRunning`
- `src/App.tsx:1879-1902` - 音频输出设备选择器 UI
---

# 交接文档 - UI 组件重构 (v1.0.1)

## 日期

2026-05-18

## 修改概述

本轮修改对三个 UI 组件进行了结构化重构，全部只改 UI 布局和样式，不改业务逻辑：

1. **删除确认弹窗** — 从粗糙 modal 重构为标准 DestructiveConfirmDialog
2. **播放列表分组 Header** — 压缩高度和视觉权重
3. **歌词候选弹窗** — 从单块布局拆分为 Header / SearchBar / CandidateList / Footer 四区

## 修改文件

| 文件 | 主要修改 |
|------|----------|
| `src/components/Playlist.tsx` | 删除确认弹窗 JSX + 分组 Header 样式 |
| `src/App.tsx` | 歌词候选弹窗完全重构 |
| `src/index.css` | 新增 danger tokens + destructive-dialog 样式 |

---

## 一、删除确认弹窗 (`src/components/Playlist.tsx`)

### 修改前问题

- 弹窗标题和正文贴边，无 content safe area
- 无 Header / Body / Footer 分层
- 删除按钮 `bg-[#ef4444]` 硬编码为危险红，主题适配差
- 按钮圆角 `rounded-full`，过于圆润
- 无 danger icon / 危险语义标识

### 修改后结构

```tsx
<div className="destructive-dialog">
  <div className="destructive-dialog-header">
    <div className="destructive-dialog-icon">
      {/* ⚠ alert-triangle SVG */}
    </div>
    <span className="destructive-dialog-title">删除歌曲</span>
  </div>
  <div className="destructive-dialog-body">
    <p className="primary-message">
      确认删除「<span className="song-name">{song.name}</span>」？
    </p>
    <p className="secondary-message">
      此操作会同时移除本地数据，删除后不可从本应用内恢复。
    </p>
  </div>
  <div className="destructive-dialog-footer">
    <button className="cancel-btn">取消</button>
    <button className="delete-btn" data-danger="true">删除</button>
  </div>
</div>
```

### 样式新增 (`src/index.css`)

```css
.destructive-dialog {
  width: min(calc(100vw - 48px), 520px);
  max-width: 560px;
  border-radius: 22px;
  border: 1px solid var(--dialogBorder);
  background: var(--dialogBg);
  box-shadow: var(--dialogShadow);
  overflow: hidden;
  z-index: 100;
}

.destructive-dialog-header {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 24px 28px 12px;
}

.destructive-dialog-icon {
  width: 36px;
  height: 36px;
  border-radius: 12px;
  background: var(--dangerSoft);
  display: flex;
  align-items: center;
  justify-content: center;
}

.destructive-dialog-title {
  font-size: 22px;
  font-weight: 800;
  color: var(--text-primary);
}

.destructive-dialog-body {
  padding: 0 28px 20px;
}

.destructive-dialog-footer {
  padding: 18px 28px 24px;
  border-top: 1px solid var(--dialogFooterBorder);
  display: flex;
  justify-content: flex-end;
  align-items: center;
  gap: 12px;
}

.destructive-dialog-footer .cancel-btn {
  height: 40px;
  min-width: 88px;
  padding: 0 18px;
  border-radius: 12px;
  background: var(--buttonSecondaryBg);
  color: var(--text-secondary);
  font-size: 14px;
  font-weight: 500;
  border: none;
  cursor: pointer;
}

.destructive-dialog-footer .delete-btn {
  height: 40px;
  min-width: 96px;
  padding: 0 20px;
  border-radius: 12px;
  background: var(--danger);
  color: var(--dangerText);
  font-size: 14px;
  font-weight: 600;
  border: none;
  cursor: pointer;
}
```

### 新增 Theme Tokens

在 `:root` 和各 `[data-theme]` 中新增：

```css
--danger: #ef4444;
--dangerHover: #dc2626;
--dangerActive: #b91c1c;
--dangerSoft: rgba(239, 68, 68, 0.12);
--dangerBorder: rgba(239, 68, 68, 0.28);
--dangerText: #ffffff;
--dialogBg: color-mix(in srgb, var(--bg-secondary) 96%, black);
--dialogBorder: var(--panel-accent-border);
--dialogShadow: 0 0 0 1px var(--panel-inner-border), 0 24px 70px rgba(0, 0, 0, 0.42), ...;
--dialogFooterBorder: var(--border-soft);
--buttonSecondaryBg: var(--button-bg);
--buttonSecondaryHoverBg: var(--button-hover-bg);
```

各主题 danger 颜色适配：
- 深色系 (aurora/graphite/studio/midnight/zero): `danger: #f87171`（较亮红，避免过刺）
- 浅色系 (daylight/paper/double/passion): `danger: #dc2626`（标准红，清晰可辨）

---

## 二、播放列表分组 Header (`src/components/Playlist.tsx`)

### 修改前问题

- 高度 52px，视觉权重过高
- 标题 18px font-semibold，接近主标题
- 数量使用 `ui-chip` 圆形 badge，显得过重
- 背景使用 `bg-card`，比歌曲卡片还抢眼

### 修改后参数

| 属性 | 修改前 | 修改后 |
|------|--------|--------|
| height | 52px | 36px |
| padding | 0 16px | 0 16px |
| bg | `bg-card` | `bg-secondary`（更轻） |
| left arrow | `text-base` | `text-xs` (~11px) |
| title | `text-[18px] font-semibold` | `text-[16px] font-bold` |
| count | `ui-chip` badge (30px pill) | `text-[14px] font-semibold` inline |
| gap | `gap-2.5` | `gap-2` |
| right chevron | `h-7 w-7 text-[18px]` | `h-6 w-6 text-[15px]` |

### 修改后结构

```tsx
<div style={{ height: 36, padding: "0 16px" }}>
  <div className="flex min-w-0 flex-1 items-center gap-2">
    <span className="text-[var(--text-muted)] text-xs leading-none">
      {isCollapsed ? "▸" : "▾"}
    </span>
    <span className="ui-text-ellipsis truncate text-[16px] font-bold leading-none text-[var(--text-primary)]">
      {folderName}
    </span>
    <span className="text-[14px] font-semibold text-[var(--text-muted)] leading-none">
      {folderSongs.length}
    </span>
  </div>
  <button className="flex h-6 w-6 items-center justify-center rounded-full text-[15px] text-[var(--text-muted)] ...">
    ›
  </button>
</div>
```

---

## 三、歌词候选弹窗 (`src/App.tsx`)

### 修改前问题

- 所有内容（Header + SearchBar + List + Footer）堆在一个 div 里，无结构分层
- 标题"选择歌词候选"贴近左上角，无 safe area
- 关闭按钮使用 `ui-button px-4`，视觉过重
- 搜索框和搜索按钮高度 44px (h-11)，过高
- 候选卡片左侧贴边，无 MetaColumn，badge 挤在一起
- 长歌词预览/乱码撑破布局
- 底部"取消"按钮贴右下角，无标准 footer
- Header 与 SearchBar、SearchBar 与内容区之间有横线分隔

### 修改后 DialogPanel 结构

```tsx
<div className="theme-aware-surface ..."
  style={{ width: "min(820px, calc(100vw - 64px))", maxHeight: "min(720px, calc(100vh - 64px))" }}>
  {/* Header */}
  <div className="grid grid-cols-[minmax(0,1fr)_auto] items-start gap-4 px-7 pt-6"
       style={{ padding: "24px 28px 14px" }}>
    <div>
      <div className="text-[24px] font-extrabold text-[var(--text-primary)]">选择歌词候选</div>
      <div className="ui-text-ellipsis mt-1.5 text-[14px] font-semibold text-[var(--text-secondary)]">{song.name}</div>
    </div>
    <button className="shrink-0 h-[32px] w-[32px] rounded-[8px] ..." aria-label="关闭">×</button>
  </div>

  {/* SearchBar */}
  <div className="grid grid-cols-[minmax(0,1fr)_88px] gap-3 items-center px-[28px]"
       style={{ padding: "0 28px 16px" }}>
    {/* SearchInput: h-[42px] rounded-[13px] */}
    {/* SearchButton: h-[42px] min-w-[88px] rounded-[13px] */}
  </div>

  {/* CandidateList */}
  <div className="flex-1 overflow-y-auto px-[28px] pb-[18px] flex flex-col gap-3"
       style={{ padding: "12px 28px 18px" }}>
    {candidates.map(candidate => (
      <button className="CandidateCard grid grid-cols-[minmax(0,1fr)_auto] gap-4 ...">
        {/* MainContent */}
        <div className="min-w-0 overflow-hidden">
          <div className="text-[16px] font-extrabold line-clamp-2">{candidate.title}</div>
          <div className="ui-text-ellipsis text-[13px] font-semibold">{artist}</div>
          <div className="line-clamp-4 overflow-wrap-anywhere" style={{ overflowWrap: "anywhere", wordBreak: "break-word" }}>
            {candidate.preview}
          </div>
          <div className="mt-3 text-[12px] text-[var(--text-muted)]">点击采用此候选</div>
        </div>
        {/* MetaColumn */}
        <div className="flex flex-col items-end gap-2 pt-1" style={{ minWidth: "84px", maxWidth: "128px" }}>
          <span className="h-[28px] rounded-[999px] border border-[var(--chip-border)] bg-[var(--chip-bg)] px-[10px] ...">
            {candidate.sourceLabel}
          </span>
          <span className="h-[28px] rounded-[999px] border border-[var(--border-soft)] bg-[var(--surface-muted)] px-[10px] ...">
            {candidate.score}
          </span>
        </div>
      </button>
    ))}
  </div>

  {/* Footer */}
  <div className="flex items-center justify-end gap-3 px-[28px] py-5"
       style={{ padding: "16px 28px 24px" }}>
    <button className="h-[40px] min-w-[88px] rounded-[12px] ...">取消</button>
  </div>
</div>
```

### 关键参数

| 区域 | 参数 |
|------|------|
| DialogPanel | 820px / 720px / 22px radius |
| Header | 24px 28px 14px |
| SearchBar | 0 28px 16px, grid 42px+88px |
| SearchInput | h-[42px], rounded-[13px] |
| SearchButton | h-[42px], min-w-[88px], rounded-[13px] |
| CandidateList | flex-1, 12px 28px 18px |
| CandidateCard | p-[16px_18px], min-h-[128px], rounded-[16px] |
| MetaColumn | 84-128px 固定宽度, flex-col |
| SourceBadge | h-[28px], rounded-pill, chip tokens |
| ScoreBadge | h-[28px], rounded-pill, surface-muted |
| Preview | line-clamp-4, overflow-wrap-anywhere, word-break:break-word |
| Footer | 16px 28px 24px, 无 border-top |

### 关闭按钮演进

| 阶段 | 样式 |
|------|------|
| 修改前 | `ui-button px-4` 灰色块 |
| 第一轮 | `h-[36px] px-[12px] rounded-[10px] bg-transparent` |
| 第二轮 | `h-[34px] px-[12px] rounded-[10px] ghost-button-hover-bg` |
| 第三轮 | `h-[32px] px-[8px] rounded-[8px]` × 文字 |
| 第四轮（最终） | `h-[32px] w-[32px] rounded-[8px] text-[18px]` × 符号，无胶囊底衬 |

### 分隔线移除

- Header: 移除 `border-b border-[var(--border-soft)]`
- SearchBar: 移除 `border-b` 和 `py-4`，改由独立 padding 控制间距
- Footer: 移除 `border-t border-[var(--border-soft)]`

---

## 四、Theme Tokens 新增汇总

所有新增 tokens 均通过 CSS 变量定义，不影响现有主题切换逻辑：

```css
/* Base tokens (:root) */
--danger: #ef4444;
--dangerHover: #dc2626;
--dangerActive: #b91c1c;
--dangerSoft: rgba(239, 68, 68, 0.12);
--dangerBorder: rgba(239, 68, 68, 0.28);
--dangerText: #ffffff;
--dialogBg: color-mix(in srgb, var(--bg-secondary) 96%, black);
--dialogBorder: var(--panel-accent-border);
--dialogShadow: ...;
--dialogFooterBorder: var(--border-soft);
--buttonSecondaryBg: var(--button-bg);
--buttonSecondaryHoverBg: var(--button-hover-bg);
```

### 各主题 danger 适配

| 主题 | danger | dangerSoft |
|------|--------|------------|
| 默认/深色系 | `#ef4444` | `rgba(239,68,68,0.12)` |
| aurora/graphite/studio/midnight/zero | `#f87171` | `rgba(248,113,113,0.15)` |
| daylight/paper | `#dc2626` | `rgba(220,38,38,0.1)` |
| passion/double | `#dc2626` | `rgba(220,38,38,0.1)` |

---

## 五、版本信息

- **本轮版本**: v1.0.1
- **提交**: `e459d3f` feat: UI refinements...
- **版本提交**: `20eb141` chore: bump version to v1.0.1
- **Release**: https://github.com/Tongtong0316/4isfstools/releases/tag/v1.0.1

---

## 六、禁止事项（重要）

以下修改在本轮中被明确禁止：

- ❌ 修改歌词搜索 API / 候选排序 / 过滤逻辑
- ❌ 修改删除业务逻辑和数据流程
- ❌ 修改分组折叠/展开逻辑
- ❌ 修改候选采纳逻辑
- ❌ 修改歌曲卡片样式
- ❌ 修改播放列表搜索框
- ❌ 修改主题切换逻辑
- ❌ 修改 theme 对象或 data-theme 属性
- ❌ hardcode 黑白颜色
- ❌ 使用负 margin 修复布局
- ❌ 使用 absolute 乱定位元素
