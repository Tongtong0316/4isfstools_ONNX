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