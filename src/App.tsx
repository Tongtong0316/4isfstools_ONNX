import { useState, useRef, useEffect, useCallback } from "react";
import type { CSSProperties } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import Playlist from "./components/Playlist";
import VocalWaveformPreview, { buildWaveformPeaks } from "./components/VocalWaveformPreview";
import { Song, ProcessingStage, ProcessingStatus } from "./types";
import LyricsPanel from "./components/lyrics/LyricsPanel";
import type { LyricDocument } from "./types/lyrics";

const MEDIA_IMPORT_EXTENSIONS = [
  "mp3",
  "wav",
  "flac",
  "ape",
  "m4a",
  "ogg",
  "aac",
  "mp4",
  "mov",
  "mkv",
  "webm",
  "avi",
  "m4v",
  "mpg",
  "mpeg",
  "3gp",
  "3g2",
  "ts",
  "m2ts",
  "mts",
  "vob",
  "wmv",
  "asf",
  "flv",
  "f4v",
  "ogv",
  "rmvb",
  "qt",
  "mxf",
];

function AppSearchIcon({ className = "" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="m17.2 17.2 3.3 3.3M10.8 18a7.2 7.2 0 1 1 0-14.4 7.2 7.2 0 0 1 0 14.4Z" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" />
    </svg>
  );
}

type LyricsCandidate = {
  id: string;
  source: string;
  sourceLabel: string;
  title: string;
  artist: string | null;
  album: string | null;
  score: number;
  synced: boolean;
  preview: string;
  document: LyricDocument;
};

type GeneratedLyricsDraftResult = {
  lyricsPath: string;
  document: LyricDocument;
};

type FileStorageSettings = {
  instrumentalRoot: string;
  vocalsRoot: string;
  lyricsRoot: string;
};

type RuntimeHealthCheck = {
  name: string;
  ok: boolean;
  severity: "info" | "warning" | "error";
  detail: string | null;
};

type RuntimeHealthReport = {
  level: "ready" | "warning" | "error";
  label: string;
  detail: string;
  torchCudaAvailable: boolean;
  selectedDevice: "cpu" | "cuda" | string;
  torchVersion: string | null;
  torchCudaVersion: string | null;
  torchCudaDeviceName: string | null;
  hasNvidiaGpu: boolean;
  nvidiaDriverVisible: boolean;
  nvidiaDriverCudaVersion: string | null;
  checks: RuntimeHealthCheck[];
};

type BootstrapStatus = {
  runtimeReady: boolean;
  demucsModelsReady: boolean;
  whisperBaseReady: boolean;
  ffmpegReady: boolean;
  canRunCore: boolean;
  torchCudaAvailable: boolean;
  selectedDevice: "cpu" | "cuda" | string;
  torchVersion: string | null;
  torchCudaVersion: string | null;
  torchCudaDeviceName: string | null;
  hasNvidiaGpu: boolean;
  nvidiaDriverVisible: boolean;
  nvidiaDriverCudaVersion: string | null;
  detail: string;
};

type SettingsPane = "runtime" | "audioOutput" | "paths" | "appearance" | "about";

type ColorThemeId = "graphite" | "aurora" | "studio" | "midnight" | "daylight" | "paper" | "passion" | "double" | "zero" | "manifesto";

const COLOR_THEMES: Array<{
  id: ColorThemeId;
  name: string;
  description: string;
  bg: string;
  card: string;
  accent: string;
  text: string;
}> = [
  {
    id: "graphite",
    name: "石墨夜色",
    description: "低眩光深色，适合长时间编辑。",
    bg: "#0b0b0d",
    card: "#202026",
    accent: "#8b5cf6",
    text: "#fafafa",
  },
  {
    id: "aurora",
    name: "青兔魔女",
    description: "冷静、清晰，强调音频状态。",
    bg: "#090b10",
    card: "#1a2230",
    accent: "#14b8a6",
    text: "#f8fafc",
  },
  {
    id: "studio",
    name: "录音棚暖调",
    description: "暖色强调但保留足够对比度。",
    bg: "#0d0f12",
    card: "#22262b",
    accent: "#f97316",
    text: "#fff7ed",
  },
  {
    id: "midnight",
    name: "午夜蓝调",
    description: "深蓝基调，适合暗光环境。",
    bg: "#070b13",
    card: "#172235",
    accent: "#38bdf8",
    text: "#f8fafc",
  },
  {
    id: "daylight",
    name: "日间清爽",
    description: "浅色高对比，适合明亮环境。",
    bg: "#f4f6fb",
    card: "#ffffff",
    accent: "#2563eb",
    text: "#111827",
  },
  {
    id: "paper",
    name: "纸面暖白",
    description: "柔和浅色，减少白底刺眼感。",
    bg: "#f7f4ef",
    card: "#fffaf2",
    accent: "#0f766e",
    text: "#1f2933",
  },
  {
    id: "passion",
    name: "慵倦晚霞",
    description: "粉灰与雾蓝的浅色体系，柔和但保持清晰对比。",
    bg: "#f7eef1",
    card: "#e8eff7",
    accent: "#527396",
    text: "#1f2937",
  },
  {
    id: "double",
    name: "津韵Double",
    description: "白灰清爽，适合长时间日间使用。",
    bg: "#f3f4f6",
    card: "#ffffff",
    accent: "#64748b",
    text: "#111827",
  },
  {
    id: "zero",
    name: "零度天堂",
    description: "深蓝夜空与星光金黄，冷冽梦幻的夜间主题。",
    bg: "#07111f",
    card: "#13233d",
    accent: "#facc15",
    text: "#f8fafc",
  },
  {
    id: "manifesto",
    name: "高级纲领",
    description: "青绿主调与紫色强调，黑色基底配合浅紫辅助。",
    bg: "#000000",
    card: "#17101d",
    accent: "#A2DA5A",
    text: "#f8fafc",
  },
];

const APP_VERSION = "1.0.1";

const SETTINGS_NAV_ITEMS: Array<{
  pane: SettingsPane;
  label: string;
  hint: string;
  icon: string;
}> = [
  { pane: "runtime", label: "运行环境", hint: "检测依赖、模型与 GPU 状态", icon: "◎" },
  { pane: "audioOutput", label: "音频输出", hint: "选择播放设备", icon: "◍" },
  { pane: "paths", label: "保存路径", hint: "分离文件保存位置", icon: "▣" },
  { pane: "appearance", label: "外观色彩", hint: "主题配色与可读性", icon: "✧" },
  { pane: "about", label: "关于", hint: "版本、声明与鸣谢", icon: "i" },
];

const SETTINGS_PAGE_COPY: Record<SettingsPane, { title: string; description: string }> = {
  runtime: {
    title: "运行环境",
    description: "检测依赖、模型与 GPU 状态，确保核心功能可以正常运行。",
  },
  audioOutput: {
    title: "音频输出",
    description: "选择音频播放设备，切换后立即生效。",
  },
  paths: {
    title: "保存路径",
    description: "设置伴奏、人声和歌词文件的保存位置，保存后可选择迁移历史文件。",
  },
  appearance: {
    title: "外观色彩",
    description: "选择适合当前环境的主题配色，保证正文与控件拥有足够对比度。",
  },
  about: {
    title: "关于",
    description: "版本信息、使用声明、开源项目与鸣谢。",
  },
};

const RUNTIME_CHECK_NAMES = [
  "Python",
  "FFmpeg",
  "Torch",
  "人声分离",
  "SoundFile",
  "AI 听写草稿",
  "NVIDIA GPU",
  "Torch CUDA",
];

type TrackGraph = {
  source: MediaElementAudioSourceNode;
  gain: GainNode;
  analyser: AnalyserNode;
};

type TrackLevels = {
  instrumental: number;
  vocals: number;
};

function App() {
  type PlaybackMode = "original" | "instrumental" | "vocals";
  const isDesktopRuntime = typeof window !== "undefined" && (
    "__TAURI_INTERNALS__" in window || "__TAURI__" in window
  );
  const isWindowsRuntime = typeof navigator !== "undefined"
    && /Win32|Win64|Windows/i.test(`${navigator.platform} ${navigator.userAgent}`);
  const [songs, setSongs] = useState<Song[]>([]);
  const [currentSong, setCurrentSong] = useState<Song | null>(null);
  const [playerState, setPlayerState] = useState<"idle" | "playing" | "paused">("idle");
  const [currentTime, setCurrentTime] = useState(0);
  const [volume, setVolume] = useState(80);
  const [playbackMode, setPlaybackMode] = useState<PlaybackMode>("instrumental");

  const formatTime = (ms: number) => {
    const s = Math.floor(ms / 1000);
    return `${Math.floor(s / 60)}:${(s % 60).toString().padStart(2, "0")}`;
  };
  const [lyricsDoc, setLyricsDoc] = useState<LyricDocument | null>(null);
  const [playbackError, setPlaybackError] = useState<string | null>(null);
  const [lyricsCandidates, setLyricsCandidates] = useState<LyricsCandidate[] | null>(null);
  const [lyricsCandidateSong, setLyricsCandidateSong] = useState<Song | null>(null);
  const [lyricsCandidateError, setLyricsCandidateError] = useState<string | null>(null);
  const [lyricsSearchQuery, setLyricsSearchQuery] = useState("");
  const [lyricsCandidateLoading, setLyricsCandidateLoading] = useState(false);
  const [lyricsCandidateOpen, setLyricsCandidateOpen] = useState(false);
  const [whisperDraftLoadingSongId, setWhisperDraftLoadingSongId] = useState<string | null>(null);
  const [whisperDraftError, setWhisperDraftError] = useState<string | null>(null);
  const [lyricsImportLoadingSongId, setLyricsImportLoadingSongId] = useState<string | null>(null);
  const [lyricsImportError, setLyricsImportError] = useState<string | null>(null);
  const [fileStorageSettings, setFileStorageSettings] = useState<FileStorageSettings | null>(null);
  const [fileStorageSettingsOpen, setFileStorageSettingsOpen] = useState(false);
  const [settingsPane, setSettingsPane] = useState<SettingsPane>("runtime");
  const [fileStorageSettingsSaving, setFileStorageSettingsSaving] = useState(false);
  const [fileStorageSettingsMessage, setFileStorageSettingsMessage] = useState<string | null>(null);
  const [runtimeHealth, setRuntimeHealth] = useState<RuntimeHealthReport | null>(null);
  const [bootstrapStatus, setBootstrapStatus] = useState<BootstrapStatus | null>(null);
  const [bootstrapInstalling, setBootstrapInstalling] = useState(false);
  const [bootstrapMessage, setBootstrapMessage] = useState<string | null>(null);
  const themeStorageKey = "4isfstools.color_theme";
  const [colorTheme, setColorTheme] = useState<ColorThemeId>(() => {
    if (typeof window === "undefined") return "graphite";
    try {
      const stored = window.localStorage.getItem(themeStorageKey);
      return COLOR_THEMES.some((theme) => theme.id === stored) ? stored as ColorThemeId : "graphite";
    } catch {
      return "graphite";
    }
  });

  const runtimeSelectedDevice =
    bootstrapStatus?.selectedDevice ??
    runtimeHealth?.selectedDevice ??
    "cpu";
  const runtimeTorchVersion = bootstrapStatus?.torchVersion ?? runtimeHealth?.torchVersion ?? null;
  const runtimeTorchCudaVersion =
    bootstrapStatus?.torchCudaVersion ?? runtimeHealth?.torchCudaVersion ?? null;
  const runtimeTorchCudaDeviceName =
    bootstrapStatus?.torchCudaDeviceName ?? runtimeHealth?.torchCudaDeviceName ?? null;
  const runtimeHasNvidiaGpu = bootstrapStatus?.hasNvidiaGpu ?? runtimeHealth?.hasNvidiaGpu ?? false;
  const runtimeTorchCudaAvailable =
    bootstrapStatus?.torchCudaAvailable ?? runtimeHealth?.torchCudaAvailable ?? false;
  const runtimeDeviceLabel = runtimeTorchCudaAvailable ? "GPU 运行" : "CPU 运行";
  const runtimeDeviceTitle = runtimeTorchCudaAvailable
    ? `Torch CUDA 可用${runtimeTorchCudaDeviceName ? `：${runtimeTorchCudaDeviceName}` : ""}`
    : runtimeHasNvidiaGpu
      ? "已检测到 NVIDIA GPU，但 Torch CUDA 暂不可用，当前使用 CPU"
      : "未检测到 NVIDIA GPU，当前使用 CPU";
  const runtimeChecks = runtimeHealth?.checks ?? [];
  const displayedRuntimeChecks: RuntimeHealthCheck[] = [
    ...RUNTIME_CHECK_NAMES.map((name) => (
      runtimeChecks.find((check) => check.name === name) ?? {
        name,
        ok: false,
        severity: "info" as const,
        detail: isDesktopRuntime ? "等待检测结果" : "桌面运行时未连接",
      }
    )),
    ...runtimeChecks.filter((check) => !RUNTIME_CHECK_NAMES.includes(check.name)),
  ];
  const runtimeCheckCountLabel = `${runtimeChecks.length}/${RUNTIME_CHECK_NAMES.length}`;

  useEffect(() => {
    document.documentElement.dataset.theme = colorTheme;
    try {
      window.localStorage.setItem(themeStorageKey, colorTheme);
    } catch {
      // ignore persistence failures
    }
  }, [colorTheme]);

  const audioRef = useRef<HTMLAudioElement | null>(null);
  const originalAudioRef = useRef<HTMLAudioElement | null>(null);
  const lyricsSaveTimerRef = useRef<number | null>(null);
  const currentSongRef = useRef<Song | null>(null);
  const lyricsLoadSeqRef = useRef(0);
  const waveformLoadSeqRef = useRef(0);
  const playbackOpRef = useRef(0);
  const audioAnalyserContextRef = useRef<AudioContext | null>(null);
  const audioGraphRef = useRef<{
    instrumental?: TrackGraph;
    vocals?: TrackGraph;
  }>({});
  const [trackLevels, setTrackLevels] = useState<TrackLevels>({
    instrumental: 0,
    vocals: 0,
  });
  const playbackMonitorRef = useRef({
    lastTime: 0,
    lastAt: 0,
    phase: 0,
  });
  const [vocalWaveformPeaks, setVocalWaveformPeaks] = useState<number[] | null>(null);
  const [vocalWaveformLoading, setVocalWaveformLoading] = useState(false);
  const [vocalWaveformError, setVocalWaveformError] = useState<string | null>(null);
  const [vocalWaveformEnabled, setVocalWaveformEnabled] = useState(true);

  // Audio output device
  const audioOutputStorageKey = "4isfstools.audio_output_device";
  const [audioOutputDeviceId, setAudioOutputDeviceId] = useState<string>(() => {
    if (typeof window === "undefined") return "default";
    try {
      return window.localStorage.getItem(audioOutputStorageKey) || "default";
    } catch { return "default"; }
  });
  const [audioOutputDevices, setAudioOutputDevices] = useState<Array<{ deviceId: string; label: string }>>([]);
  const [audioOutputSupport, setAudioOutputSupport] = useState<"unknown" | "supported" | "unsupported">("unknown");
  const audioOutputDeviceIdRef = useRef(audioOutputDeviceId);
  audioOutputDeviceIdRef.current = audioOutputDeviceId;

  const unlockAudioOutputDeviceLabels = useCallback(async () => {
    try {
      if (!navigator.mediaDevices?.getUserMedia) {
        return false;
      }
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      stream.getTracks().forEach((track) => track.stop());
      return true;
    } catch {
      return false;
    }
  }, []);

  const refreshAudioOutputDevices = useCallback(async () => {
    try {
      if (!navigator.mediaDevices?.enumerateDevices) {
        setAudioOutputSupport("unsupported");
        return;
      }
      await unlockAudioOutputDeviceLabels();
      const devices = await navigator.mediaDevices.enumerateDevices();
      const outputs = devices
        .filter((d) => d.kind === "audiooutput")
        .map((d) => ({ deviceId: d.deviceId, label: d.label || d.deviceId }));
      setAudioOutputDevices(outputs);
      // Probe support: check if AudioContext.setSinkId or HTMLAudioElement.setSinkId exists
      const probe = new Audio();
      if (typeof AudioContext !== "undefined" && "setSinkId" in AudioContext.prototype) {
        setAudioOutputSupport("supported");
      } else if (typeof probe.setSinkId === "function") {
        setAudioOutputSupport("supported");
      } else {
        setAudioOutputSupport("unsupported");
      }
    } catch {
      setAudioOutputSupport("unsupported");
    }
  }, [unlockAudioOutputDeviceLabels]);

  const getRequestedAudioSinkId = useCallback(() => {
    const deviceId = audioOutputDeviceIdRef.current;
    return deviceId && deviceId !== "default" ? deviceId : "";
  }, []);

  const applyAudioOutputDevice = useCallback(async (audio: HTMLAudioElement) => {
    const sinkId = getRequestedAudioSinkId();
    // Prefer AudioContext.setSinkId (routes the Web Audio graph when active).
    const ctx = audioAnalyserContextRef.current as (AudioContext & { setSinkId?: (id: string) => Promise<void> }) | null;
    if (ctx && typeof ctx.setSinkId === "function") {
      try {
        await ctx.setSinkId(sinkId);
      } catch (e) {
        console.warn("[audio] AudioContext.setSinkId failed:", e);
      }
    }
    // Also apply to the media element for direct-output fallback.
    try {
      if (typeof audio.setSinkId === "function") {
        await audio.setSinkId(sinkId);
      }
    } catch (e) {
      console.warn("[audio] setSinkId failed, using default output:", e);
    }
  }, [getRequestedAudioSinkId]);

  const applyToAllAudioOutputs = useCallback(async () => {
    const sinkId = getRequestedAudioSinkId();
    // Apply to AudioContext if available.
    const ctx = audioAnalyserContextRef.current as (AudioContext & { setSinkId?: (id: string) => Promise<void> }) | null;
    if (ctx && typeof ctx.setSinkId === "function") {
      try { await ctx.setSinkId(sinkId); } catch { /* fallback below */ }
    }
    // Apply to active HTMLAudioElements
    if (audioRef.current) void applyAudioOutputDevice(audioRef.current);
    if (originalAudioRef.current) void applyAudioOutputDevice(originalAudioRef.current);
  }, [applyAudioOutputDevice, getRequestedAudioSinkId]);

  useEffect(() => {
    try {
      window.localStorage.setItem(audioOutputStorageKey, audioOutputDeviceId);
    } catch { /* ignore */ }
    // Apply immediately to all active outputs
    void applyToAllAudioOutputs();
  }, [audioOutputDeviceId, applyToAllAudioOutputs]);

  useEffect(() => {
    if (fileStorageSettingsOpen && settingsPane === "audioOutput") {
      void refreshAudioOutputDevices();
    }
  }, [fileStorageSettingsOpen, settingsPane, refreshAudioOutputDevices]);

  const readySongCount = songs.filter((s) => s.status === "ready").length;


  const isBenignAbortError = (error: unknown) => {
    if (!error) return false;
    if (typeof error === "object" && error !== null) {
      const maybeName = (error as { name?: string }).name;
      const maybeMessage = String((error as { message?: string }).message || "");
      if (maybeName === "AbortError") return true;
      if (maybeMessage.toLowerCase().includes("operation was aborted")) return true;
    }
    return false;
  };

  const applyModeRouting = useCallback((vol: number, mode: PlaybackMode) => {
    const instrumentalGraph = audioGraphRef.current.instrumental;
    const vocalsGraph = audioGraphRef.current.vocals;
    const instrumentalGain = mode === "vocals" ? 0 : (vol / 100);
    const vocalsGain = mode === "instrumental" ? 0 : (vol / 100);
    if (instrumentalGraph) {
      instrumentalGraph.gain.gain.setTargetAtTime(instrumentalGain, audioAnalyserContextRef.current?.currentTime ?? 0, 0.01);
    } else if (audioRef.current) {
      audioRef.current.volume = instrumentalGain;
    }
    if (vocalsGraph) {
      vocalsGraph.gain.gain.setTargetAtTime(vocalsGain, audioAnalyserContextRef.current?.currentTime ?? 0, 0.01);
    } else if (originalAudioRef.current) {
      originalAudioRef.current.volume = vocalsGain;
    }
  }, []);

  const estimatePlaybackLevel = useCallback((kind: keyof TrackLevels) => {
    if (playerState !== "playing") return 0;
    const source =
      kind === "vocals"
        ? (originalAudioRef.current || audioRef.current)
        : (audioRef.current || originalAudioRef.current);
    if (!source || source.paused || source.ended || source.readyState < HTMLMediaElement.HAVE_CURRENT_DATA) {
      return 0;
    }
    const audibleByMode =
      kind === "instrumental"
        ? playbackMode !== "vocals"
        : playbackMode !== "instrumental";
    if (!audibleByMode || volume <= 0) return 0;

    if (kind === "vocals" && vocalWaveformPeaks?.length && currentSong?.duration) {
      const ratio = Math.max(0, Math.min(0.999, (source.currentTime * 1000) / currentSong.duration));
      const index = Math.floor(ratio * vocalWaveformPeaks.length);
      return Math.min(0.5, Math.max(0.02, vocalWaveformPeaks[index] * 0.5)) * (volume / 100);
    }

    playbackMonitorRef.current.phase = (playbackMonitorRef.current.phase + 0.37) % (Math.PI * 2);
    return (0.08 + Math.abs(Math.sin(playbackMonitorRef.current.phase)) * 0.12) * (volume / 100);
  }, [currentSong?.duration, playbackMode, playerState, vocalWaveformPeaks, volume]);

  const destroyTrackGraphs = useCallback(() => {
    const graphEntries = [
      audioGraphRef.current.instrumental,
      audioGraphRef.current.vocals,
    ];
    for (const graph of graphEntries) {
      if (!graph) continue;
      try {
        graph.source.disconnect();
      } catch {
        /* noop */
      }
      try {
        graph.gain.disconnect();
      } catch {
        /* noop */
      }
      try {
        graph.analyser.disconnect();
      } catch {
        /* noop */
      }
    }
    audioGraphRef.current = {};
  }, []);

  const stopAllAudio = useCallback(() => {
    destroyTrackGraphs();
    if (audioRef.current) {
      audioRef.current.pause();
      audioRef.current.currentTime = 0;
      audioRef.current.removeAttribute("src");
      audioRef.current.load();
    }
    if (originalAudioRef.current) {
      originalAudioRef.current.pause();
      originalAudioRef.current.currentTime = 0;
      originalAudioRef.current.removeAttribute("src");
      originalAudioRef.current.load();
    }
  }, []);

  const pausePlayback = useCallback(() => {
    playbackOpRef.current += 1;
    audioRef.current?.pause();
    originalAudioRef.current?.pause();
    setPlayerState("paused");
  }, []);

  const playAudio = useCallback((audio: HTMLAudioElement) => {
    return audio.play();
  }, []);

  const createAudioTrack = useCallback((src: string) => {
    const audio = new Audio();
    audio.src = src;
    audio.preload = "auto";
    audio.load();
    audio.volume = 1;
    // Apply output device directly on the HTMLAudioElement
    // (AudioContext may not exist yet, so try setSinkId directly)
    const sinkId = getRequestedAudioSinkId();
    if (typeof audio.setSinkId === "function") {
      void audio.setSinkId(sinkId).catch((e) => console.warn("[audio] setSinkId failed:", e));
    }
    return audio;
  }, [getRequestedAudioSinkId]);

  const waitForMediaReady = useCallback((audio: HTMLAudioElement, timeoutMs = 1500) => {
    if (audio.readyState >= HTMLMediaElement.HAVE_CURRENT_DATA) {
      return Promise.resolve();
    }

    return new Promise<void>((resolve) => {
      let settled = false;
      const finish = () => {
        if (settled) return;
        settled = true;
        audio.removeEventListener("canplay", finish);
        audio.removeEventListener("loadeddata", finish);
        audio.removeEventListener("error", finish);
        window.clearTimeout(timerId);
        resolve();
      };
      const timerId = window.setTimeout(finish, timeoutMs);
      audio.addEventListener("canplay", finish, { once: true });
      audio.addEventListener("loadeddata", finish, { once: true });
      audio.addEventListener("error", finish, { once: true });
    });
  }, []);

  const bindAudioError = useCallback((audio: HTMLAudioElement, onErrorText: (err: MediaError | null) => string) => {
    audio.addEventListener("error", () => {
      const err = audio.error;
      if (err?.code === 1) return;
      setPlaybackError(onErrorText(err));
      setPlayerState("idle");
    });
  }, []);

  const createTrackGraph = useCallback((audio: HTMLAudioElement): TrackGraph | null => {
    if (!audioAnalyserContextRef.current && !isWindowsRuntime) {
      audioAnalyserContextRef.current = new AudioContext();
    }
    const context = audioAnalyserContextRef.current;
    if (!context || (isWindowsRuntime && context.state !== "running")) {
      return null;
    }
    try {
      const source = context.createMediaElementSource(audio);
      const gain = context.createGain();
      const analyser = context.createAnalyser();
      analyser.fftSize = 1024;
      gain.gain.value = 1;
      source.connect(gain);
      gain.connect(analyser);
      analyser.connect(context.destination);
      return { source, gain, analyser };
    } catch (e) {
      console.error("Failed to create track graph:", e);
      return null;
    }
  }, [isWindowsRuntime]);

  const loadVocalWaveform = useCallback(async (song: Song | null) => {
    const seq = ++waveformLoadSeqRef.current;
    if (!song?.vocalsPath) {
      setVocalWaveformPeaks(null);
      setVocalWaveformLoading(false);
      setVocalWaveformError(null);
      return;
    }

    setVocalWaveformLoading(true);
    setVocalWaveformError(null);

    try {
      const vocalsUrl = convertFileSrc(song.vocalsPath);
      const response = await fetch(vocalsUrl);
      if (!response.ok) {
        throw new Error(`无法读取人声轨: HTTP ${response.status}`);
      }
      const arrayBuffer = await response.arrayBuffer();
      const AudioContextCtor =
        window.AudioContext || (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
      if (!AudioContextCtor) {
        throw new Error("当前浏览器环境不支持 AudioContext");
      }
      const context = new (AudioContextCtor as typeof AudioContext)();
      try {
        const audioBuffer = await context.decodeAudioData(arrayBuffer.slice(0));
        const peaks = buildWaveformPeaks(audioBuffer);
        if (waveformLoadSeqRef.current === seq) {
          setVocalWaveformPeaks(peaks);
        }
      } finally {
        if (context.state !== "closed") {
          await context.close().catch(() => undefined);
        }
      }
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      if (waveformLoadSeqRef.current === seq) {
        setVocalWaveformPeaks(null);
        setVocalWaveformError(`原唱波形加载失败: ${message}`);
      }
    } finally {
      if (waveformLoadSeqRef.current === seq) {
        setVocalWaveformLoading(false);
      }
    }
  }, []);

  const ensureAudioContextRunning = useCallback(async (createIfMissing = false) => {
    if (!audioAnalyserContextRef.current && createIfMissing) {
      const AudioContextCtor =
        window.AudioContext || (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
      if (!AudioContextCtor) return false;
      audioAnalyserContextRef.current = new (AudioContextCtor as typeof AudioContext)();
    }
    const context = audioAnalyserContextRef.current;
    if (!context) return false;
    if (context.state === "suspended") {
      try {
        await context.resume();
      } catch (e) {
        console.error("Failed to resume audio context:", e);
      }
    }
    // Re-apply audio output device after context resumes
    // (resuming an AudioContext may reset its output device to default)
    const sinkId = getRequestedAudioSinkId();
    const ctxWithSink = context as AudioContext & { setSinkId?: (id: string) => Promise<void> };
    if (typeof ctxWithSink.setSinkId === "function") {
      try {
        await ctxWithSink.setSinkId(sinkId);
      } catch (e) {
        console.warn("[audio] setSinkId after resume failed:", e);
      }
    }
    return context.state === "running";
  }, [getRequestedAudioSinkId]);

  const ensurePlaybackGraphs = useCallback(async (mode: PlaybackMode, vol: number) => {
    if (isWindowsRuntime) {
      destroyTrackGraphs();
      applyModeRouting(vol, mode);
      return false;
    }
    if (audioRef.current && !audioGraphRef.current.instrumental) {
      const instrumentalGraph = createTrackGraph(audioRef.current);
      if (instrumentalGraph) {
        audioGraphRef.current.instrumental = instrumentalGraph;
      }
    }
    if (originalAudioRef.current?.src && !audioGraphRef.current.vocals) {
      const vocalsGraph = createTrackGraph(originalAudioRef.current);
      if (vocalsGraph) {
        audioGraphRef.current.vocals = vocalsGraph;
      }
    }
    applyModeRouting(vol, mode);
    await ensureAudioContextRunning(false);
    return true;
  }, [applyModeRouting, createTrackGraph, destroyTrackGraphs, ensureAudioContextRunning, isWindowsRuntime]);

  useEffect(() => {
    const interval = window.setInterval(() => {
      const captureLevel = (graph: TrackGraph | undefined) => {
        if (!graph) return 0;
        const buffer = new Uint8Array(graph.analyser.frequencyBinCount);
        graph.analyser.getByteTimeDomainData(buffer);
        let sumSquares = 0;
        for (let i = 0; i < buffer.length; i += 1) {
          const centered = (buffer[i] - 128) / 128;
          sumSquares += centered * centered;
        }
        return Math.sqrt(sumSquares / buffer.length);
      };
      const graphInstrumentalLevel = captureLevel(audioGraphRef.current.instrumental);
      const graphVocalsLevel = captureLevel(audioGraphRef.current.vocals);
      setTrackLevels({
        instrumental: graphInstrumentalLevel || estimatePlaybackLevel("instrumental"),
        vocals: graphVocalsLevel || estimatePlaybackLevel("vocals"),
      });
    }, 250);
    return () => window.clearInterval(interval);
  }, [estimatePlaybackLevel]);

  useEffect(() => {
    return () => {
      destroyTrackGraphs();
      if (audioAnalyserContextRef.current) {
        void audioAnalyserContextRef.current.close().catch(() => undefined);
        audioAnalyserContextRef.current = null;
      }
    };
  }, [destroyTrackGraphs]);

  const getTimelineTime = useCallback(() => {
    if (playbackMode === "vocals" && originalAudioRef.current?.src) {
      return originalAudioRef.current.currentTime;
    }
    return audioRef.current?.currentTime ?? originalAudioRef.current?.currentTime ?? 0;
  }, [playbackMode]);

  const syncSecondaryTrackToMaster = useCallback((mode: PlaybackMode) => {
    if (!audioRef.current || !originalAudioRef.current?.src) return;
    if (audioRef.current.paused || originalAudioRef.current.paused) return;

    // Only enforce tight sync in original mode.
    // When the user is soloing accompaniment or vocals, the audible track should
    // keep running smoothly instead of being repeatedly seeked back into place.
    if (mode !== "original") return;

    const masterTime = audioRef.current.currentTime;
    const followerTime = originalAudioRef.current.currentTime;
    if (Math.abs(followerTime - masterTime) > 0.08) {
      originalAudioRef.current.currentTime = masterTime;
    }
  }, []);

  const startPlayback = useCallback(async (mode: PlaybackMode, resetToStart = false) => {
    if (!audioRef.current) return false;
    const opId = playbackOpRef.current + 1;
    playbackOpRef.current = opId;
    const isCurrentOp = () => playbackOpRef.current === opId;

    try {
      const timelineTime = resetToStart ? 0 : getTimelineTime();

      const vocalsAudio = originalAudioRef.current;
      const needsVocals = mode === "original" || mode === "vocals";
      const hasVocals = Boolean(vocalsAudio?.src);
      if (needsVocals && !hasVocals) {
        setPlaybackError("人声轨不可用，请先完成人声分离");
        return false;
      }

      // Force a shared timeline origin so both tracks stay anchored from song start.
      const targetTime = Math.max(0, timelineTime);
      audioRef.current.currentTime = targetTime;
      if (vocalsAudio?.src) {
        vocalsAudio.currentTime = targetTime;
      }

      const readinessTasks = [waitForMediaReady(audioRef.current)];
      if (vocalsAudio?.src) {
        readinessTasks.push(waitForMediaReady(vocalsAudio));
      }
      await Promise.all(readinessTasks);
      if (!isCurrentOp()) return false;

      applyModeRouting(volume, mode);
      if (!isCurrentOp()) return false;

      const playTasks: Array<Promise<void>> = [playAudio(audioRef.current)];
      if (vocalsAudio?.src) {
        playTasks.push(playAudio(vocalsAudio));
      }
      await Promise.all(playTasks);
      if (!isCurrentOp()) return false;
      syncSecondaryTrackToMaster(mode);
      setPlayerState("playing");
      return true;
    } catch (e) {
      if (isBenignAbortError(e)) {
        return false;
      }
      console.error("play() failed:", e);
      setPlaybackError(`播放失败: ${e}`);
      setPlayerState("idle");
      return false;
    }
  }, [getTimelineTime, volume, applyModeRouting, isBenignAbortError, playAudio, syncSecondaryTrackToMaster, waitForMediaReady]);

  // Load songs on mount
  useEffect(() => {
    if (!isDesktopRuntime) {
      return;
    }
    const loadSongs = async () => {
      try {
        const existingSongs = await invoke<Song[]>("get_songs");
        setSongs(existingSongs);
      } catch (e) {
        console.error("Failed to load songs:", e);
      }
    };
    loadSongs();
  }, [isDesktopRuntime]);

  // Parse LRC lyrics
  const parseLRC = useCallback((lrcContent: string): Array<{ time: number; text: string }> => {
    const lines: Array<{ time: number; text: string }> = [];
    const timestampRegex = /\[(\d{2}):(\d{2})(?:[.:](\d{2,3}))?\]/g;
    const contentLines = lrcContent.replace(/^\uFEFF/, "").split(/\r?\n/);

    for (const rawLine of contentLines) {
      const timestamps = Array.from(rawLine.matchAll(timestampRegex));
      if (timestamps.length === 0) continue;

      const text = rawLine.replace(timestampRegex, "").trim();
      if (!text) continue;

      for (const timestamp of timestamps) {
        const minutes = parseInt(timestamp[1], 10);
        const seconds = parseInt(timestamp[2], 10);
        const msStr = (timestamp[3] ?? "0").padEnd(3, "0");
        const ms = parseInt(msStr, 10);
        const time = minutes * 60000 + seconds * 1000 + ms;
        lines.push({ time, text });
      }
    }

    return lines.sort((a, b) => a.time - b.time);
  }, []);

  const createDocumentFromLRC = useCallback((song: Song, parsedLines: Array<{ time: number; text: string }>, source = "lrc_fallback"): LyricDocument => {
    const now = Date.now();
    return {
      songId: song.id,
      version: 1,
      language: null,
      source,
      alignmentEngine: "none",
      createdAt: now,
      updatedAt: now,
      globalOffsetMs: 0,
      dirty: false,
      qualityScore: 0.4,
      lines: parsedLines.map((line, index) => {
        const nextStart = parsedLines[index + 1]?.time ?? (line.time + 2500);
        const endMs = Math.max(line.time + 300, nextStart - 50);
        return {
          id: `${song.id}-line-${index}`,
          index,
          startMs: line.time,
          endMs,
          text: line.text,
          confidence: 0.5,
          edited: false,
          locked: false,
          tokens: [],
        };
      }),
    };
  }, []);

  // Load lyrics for a song
  const loadLyrics = useCallback(async (song: Song) => {
    const seq = ++lyricsLoadSeqRef.current;
    const stillCurrent = () => currentSongRef.current?.id === song.id && lyricsLoadSeqRef.current === seq;

    if (!song.lyricsPath) {
      if (stillCurrent()) setLyricsDoc(null);
      return;
    }
    try {
      const document = await invoke<LyricDocument | null>("get_lyrics_document", { songId: song.id });
      if (document && document.lines.length > 0) {
        if (stillCurrent()) setLyricsDoc(document);
        return;
      }
      const content = await invoke<string>("read_file", { path: song.lyricsPath });
      const parsed = parseLRC(content);
      const fallbackDoc = createDocumentFromLRC(song, parsed, "lrc_fallback");
      if (stillCurrent()) setLyricsDoc(fallbackDoc);
    } catch (e) {
      console.error("Failed to load lyrics:", e);
      if (stillCurrent()) setLyricsDoc(null);
    }
  }, [parseLRC, createDocumentFromLRC]);

  useEffect(() => {
    currentSongRef.current = currentSong;
  }, [currentSong]);

  useEffect(() => {
    void loadVocalWaveform(currentSong);
  }, [currentSong?.id, currentSong?.vocalsPath, loadVocalWaveform]);

  useEffect(() => {
    if (!isDesktopRuntime) return;
    let cancelled = false;
    void (async () => {
      try {
        const settings = await invoke<FileStorageSettings>("get_file_storage_settings");
        if (!cancelled) {
          setFileStorageSettings(settings);
        }
      } catch (error) {
        console.error("Failed to load file storage settings:", error);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [isDesktopRuntime]);

  useEffect(() => {
    if (!isDesktopRuntime) return;
    let cancelled = false;
    void (async () => {
      try {
        const health = await invoke<RuntimeHealthReport>("get_runtime_health");
        const bootstrap = await invoke<BootstrapStatus>("get_bootstrap_status");
        if (!cancelled) {
          setRuntimeHealth(health);
          setBootstrapStatus(bootstrap);
        }
      } catch (error) {
        console.error("Failed to detect runtime health:", error);
        if (!cancelled) {
          setRuntimeHealth({
            level: "error",
            label: "环境异常",
            detail: "无法完成启动检测",
            torchCudaAvailable: false,
            selectedDevice: "cpu",
            torchVersion: null,
            torchCudaVersion: null,
            torchCudaDeviceName: null,
            hasNvidiaGpu: false,
            nvidiaDriverVisible: false,
            nvidiaDriverCudaVersion: null,
            checks: [],
          });
          setBootstrapStatus(null);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [isDesktopRuntime]);

  const handleRefreshRuntimeHealth = useCallback(async () => {
    if (!isDesktopRuntime) return;
    try {
      const health = await invoke<RuntimeHealthReport>("get_runtime_health");
      const bootstrap = await invoke<BootstrapStatus>("get_bootstrap_status");
      setRuntimeHealth(health);
      setBootstrapStatus(bootstrap);
    } catch (error) {
      console.error("Failed to refresh runtime health:", error);
    }
  }, [isDesktopRuntime]);

  const handleBootstrapInstall = useCallback(async () => {
    if (!isDesktopRuntime) return;
    setBootstrapInstalling(true);
    setBootstrapMessage("正在安装运行时与模型...");
    try {
      const status = await invoke<BootstrapStatus>("bootstrap_install_minimal", {
        preferDemucsCuda: false,
      });
      const health = await invoke<RuntimeHealthReport>("get_runtime_health");
      setBootstrapStatus(status);
      setRuntimeHealth(health);
      if (status.canRunCore) {
        setBootstrapMessage("安装完成，可运行。");
      } else {
        const missing = health.checks
          .filter((check) => !check.ok)
          .map((check) => check.name)
          .join("、");
        setBootstrapMessage(`安装未完成：${missing || "存在未就绪组件"}。`);
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setBootstrapMessage(`安装失败：${message}`);
    } finally {
      setBootstrapInstalling(false);
    }
  }, [isDesktopRuntime]);

  // Listen for processing events
  useEffect(() => {
    if (!isDesktopRuntime) {
      return;
    }
    const unlistenProgress = listen<ProcessingStatus>("processing-progress", (event) => {
      const { song_id, stage, progress } = event.payload;
      const nextStatus =
        stage === "cancelled"
          ? "cancelled"
          : stage === "cancelling"
            ? "cancelling"
            : "processing";
      setSongs((prev) =>
        prev.map((s) =>
          s.id === song_id
            ? { ...s, progress, processingStage: stage as ProcessingStage, status: nextStatus as Song["status"] }
            : s
        )
      );
    });

    const unlistenComplete = listen<{ song: Song }>("processing-complete", (event) => {
      const updatedSong = event.payload.song;
      setSongs((prev) =>
        prev.map((s) =>
          s.id === updatedSong.id
            ? s.status === "cancelled" || s.status === "cancelling"
              ? s
              : updatedSong
            : s
        )
      );
      // Also update currentSong if it's the one that completed
      setCurrentSong((prev) =>
        prev?.id === updatedSong.id && prev.status !== "cancelled" && prev.status !== "cancelling" ? updatedSong : prev
      );
      if (currentSongRef.current?.id === updatedSong.id) {
        void loadLyrics(updatedSong);
      }
    });

    const unlistenError = listen<{ song_id: string; stage: string; error: string }>("processing-error", (event) => {
      const { song_id, stage, error } = event.payload;
      setSongs((prev) =>
        prev.map((s) =>
          s.id === song_id
            ? s.status === "cancelled" || s.status === "cancelling"
              ? s
              : { ...s, status: "error" as const, processingStage: stage as ProcessingStage, error_message: error }
            : s
        )
      );
    });

    return () => {
      unlistenProgress.then((fn) => fn());
      unlistenComplete.then((fn) => fn());
      unlistenError.then((fn) => fn());
    };
  }, [loadLyrics, isDesktopRuntime]);

  const handleSaveLyricsDocument = useCallback((document: LyricDocument) => {
    setLyricsDoc(document);
    if (lyricsSaveTimerRef.current !== null) {
      window.clearTimeout(lyricsSaveTimerRef.current);
    }
    const songId = document.songId;
    lyricsSaveTimerRef.current = window.setTimeout(async () => {
      if (!songId) return;
      try {
        await invoke("save_lyrics_document", { songId, document });
      } catch (e) {
        console.error("Failed to save lyrics document:", e);
      }
    }, 400);
  }, []);

  const refreshSongs = useCallback(async () => {
    const nextSongs = await invoke<Song[]>("get_songs");
    setSongs(nextSongs);
    return nextSongs;
  }, []);

  const handleImportLyricsLrc = useCallback(async (song: Song) => {
    if (!isDesktopRuntime) {
      setLyricsImportError("当前环境不支持导入 LRC 歌词");
      return;
    }
    setLyricsImportLoadingSongId(song.id);
    setLyricsImportError(null);
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "LRC Lyrics", extensions: ["lrc", "txt"] }],
        defaultPath: song.lyricsPath || song.originalPath || undefined,
      });
      if (typeof selected !== "string" || !selected.trim()) {
        return;
      }

      const content = await invoke<string>("read_file", { path: selected });
      const parsed = parseLRC(content);
      if (parsed.length === 0) {
        setLyricsImportError("未识别到 LRC 时间轴，请确认文件包含 [mm:ss.xx] 时间戳");
        return;
      }

      const document = createDocumentFromLRC(song, parsed, "lrc_import");
      setLyricsDoc(document);
      await invoke("save_lyrics_document", { songId: song.id, document });
      const refreshedSongs = await refreshSongs();
      const updatedSong = refreshedSongs.find((item) => item.id === song.id) || null;
      if (updatedSong) {
        setCurrentSong((prev) => (prev?.id === updatedSong.id ? updatedSong : prev));
      }
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      console.error("Failed to import LRC lyrics:", e);
      setLyricsImportError(`导入 LRC 失败: ${message}`);
    } finally {
      setLyricsImportLoadingSongId(null);
    }
  }, [createDocumentFromLRC, isDesktopRuntime, parseLRC, refreshSongs]);

  // Handle file import - call backend to create songs
  const handleFilesSelected = useCallback(async (paths: string[]) => {
    try {
      const newSongs = await invoke<Song[]>("import_songs", { paths });
      setSongs((prev) => [...prev, ...newSongs]);
      // Auto-start processing after import; lyric generation is now manual.
      await Promise.all(
        newSongs.map(async (song) => {
          try {
            await invoke("start_process", { songId: song.id, preferDemucsCuda: true });
            setSongs((prev) => prev.map((item) =>
              item.id === song.id && item.status !== "processing" && item.status !== "cancelling"
                ? { ...item, status: "queued" as const, progress: 0, processingStage: "queued" as ProcessingStage, error_message: undefined }
                : item
            ));
          } catch (e) {
            console.error(`Failed to auto-start process for ${song.name}:`, e);
          }
        })
      );
    } catch (e) {
      console.error("Failed to import songs:", e);
    }
  }, []);

  const handleSaveStorageSettings = useCallback(async (settingsOverride?: FileStorageSettings) => {
    const settingsToSave = settingsOverride ?? fileStorageSettings;
    if (!settingsToSave) return;
    setFileStorageSettingsSaving(true);
    setFileStorageSettingsMessage(null);
    try {
      const normalized = await invoke<FileStorageSettings>("update_file_storage_settings", {
        settings: settingsToSave,
      });
      setFileStorageSettings(normalized);
      const refreshedSongs = await refreshSongs();
      const targetSongId = currentSongRef.current?.id ?? null;
      if (targetSongId) {
        const updatedSong = refreshedSongs.find((song) => song.id === targetSongId) || null;
        setCurrentSong(updatedSong);
      }
      setFileStorageSettingsMessage("已保存文件管理设置并完成自动迁移。");
    } catch (error) {
      console.error("Failed to save file storage settings:", error);
      setFileStorageSettingsMessage(`保存失败: ${error instanceof Error ? error.message : String(error)}`);
    } finally {
      setFileStorageSettingsSaving(false);
    }
  }, [fileStorageSettings, refreshSongs]);

  const handleChooseStorageFolder = useCallback(async (field: keyof FileStorageSettings) => {
    const currentPath = fileStorageSettings?.[field] || "";
    const selected = await open({
      directory: true,
      multiple: false,
      defaultPath: currentPath.trim() || undefined,
    });
    if (typeof selected === "string" && selected.trim()) {
      const nextSettings = {
        instrumentalRoot: fileStorageSettings?.instrumentalRoot || "",
        vocalsRoot: fileStorageSettings?.vocalsRoot || "",
        lyricsRoot: fileStorageSettings?.lyricsRoot || "",
        [field]: selected,
      } as FileStorageSettings;
      setFileStorageSettings(nextSettings);
      void handleSaveStorageSettings(nextSettings);
    }
  }, [fileStorageSettings, handleSaveStorageSettings]);

  const handleResetStorageSettings = useCallback(() => {
    const nextSettings = {
      instrumentalRoot: "",
      vocalsRoot: "",
      lyricsRoot: "",
    };
    setFileStorageSettings(nextSettings);
    setFileStorageSettingsMessage("已恢复为默认目录，保存后自动迁移。");
    void handleSaveStorageSettings(nextSettings);
  }, [handleSaveStorageSettings]);

  // Cancel processing
  const handleCancelProcess = useCallback(async (song: Song) => {
    setSongs((prev) => prev.map((item) =>
      item.id === song.id
        ? { ...item, status: "cancelled" as const, progress: 0, processingStage: "cancelled" as ProcessingStage, error_message: "用户取消" }
        : item
    ));
    setCurrentSong((prev) =>
      prev?.id === song.id
        ? { ...prev, status: "cancelled" as const, progress: 0, processingStage: "cancelled" as ProcessingStage, error_message: "用户取消" }
        : prev
    );
    try {
      await invoke("cancel_process", { songId: song.id });
    } catch (e) {
      console.error("Failed to cancel processing:", e);
    }
  }, []);

  const handleSeparateInstrumental = useCallback(async (song: Song) => {
    try {
      const command = song.status === "ready" ? "reprocess_song" : "start_process";
      await invoke(command, { songId: song.id, preferDemucsCuda: true });
      setSongs((prev) => prev.map((item) =>
        item.id === song.id && item.status !== "processing" && item.status !== "cancelling"
          ? { ...item, status: "queued" as const, progress: 0, processingStage: "queued" as ProcessingStage, error_message: undefined }
          : item
      ));
    } catch (e) {
      console.error("Failed to start separation:", e);
    }
  }, []);

  // Select a song - always select, auto-play only when ready
  const handleSelectSong = useCallback(async (song: Song) => {
    // Always update current song selection
    const latestSong = songs.find((s) => s.id === song.id) || song;
    const nextMode: PlaybackMode =
      (playbackMode === "original" || playbackMode === "vocals") && latestSong.vocalsPath
        ? playbackMode
        : "instrumental";
    stopAllAudio();
    setCurrentSong(latestSong);
    setPlaybackMode(nextMode);
    setPlaybackError(null);
    setWhisperDraftError(null);
    setLyricsImportError(null);

    // Load lyrics in the background so playback stays attached to the user click
    void loadLyrics(latestSong);

    if (song.status !== "ready") {
      return;
    }

    // Validate paths exist
    if (!latestSong.instrumentalPath) {
      setPlaybackError("伴奏文件路径不存在，请重新处理");
      return;
    }

    try {
      // Use convertFileSrc for streaming playback
      const instrumentalUrl = convertFileSrc(latestSong.instrumentalPath);

      // Clean up old audio
      if (audioRef.current) {
        audioRef.current.pause();
      }
      if (originalAudioRef.current) {
        originalAudioRef.current.pause();
      }

      audioRef.current = createAudioTrack(instrumentalUrl);
      bindAudioError(audioRef.current, (err) => `伴奏加载失败: ${err?.message || "未知错误"}`);

      audioRef.current.addEventListener("loadedmetadata", async () => {
        const durationMs = audioRef.current!.duration * 1000;
        setSongs((prev) => prev.map((s) =>
          s.id === latestSong.id ? { ...s, duration: durationMs } : s
        ));
        setCurrentSong((prev) =>
          prev?.id === latestSong.id ? { ...prev, duration: durationMs } : prev
        );
        try {
          await invoke("update_song_duration", { songId: latestSong.id, duration: durationMs });
        } catch (e) {
          console.error("Failed to persist duration:", e);
        }
      });

      if (latestSong.vocalsPath) {
        const vocalsUrl = convertFileSrc(latestSong.vocalsPath);
        originalAudioRef.current = createAudioTrack(vocalsUrl);
        bindAudioError(originalAudioRef.current, (err) => `人声加载失败: ${err?.message || "未知错误"}`);
      } else {
        originalAudioRef.current = null;
        audioGraphRef.current.vocals = undefined;
      }

      await ensurePlaybackGraphs(nextMode, volume);
      await startPlayback(nextMode, true);
    } catch (e) {
      console.error("Failed to play:", e);
      setPlaybackError(`播放失败: ${e}`);
      setPlayerState("idle");
    }
  }, [songs, loadLyrics, volume, playbackMode, stopAllAudio, startPlayback, createAudioTrack, bindAudioError, ensurePlaybackGraphs]);

  const handlePlayPause = useCallback(async () => {
    if (!audioRef.current || !audioRef.current.src) {
      if (currentSong?.status === "ready") {
        await handleSelectSong(currentSong);
      }
      return;
    }

    if (playerState === "playing") {
      pausePlayback();
    } else {
      await ensurePlaybackGraphs(playbackMode, volume);
      await startPlayback(playbackMode, false);
    }
  }, [currentSong, handleSelectSong, playerState, playbackMode, volume, pausePlayback, startPlayback, ensurePlaybackGraphs]);

  const handleSeek = useCallback((time: number) => {
    if (audioRef.current) audioRef.current.currentTime = time / 1000;
    if (originalAudioRef.current) originalAudioRef.current.currentTime = time / 1000;
    setCurrentTime(time);
  }, []);

  const handleVolumeChange = useCallback((vol: number) => {
    setVolume(vol);
    applyModeRouting(vol, playbackMode);
  }, [playbackMode, applyModeRouting]);

  const handleModeChange = useCallback(async (mode: PlaybackMode) => {
    setPlaybackError(null);

    if (mode !== "instrumental" && !currentSong?.vocalsPath) {
      setPlaybackError("人声文件路径不存在，请先完成人声分离");
      return;
    }

    setPlaybackMode(mode);
    applyModeRouting(volume, mode);

    if (playerState === "playing") {
      await ensurePlaybackGraphs(mode, volume);
    }
  }, [volume, currentSong, applyModeRouting, playerState, ensurePlaybackGraphs]);

  const handlePrev = useCallback(() => {
    const readySongs = songs.filter((s) => s.status === "ready");
    const idx = readySongs.findIndex((s) => s.id === currentSong?.id);
    if (idx > 0) handleSelectSong(readySongs[idx - 1]);
  }, [songs, currentSong, handleSelectSong]);

  const handleNext = useCallback(() => {
    const readySongs = songs.filter((s) => s.status === "ready");
    const idx = readySongs.findIndex((s) => s.id === currentSong?.id);
    if (idx < readySongs.length - 1) handleSelectSong(readySongs[idx + 1]);
  }, [songs, currentSong, handleSelectSong]);

  const handleDeleteSong = useCallback(async (id: string) => {
    try {
      await invoke("delete_song", { id });
      stopAllAudio();
      setSongs((prev) => prev.filter((s) => s.id !== id));
      if (currentSong?.id === id) {
        setCurrentSong(null);
        setCurrentTime(0);
        setPlayerState("idle");
        setPlaybackError(null);
        setLyricsDoc(null);
        audioRef.current = null;
        originalAudioRef.current = null;
      }
    } catch (e) {
      console.error(e);
      return;
    }
  }, [currentSong, stopAllAudio]);

  const handleMoveSongToFolder = useCallback(async (songId: string, folderName: string | null) => {
    try {
      await invoke("set_song_folder", { songId, folderName });
      setSongs((prev) => prev.map((song) =>
        song.id === songId ? { ...song, playlistFolder: folderName } : song
      ));
      setCurrentSong((prev) =>
        prev?.id === songId ? { ...prev, playlistFolder: folderName } : prev
      );
    } catch (e) {
      console.error("Failed to move song to folder:", e);
    }
  }, []);

  const handleRenameSong = useCallback(async (songId: string, newName: string) => {
    try {
      await invoke("rename_song", { songId, newName });
      setSongs((prev) => prev.map((song) =>
        song.id === songId ? { ...song, name: newName } : song
      ));
      setCurrentSong((prev) =>
        prev?.id === songId ? { ...prev, name: newName } : prev
      );
    } catch (e) {
      console.error("Failed to rename song:", e);
    }
  }, []);

  const handleRenameFolder = useCallback(async (oldName: string, newName: string) => {
    try {
      await invoke("rename_playlist_folder", { oldName, newName });
      setSongs((prev) => prev.map((song) =>
        song.playlistFolder === oldName ? { ...song, playlistFolder: newName } : song
      ));
      setCurrentSong((prev) =>
        prev?.playlistFolder === oldName ? { ...prev, playlistFolder: newName } : prev
      );
    } catch (e) {
      console.error("Failed to rename playlist folder:", e);
    }
  }, []);

  const handleSearchLyrics = useCallback(async (song: Song, query?: string) => {
    try {
      setLyricsCandidateSong(song);
      setLyricsCandidateOpen(true);
      const trimmedQuery = query?.trim();
      if (!trimmedQuery) {
        setLyricsCandidateError(null);
        setLyricsCandidates(null);
        setLyricsSearchQuery("");
        setLyricsCandidateLoading(false);
        return;
      }
      setLyricsCandidateError(null);
      setLyricsSearchQuery(trimmedQuery);
      setLyricsCandidateLoading(true);
      const candidates = await invoke<LyricsCandidate[]>("search_match_lyrics", {
        songId: song.id,
        query: trimmedQuery ? trimmedQuery : null,
      });
      if (!candidates || candidates.length === 0) {
        setLyricsCandidateError("没有找到可用的歌词候选");
        return;
      }
      setLyricsCandidates(candidates);
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      setLyricsCandidateError(`搜索歌词失败: ${message}`);
    } finally {
      setLyricsCandidateLoading(false);
    }
  }, []);

  const handleApplyLyricsCandidate = useCallback(async (candidate: LyricsCandidate) => {
    try {
      await invoke("save_lyrics_document", { songId: candidate.document.songId, document: candidate.document });
      setLyricsDoc(candidate.document);
      const refreshedSongs = await refreshSongs();
      const updatedSong = refreshedSongs.find((item) => item.id === candidate.document.songId) || null;
      if (updatedSong) {
        setCurrentSong((prev) => (prev?.id === updatedSong.id ? updatedSong : prev));
      }
      setLyricsCandidates(null);
      setLyricsCandidateSong(null);
      setLyricsCandidateError(null);
    } catch (e) {
      console.error("Failed to apply lyrics candidate:", e);
    }
  }, [refreshSongs]);

  const handleGenerateWhisperDraft = useCallback(async (song: Song) => {
    setWhisperDraftError(null);
    setWhisperDraftLoadingSongId(song.id);
    try {
      const result = await invoke<GeneratedLyricsDraftResult>("generate_whisper_base_lyrics", {
        songId: song.id,
      });

      setWhisperDraftError(null);
      setSongs((prev) => prev.map((item) =>
        item.id === song.id
          ? { ...item, lyricsPath: result.lyricsPath }
          : item
      ));
      setCurrentSong((prev) =>
        prev?.id === song.id
          ? { ...prev, lyricsPath: result.lyricsPath }
          : prev
      );
      if (currentSongRef.current?.id === song.id) {
        setLyricsDoc(result.document);
      }
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      setWhisperDraftError(`AI 听写生成失败: ${message}`);
    } finally {
      setWhisperDraftLoadingSongId((prev) => (prev === song.id ? null : prev));
    }
  }, []);

  const closeLyricsCandidateModal = useCallback(() => {
    setLyricsCandidates(null);
    setLyricsCandidateSong(null);
    setLyricsCandidateError(null);
    setLyricsSearchQuery("");
    setLyricsCandidateLoading(false);
    setLyricsCandidateOpen(false);
  }, []);

  // Update playback time & lyrics sync
  useEffect(() => {
    const interval = setInterval(() => {
      if (playerState === "playing") {
        syncSecondaryTrackToMaster(playbackMode);
        const audio =
          playbackMode === "vocals"
            ? (originalAudioRef.current || audioRef.current)
            : (audioRef.current || originalAudioRef.current);
        if (audio) {
          const nextTime = audio.currentTime * 1000;
          const now = performance.now();
          if (nextTime > playbackMonitorRef.current.lastTime + 20) {
            playbackMonitorRef.current.lastTime = nextTime;
            playbackMonitorRef.current.lastAt = now;
          } else if (now - playbackMonitorRef.current.lastAt > 1500 && !audio.paused && !audio.ended) {
            console.warn("[audio] media element is playing but currentTime is not advancing", {
              playbackMode,
              readyState: audio.readyState,
              currentTime: audio.currentTime,
              audioContextState: audioAnalyserContextRef.current?.state,
            });
            playbackMonitorRef.current.lastAt = now;
          }
          setCurrentTime(nextTime);
        }
      }
    }, 100);
    return () => clearInterval(interval);
  }, [playerState, playbackMode, syncSecondaryTrackToMaster]);

  useEffect(() => {
    return () => {
      if (lyricsSaveTimerRef.current !== null) {
        window.clearTimeout(lyricsSaveTimerRef.current);
      }
    };
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.code !== "Space") return;
      const target = event.target as HTMLElement | null;
      if (target) {
        const tag = target.tagName;
        const editable = target.getAttribute("contenteditable");
        if (tag === "INPUT" || tag === "TEXTAREA" || editable === "true") {
          return;
        }
      }
      event.preventDefault();
      handlePlayPause();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [handlePlayPause]);

  return (
    <div className="relative h-full w-full bg-[var(--bg-primary)] flex flex-col">
      <div className="flex h-full flex-col gap-[18px] p-[24px]">
        {/* Header */}
        <header className="app-header">
          <div className="app-header-left">
            <div className="app-header-brand">
              <img src="/icon.png" alt="Macaron Singer" className="app-logo" onError={(e) => e.currentTarget.style.display = 'none'} />
              <h1 className="app-title">Macaron Singer</h1>
            </div>
            <button
              type="button"
              onClick={() => {
                setFileStorageSettingsOpen(true);
                setSettingsPane("runtime");
              }}
              className="status-chip transition-colors hover:bg-[var(--button-hover-bg)]"
              aria-label="查看运行环境状态"
            >
              <span
                className={`status-chip-dot ${
                  runtimeHealth?.level === "ready"
                    ? "status-chip-dot-success"
                    : runtimeHealth?.level === "warning"
                      ? "status-chip-dot-warning"
                      : "status-chip-dot-error"
                }`}
              />
              <span className="ui-chip-text">
                {runtimeHealth?.label ?? "检测中..."}
              </span>
            </button>
            <div
              className={`status-chip ${runtimeTorchCudaAvailable ? "border-[color-mix(in_srgb,var(--status-success)_32%,transparent)]" : ""}`}
              title={runtimeDeviceTitle}
            >
              <span
                className={`status-chip-dot ${
                  runtimeTorchCudaAvailable ? "status-chip-dot-success" : ""
                }`}
              />
              <span className="ui-chip-text">{runtimeDeviceLabel}</span>
            </div>
          </div>
          <div className="app-header-right">
            <div className="header-stats">
              <span>已收录</span>
              <span className="font-bold text-[var(--text-primary)]">
                {readySongCount}
              </span>
              <span>首</span>
            </div>
            <button
              onClick={() => {
                setFileStorageSettingsOpen(true);
                setSettingsPane("paths");
              }}
              className="ui-button ghost-button text-[13px] font-semibold transition-colors"
            >
              偏好设置
            </button>
            <button
              onClick={async () => {
                const selected = await open({
                  multiple: true,
                  filters: [{ name: "Audio / Video", extensions: MEDIA_IMPORT_EXTENSIONS }]
                });
                if (selected) {
                  const paths = Array.isArray(selected) ? selected : [selected];
                  if (paths.length > 0) handleFilesSelected(paths);
                }
              }}
              className="ui-button ui-button-primary primary-action-button text-[13px] font-bold transition-colors hover:bg-[var(--accent-hover)]"
            >
              导入歌曲
            </button>
          </div>
        </header>

        {/* Main content: left playlist, right player with lyrics */}
        <div className="min-h-0 flex-1 flex gap-[16px]">
          {/* Left: Playlist */}
          <div className="w-[300px] shrink-0 min-h-0 flex flex-col">
            <Playlist
              songs={songs}
              currentSong={currentSong}
              onSelectSong={handleSelectSong}
              onDeleteSong={handleDeleteSong}
              onCancelProcess={handleCancelProcess}
              onStartProcess={handleSeparateInstrumental}
              onMoveSongToFolder={handleMoveSongToFolder}
              onRenameSong={handleRenameSong}
              onRenameFolder={handleRenameFolder}
              onSearchLyrics={handleSearchLyrics}
              onImportLyricsLrc={handleImportLyricsLrc}
              onGenerateLyricsDraft={handleGenerateWhisperDraft}
            />
          </div>

          {/* Right: Large Player with Lyrics */}
          <div className="player-shell min-w-0 flex-1 overflow-hidden flex flex-col">
            {currentSong ? (
              <div className="relative flex-1 flex flex-col min-h-0 h-full">
                {/* Track meter */}
                <div className="shrink-0 relative mt-2 h-[88px]">
                  <div className="pointer-events-none absolute left-1/2 top-3 z-20 w-[min(50vw,640px)] -translate-x-1/2">
                    <div className="level-meter-panel">
                      <div className="space-y-1">
                        {([
                          ["伴奏", trackLevels.instrumental, "instrumental"],
                          ["人声", trackLevels.vocals, "vocal"],
                        ] as Array<[string, number, "instrumental" | "vocal"]>).map(([label, level, type]) => (
                          <div key={label} className="level-meter-row">
                            <span className="level-meter-label">{label}</span>
                            <div className="level-meter-track">
                              <div
                                className={`level-meter-fill ${
                                  type === "instrumental" ? "level-meter-fill-instrumental" : "level-meter-fill-vocal"
                                }`}
                                style={{
                                  width: `${Math.max(2, Math.min(100, level * 200))}%`,
                                }}
                              />
                            </div>
                          </div>
                        ))}
                      </div>
                    </div>
                  </div>
                </div>
                {/* Lyrics Area */}
                  <div className="min-h-0 flex-1 flex flex-col px-6 pt-2 pb-[196px]">
                  <div className="min-h-0 flex-1 flex items-center justify-center overflow-hidden">
                    {lyricsDoc ? (
                      <LyricsPanel
                        document={lyricsDoc}
                        currentTime={currentTime}
                        isPlaying={playerState === "playing"}
                        onSeek={handleSeek}
                        onSaveDocument={handleSaveLyricsDocument}
                      />
                    ) : (
                      <div className="text-[var(--text-muted)] text-base text-center py-8">
                        暂无歌词
                      </div>
                    )}
                  </div>
                </div>
                {vocalWaveformEnabled && (
                  <div className="waveform-layer">
                    <VocalWaveformPreview
                      peaks={vocalWaveformPeaks}
                      currentTime={currentTime}
                      duration={currentSong.duration}
                      isPlaying={playerState === "playing"}
                      loading={vocalWaveformLoading}
                      error={vocalWaveformError}
                    />
                  </div>
                )}
                {/* Controls - structured layout with breathing room */}
                <div className="player-dock h-[172px] shrink-0">
                  {/* Song Info */}
                  <div className="player-track-info-row">
                    <div className="player-track-info-left">
                      <div className="player-track-icon">🎵</div>
                      <div className="min-w-0">
                        <div className="ui-text-ellipsis text-sm font-semibold text-[var(--text-primary)]" title={currentSong.name}>{currentSong.name}</div>
                        <div className="ui-text-ellipsis text-xs text-[var(--text-secondary)]">
                          {playbackMode === "original" ? "原唱模式" : playbackMode === "vocals" ? "人声模式" : "伴奏模式"}
                        </div>
                        <div className="song-status ui-text-ellipsis" title={playbackError || whisperDraftError || lyricsImportError || ""}>
                          {whisperDraftLoadingSongId === currentSong.id
                            ? "AI 听写生成中..."
                            : whisperDraftError && whisperDraftLoadingSongId !== currentSong.id
                              ? whisperDraftError
                              : lyricsImportLoadingSongId === currentSong.id
                                ? "LRC 导入中..."
                                : lyricsImportError && lyricsImportLoadingSongId !== currentSong.id
                                  ? lyricsImportError
                                  : playbackError || ""}
                        </div>
                      </div>
                    </div>
                    <div className="waveform-controls">
                      <button
                        onClick={() => setVocalWaveformEnabled((value) => !value)}
                        className={`ui-button waveform-toggle-button transition-all ${
                          vocalWaveformEnabled ? "is-active" : "hover:bg-[var(--button-hover-bg)]"
                        }`}
                      >
                        {vocalWaveformEnabled ? "显示原唱波形" : "隐藏原唱波形"}
                      </button>
                    </div>
                  </div>
                  {/* Progress Bar */}
                  <div className="player-progress-row">
                    <span className="player-time-label text-right">
                      {formatTime(currentTime)}
                    </span>
                    <div
                      className="player-progress-track cursor-pointer"
                      onClick={(e) => {
                        const rect = e.currentTarget.getBoundingClientRect();
                        const pct = (e.clientX - rect.left) / rect.width;
                        if (currentSong.duration > 0) {
                          handleSeek(pct * currentSong.duration);
                        }
                      }}
                    >
                      <div
                        className="player-progress-fill h-full rounded-full transition-all"
                        style={{ width: `${currentSong.duration > 0 ? (currentTime / currentSong.duration) * 100 : 0}%` }}
                      />
                    </div>
                    <span className="player-time-label">
                      {formatTime(currentSong.duration)}
                    </span>
                  </div>

                  {/* Controls Row - centered with enforced separation */}
                  <div className="player-controls-row">
                    <div className="transport-controls">
                      <button onClick={handlePrev} className="player-secondary-button ui-icon-button text-[var(--text-secondary)] transition-colors hover:bg-[var(--button-hover-bg)] hover:text-[var(--text-primary)]">
                      <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                        <path d="M6 6h2v12H6V6zm3.5 6l8.5 6V6l-8.5 6z"/>
                      </svg>
                    </button>
                    <button
                      onClick={handlePlayPause}
                      className="player-play-button ui-icon-button bg-[var(--primary-button-bg)] text-[var(--primary-button-text)] shadow-lg transition-transform hover:scale-105"
                    >
                      {playerState === "playing" ? (
                        <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
                          <path d="M6 4h4v16H6V4zm8 0h4v16h-4V4z"/>
                        </svg>
                      ) : (
                        <svg className="w-4 h-4 ml-0.5" fill="currentColor" viewBox="0 0 24 24">
                          <path d="M8 5v14l11-7z"/>
                        </svg>
                      )}
                    </button>
                    <button onClick={handleNext} className="player-secondary-button ui-icon-button text-[var(--text-secondary)] transition-colors hover:bg-[var(--button-hover-bg)] hover:text-[var(--text-primary)]">
                      <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                        <path d="M6 18l8.5-6L6 6v12zm2-8.14L11.03 12 8 14.14V9.86zM16 6h2v12h-2V6z"/>
                      </svg>
                    </button>
                    </div>
                    <div className="mode-controls">
                    <button
                      onClick={() => handleModeChange("original")}
                      className={`ui-button player-mode-button transition-all ${
                        playbackMode === "original" ? "is-active" : ""
                      }`}
                    >
                      原唱
                    </button>
                    <button
                      onClick={() => handleModeChange("instrumental")}
                      className={`ui-button player-mode-button transition-all ${
                        playbackMode === "instrumental" ? "is-active" : ""
                      }`}
                    >
                      伴奏
                    </button>
                    <button
                      onClick={() => handleModeChange("vocals")}
                      className={`ui-button player-mode-button transition-all ${
                        playbackMode === "vocals" ? "is-active" : ""
                      }`}
                    >
                      人声
                    </button>
                    </div>
                    <div className="volume-controls">
                      <button onClick={() => handleVolumeChange(volume > 0 ? 0 : 80)} className="ui-icon-button player-volume-button text-[var(--text-secondary)] transition-colors hover:bg-[var(--button-hover-bg)] hover:text-[var(--text-primary)]">
                        {volume > 0 ? (
                          <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
                            <path d="M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02z"/>
                          </svg>
                        ) : (
                          <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
                            <path d="M16.5 12c0-1.77-1.02-3.29-2.5-4.03v2.21l2.45 2.45c.03-.2.05-.41.05-.63zm2.5 0c0 .94-.2 1.82-.54 2.64l1.51 1.51C20.63 14.91 21 13.5 21 12c0-4.28-2.99-7.86-7-8.77v2.06c2.89.86 5 3.54 5 6.71zM4.27 3L3 4.27 7.73 9H3v6h4l5 5v-6.73l4.25 4.25c-.67.52-1.42.93-2.25 1.18v2.06c1.38-.31 2.63-.95 3.69-1.81L19.73 21 21 19.73l-9-9L4.27 3zM12 4L9.91 6.09 12 8.18V4z"/>
                          </svg>
                        )}
                      </button>
                      <div
                        className="volume-track cursor-pointer"
                        onClick={(event) => {
                          const rect = event.currentTarget.getBoundingClientRect();
                          const pct = (event.clientX - rect.left) / rect.width;
                          const next = Math.max(0, Math.min(100, Math.round(pct * 100)));
                          handleVolumeChange(next);
                        }}
                      >
                        <div className="volume-fill h-full rounded-full transition-all" style={{ width: `${volume}%` }} />
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            ) : (
              <div className="h-full flex flex-col items-center justify-center text-[var(--text-muted)]">
                <div className="text-4xl mb-4">🎤</div>
                <div className="text-sm">从左侧列表选择歌曲</div>
                <div className="text-xs text-[var(--text-muted)] mt-2">使用右上“导入歌曲”按钮添加音乐</div>
              </div>
            )}
          </div>
        </div>
      </div>

      {fileStorageSettingsOpen && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center p-6">
          <div
            className="absolute inset-0 bg-black/55 backdrop-blur-[2px]"
            onClick={() => setFileStorageSettingsOpen(false)}
          />
          <div
            data-debug-id="preferences-modal"
            className="theme-aware-surface relative flex h-[78vh] w-full max-w-[1480px] overflow-hidden rounded-[24px] border border-[var(--panel-accent-border)] bg-[var(--bg-secondary)] shadow-[0_0_0_1px_var(--panel-inner-border),0_20px_60px_rgba(0,0,0,0.35),0_14px_38px_var(--panel-glow)] backdrop-blur-xl"
          >
            <button
              type="button"
              className="settings-close-button"
              onClick={() => setFileStorageSettingsOpen(false)}
            >
              <span>关闭</span>
              <span className="settings-close-button-mark">×</span>
            </button>
            <aside
              data-debug-id="settings-sidebar"
              className="settings-sidebar theme-subtle-surface flex w-[312px] min-w-[300px] shrink-0 flex-col border-r border-[rgba(148,163,184,0.16)] bg-[var(--bg-secondary)] px-6 py-7"
            >
              <div className="settings-sidebar-header">
                <div className="settings-sidebar-title text-[22px] font-bold leading-[1.2] tracking-tight text-[var(--text-primary)]">偏好设置</div>
              </div>
              <div aria-hidden="true" className="h-7" />
              <div className="settings-sidebar-nav flex flex-col gap-2">
                {SETTINGS_NAV_ITEMS.map(({ label, pane, hint, icon }) => {
                  const active = settingsPane === pane;
                  return (
                    <button
                      key={pane}
                      type="button"
                      onClick={() => setSettingsPane(pane)}
                      className={`settings-nav-item flex h-14 w-full items-center gap-3 rounded-[14px] px-3.5 text-left transition-colors focus:outline-none focus-visible:ring-1 focus-visible:ring-[var(--accent)] ${
                        active
                          ? "bg-[color-mix(in_srgb,var(--accent)_8%,var(--bg-tertiary)_65%)] text-[var(--text-primary)]"
                          : "text-[var(--text-secondary)] hover:bg-[rgba(148,163,184,0.08)]"
                      }`}
                    >
                      <span
                        className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-[10px] text-[14px] font-bold ${
                          active ? "bg-[color-mix(in_srgb,var(--accent)_22%,transparent)] text-[var(--accent)]" : "bg-[var(--bg-card)] text-[var(--text-muted)]"
                        }`}
                      >
                        {icon}
                      </span>
                      <span className="min-w-0">
                        <span className={`block truncate text-[15px] font-bold leading-[1.2] ${active ? "text-[var(--text-primary)]" : ""}`}>{label}</span>
                        <span className="mt-1 block truncate text-[12px] leading-[1.2] text-[var(--text-muted)]">{hint}</span>
                      </span>
                    </button>
                  );
                })}
              </div>
            </aside>

            <main className="min-w-0 flex-1 overflow-y-auto">
              <div
                data-debug-id="settings-main"
                className="settings-main flex min-h-full w-full overflow-auto px-14 py-12"
              >
                <div data-debug-id="settings-main-inner" className="settings-main-inner flex w-full max-w-[1120px] flex-col">
                  <div className="settings-page-header mb-6 max-w-[820px]">
                    <div data-debug-id="settings-page-title" className="settings-page-title text-[36px] font-extrabold leading-[1.12] tracking-tight text-[var(--text-primary)]">
                      {SETTINGS_PAGE_COPY[settingsPane].title}
                    </div>
                    <div className="settings-page-description mt-2 text-[15px] leading-6 text-[var(--text-secondary)]">
                      {SETTINGS_PAGE_COPY[settingsPane].description}
                    </div>
                  </div>

                  {settingsPane === "paths" ? (
                    !fileStorageSettings ? (
                      <div className="ui-loading-state max-w-[760px]">
                        <div className="ui-loading-label">正在加载文件管理设置...</div>
                        <div className="ui-progress-track" aria-hidden="true">
                          <div className="ui-progress-fill progress-shimmer" style={{ width: "42%" }} />
                        </div>
                      </div>
                    ) : (
                      <div className="flex w-full flex-col gap-4">
                        {([
                          ["伴奏目录", "instrumentalRoot", "自动保存分离后的伴奏文件", "♪"],
                          ["人声目录", "vocalsRoot", "自动保存分离后的人声文件", "●"],
                          ["歌词目录", "lyricsRoot", "自动保存歌词 JSON / LRC 文件", "▤"],
                        ] as Array<[string, keyof FileStorageSettings, string, string]>).map(([label, field, hint, icon]) => (
                          <div
                            key={field}
                            className="path-card rounded-[16px] border border-[rgba(148,163,184,0.16)] bg-[var(--bg-card)] p-4 transition-colors"
                          >
                            <div className="path-card-header mb-4 flex items-start justify-between gap-5">
                              <div className="flex min-w-0 items-start gap-4">
                                <div className="flex h-11 w-11 shrink-0 items-center justify-center rounded-[12px] bg-[color-mix(in_srgb,var(--accent)_18%,var(--bg-tertiary))] text-[20px] font-bold text-[var(--accent)]">
                                  {icon}
                                </div>
                                <div className="min-w-0">
                                  <div className="path-card-title truncate text-[16px] font-bold leading-[1.3] tracking-tight text-[var(--text-primary)]">
                                    {label}
                                  </div>
                                  <div className="path-card-description mt-1 text-[13px] leading-[1.4] text-[var(--text-secondary)]">{hint}</div>
                                </div>
                              </div>
                              <button
                                type="button"
                                className="path-card-action inline-flex h-10 shrink-0 items-center justify-center whitespace-nowrap rounded-[12px] border border-[color-mix(in_srgb,var(--accent)_35%,transparent)] px-4 text-[13px] font-semibold text-[var(--accent)] transition-colors hover:bg-[color-mix(in_srgb,var(--accent)_10%,transparent)] focus:outline-none focus-visible:ring-1 focus-visible:ring-[var(--accent)] disabled:cursor-not-allowed disabled:opacity-60"
                                onClick={() => void handleChooseStorageFolder(field)}
                                disabled={fileStorageSettingsSaving}
                              >
                                选择目录
                              </button>
                            </div>
                            <input
                              type="text"
                              value={fileStorageSettings[field]}
                              title={fileStorageSettings[field]}
                              onChange={(event) =>
                                setFileStorageSettings((prev) =>
                                  prev ? { ...prev, [field]: event.target.value } : prev
                                )
                              }
                              placeholder="留空则恢复默认目录"
                              className="path-input h-11 w-full min-w-0 truncate rounded-[12px] border border-[rgba(148,163,184,0.16)] bg-[var(--bg-primary)] px-3.5 text-sm text-[var(--text-primary)] outline-none transition-colors placeholder:text-[var(--text-muted)] focus:border-[var(--accent)] focus-visible:ring-1 focus-visible:ring-[var(--accent)]"
                            />
                          </div>
                        ))}

                        {fileStorageSettingsMessage && (
                          <div className="rounded-[14px] border border-[rgba(148,163,184,0.16)] bg-[var(--bg-card)] px-4 py-3 text-sm text-[var(--text-secondary)]">
                            {fileStorageSettingsMessage}
                          </div>
                        )}

                        <div className="settings-actions mt-2 flex items-center justify-between gap-4 border-t border-[rgba(148,163,184,0.16)] pt-5">
                          <button
                            type="button"
                            className="inline-flex h-10 items-center justify-center whitespace-nowrap rounded-[12px] px-4 text-sm font-semibold text-[var(--text-secondary)] transition-colors hover:bg-[rgba(148,163,184,0.08)] focus:outline-none focus-visible:ring-1 focus-visible:ring-[var(--accent)] disabled:cursor-not-allowed disabled:opacity-60"
                            onClick={handleResetStorageSettings}
                            disabled={fileStorageSettingsSaving || !fileStorageSettings}
                          >
                            恢复默认路径
                          </button>
                          <div className="settings-actions-right flex items-center gap-3">
                            <button
                              type="button"
                              className="inline-flex h-10 items-center justify-center whitespace-nowrap rounded-[12px] px-4 text-sm font-semibold text-[var(--text-secondary)] transition-colors hover:bg-[rgba(148,163,184,0.08)] focus:outline-none focus-visible:ring-1 focus-visible:ring-[var(--accent)]"
                              onClick={() => setFileStorageSettingsOpen(false)}
                            >
                              取消
                            </button>
                            <button
                              type="button"
                              className="inline-flex h-10 min-w-[128px] items-center justify-center whitespace-nowrap rounded-[12px] bg-[var(--accent)] px-[18px] text-sm font-bold text-white transition-colors hover:bg-[var(--accent-hover)] focus:outline-none focus-visible:ring-1 focus-visible:ring-[var(--accent)] disabled:cursor-not-allowed disabled:opacity-60"
                              onClick={() => void handleSaveStorageSettings()}
                              disabled={fileStorageSettingsSaving || !fileStorageSettings}
                            >
                              {fileStorageSettingsSaving ? "保存中..." : "保存并迁移"}
                            </button>
                          </div>
                        </div>
                      </div>
                    )
                  ) : settingsPane === "audioOutput" ? (
                    <div className="flex w-full flex-col gap-5">
                      <div className="settings-card max-w-[760px] rounded-[16px] border border-[rgba(148,163,184,0.16)] bg-[var(--bg-card)] p-5">
                        <div className="flex items-start justify-between gap-4">
                          <div className="min-w-0">
                            <div className="text-[18px] font-bold leading-[1.3] tracking-tight text-[var(--text-primary)]">输出设备</div>
                            <div className="mt-2 max-w-[560px] text-[13px] leading-5 text-[var(--text-secondary)]">
                              选择用于播放预览音频的输出设备。若设备未显示，请先授权浏览器音频权限。
                            </div>
                          </div>
                          <button
                            type="button"
                            className="inline-flex h-10 shrink-0 items-center justify-center rounded-[12px] border border-[color-mix(in_srgb,var(--accent)_35%,transparent)] px-4 text-[13px] font-semibold text-[var(--accent)] transition-colors hover:bg-[color-mix(in_srgb,var(--accent)_10%,transparent)] focus:outline-none focus-visible:ring-1 focus-visible:ring-[var(--accent)]"
                            onClick={() => void refreshAudioOutputDevices()}
                          >
                            刷新设备
                          </button>
                        </div>
                        <div className="mt-5">
                          <select
                            value={audioOutputDeviceId}
                            onChange={(e) => setAudioOutputDeviceId(e.target.value)}
                            style={{ backgroundColor: "var(--bg-primary)", color: "var(--text-primary)" }}
                            className="h-10 w-full max-w-[560px] rounded-[10px] border border-[rgba(148,163,184,0.16)] px-3.5 text-[14px] outline-none transition-colors focus:border-[var(--accent)] focus-visible:ring-1 focus-visible:ring-[var(--accent)]"
                          >
                            <option value="default" style={{ backgroundColor: "var(--bg-primary)", color: "var(--text-primary)" }}>系统默认</option>
                            {audioOutputDevices.map((d) => (
                              <option key={d.deviceId} value={d.deviceId} style={{ backgroundColor: "var(--bg-primary)", color: "var(--text-primary)" }}>
                                {d.label}
                              </option>
                            ))}
                          </select>
                          <div className="ui-info-banner mt-4 max-w-[560px]">
                            <span className="ui-info-icon">i</span>
                            <span className="ui-info-text">
                              {audioOutputDeviceId === "default"
                                ? "当前：使用系统默认输出设备"
                                : `当前：${audioOutputDevices.find((d) => d.deviceId === audioOutputDeviceId)?.label ?? audioOutputDeviceId}`}
                              {audioOutputDevices.length <= 1 ? "。需要浏览器授权后才能列出完整设备。" : ""}
                            </span>
                          </div>
                          {audioOutputSupport === "unsupported" && (
                            <div className="mt-3 max-w-[560px] rounded-[12px] border border-amber-400/20 bg-amber-400/[0.06] px-3 py-2 text-[12px] text-amber-200/80">
                              当前环境不支持选择输出设备，声音将使用系统默认输出。
                            </div>
                          )}
                        </div>
                      </div>
                    </div>
                  ) : settingsPane === "appearance" ? (
                    <div className="grid w-full gap-5 md:grid-cols-2">
                      {COLOR_THEMES.map((theme) => {
                        const active = colorTheme === theme.id;
                        return (
                          <button
                            key={theme.id}
                            type="button"
                            onClick={() => setColorTheme(theme.id)}
                            className={`settings-theme-card group relative flex h-[164px] min-w-0 flex-col items-start rounded-[16px] border p-4 text-left transition-colors focus:outline-none focus-visible:ring-1 focus-visible:ring-[var(--accent)] ${
                              active
                                ? "border-[color-mix(in_srgb,var(--accent)_75%,transparent)] bg-[var(--bg-card)] shadow-[0_0_0_1px_color-mix(in_srgb,var(--accent)_14%,transparent)]"
                                : "border-[rgba(148,163,184,0.16)] bg-[var(--bg-card)] hover:border-[rgba(148,163,184,0.28)] hover:bg-[var(--bg-tertiary)]"
                            }`}
                          >
                            {active && (
                              <span className="absolute right-3 top-3 inline-flex h-6 items-center justify-center rounded-full bg-[var(--accent)] px-2 text-[11px] font-bold text-white">
                                已选择
                              </span>
                            )}
                            <div className="flex w-full min-w-0 items-start gap-3 pr-16">
                              <div className="shrink-0">
                                <span
                                  className="theme-swatch settings-theme-swatch block h-10 w-10 rounded-[12px] border border-white/10"
                                  style={{
                                    "--theme-bg": theme.bg,
                                    "--theme-card": theme.card,
                                  } as CSSProperties & Record<string, string>}
                                />
                              </div>
                              <div className="min-w-0">
                                <div className="truncate text-[17px] font-bold leading-[1.25] text-[var(--text-primary)]">
                                  {theme.name}
                                </div>
                                <div className="mt-1 line-clamp-2 text-[13px] leading-5 text-[var(--text-secondary)]">
                                  {theme.description}
                                </div>
                              </div>
                            </div>
                            <div className="mt-4 flex w-full items-center gap-2">
                              {[theme.bg, theme.card, theme.accent, theme.text].map((color) => (
                                <span
                                  key={color}
                                  className="settings-theme-color h-8 min-w-0 flex-1 rounded-[8px] border border-white/10"
                                  style={{ backgroundColor: color }}
                                />
                              ))}
                            </div>
                            <div className="mt-3 line-clamp-2 text-[12px] leading-5 text-[var(--text-secondary)]">
                              示例文字：歌词编辑、依赖状态与按钮文本保持清晰可读。
                            </div>
                          </button>
                        );
                      })}
                    </div>
                  ) : settingsPane === "about" ? (
                    <div className="flex w-full max-w-[860px] flex-col gap-4">
                      <div className="settings-card rounded-[16px] border border-[rgba(148,163,184,0.16)] bg-[var(--bg-card)] px-6 py-5">
                        <div className="grid min-h-12 gap-4 text-[14px] leading-6 text-[var(--text-secondary)] md:grid-cols-2">
                          <div className="min-w-0">
                            <div className="text-[13px] font-semibold text-[var(--text-muted)]">版本号</div>
                            <div className="ui-text-ellipsis mt-1 text-[18px] font-bold text-[var(--text-primary)]" title={APP_VERSION}>{APP_VERSION}</div>
                          </div>
                          <div className="min-w-0">
                            <div className="text-[13px] font-semibold text-[var(--text-muted)]">作者</div>
                            <div className="ui-text-ellipsis mt-1 text-[18px] font-bold text-[var(--text-primary)]" title="-捅捅-（B 站 UID：1519262）">-捅捅-（B 站 UID：1519262）</div>
                          </div>
                        </div>
                      </div>
                      <div className="settings-card rounded-[16px] border border-[rgba(148,163,184,0.16)] bg-[var(--bg-card)] px-7 py-6">
                        <div className="text-[17px] font-bold text-[var(--text-primary)]">声明</div>
                        <p className="ui-copy mt-3 text-[14px] leading-[1.75] text-[var(--text-secondary)]">
                          GitHub 项目名为《4isfstools》，此软件一般用于在哪都找不到伴奏的那些歌，本软件仅提供音频处理、学习与研究用途，不提供或分发受版权保护的音频内容。用户应确保其处理的音频文件来源合法，并拥有相应版权或使用授权。因用户上传、处理、导出或传播音频内容所产生的版权及其他法律责任，由用户自行承担。
                        </p>
                      </div>
                      <div className="settings-card rounded-[16px] border border-[rgba(148,163,184,0.16)] bg-[var(--bg-card)] px-7 py-6">
                        <div className="text-[17px] font-bold text-[var(--text-primary)]">开源声明</div>
                        <p className="ui-copy mt-3 text-[14px] text-[var(--text-secondary)]">
                          本软件使用了以下开源项目，相关版权归原作者或贡献者所有，并遵循其对应的开源许可证：
                        </p>
                        <div className="ui-chip-wrap mt-4">
                          {[
                            "FFmpeg — FFmpeg Developers",
                            "PyTorch — PyTorch Contributors / Linux Foundation",
                            "Demucs — Meta AI / Facebook Research",
                            "Whisper — OpenAI",
                            "SoundFile / python-soundfile — Bastibe and contributors",
                            "NumPy — NumPy Developers",
                            "SciPy — SciPy Developers",
                            "Tauri — Tauri Programme within The Commons Conservancy",
                            "React — Meta Platforms, Inc.",
                            "Vite — Evan You and Vite Contributors",
                            "163MusicLyrics — jitwxs",
                          ].map((item) => (
                            <div key={item} className="ui-chip flex-[1_1_320px]" title={item}>
                              <span>{item}</span>
                            </div>
                          ))}
                        </div>
                      </div>
                      <div className="settings-card rounded-[16px] border border-[rgba(148,163,184,0.16)] bg-[var(--bg-card)] px-7 py-6">
                        <div className="text-[17px] font-bold text-[var(--text-primary)]">鸣谢</div>
                        <div className="ui-chip-wrap mt-3">
                          {["零度天堂（BUID：448187）", "达宝Doublemint（BUID：5854007）", "杠杠（BUID：3493291207166696）"].map((name) => (
                            <span key={name} className="ui-chip" title={name}><span>{name}</span></span>
                          ))}
                        </div>
                      </div>
                    </div>
                  ) : (
                    <div className="flex w-full flex-col gap-5">
                      <div data-debug-id="env-summary-card" className="env-summary-card rounded-[16px] border border-[rgba(148,163,184,0.16)] bg-[var(--bg-card)] p-4 shadow-[0_1px_0_rgba(255,255,255,0.03)_inset]">
                        <div className="flex items-center justify-between gap-5">
                          <div className="flex min-w-0 items-start gap-3">
                            <div
                              className={`flex h-11 w-11 shrink-0 items-center justify-center rounded-[13px] text-[22px] ${
                                runtimeHealth?.level === "ready"
                                  ? "bg-emerald-400/10 text-emerald-300"
                                : runtimeHealth?.level === "warning"
                                  ? "bg-amber-400/10 text-amber-300"
                                  : "bg-rose-400/10 text-rose-300"
                              }`}
                            >
                              {runtimeHealth?.level === "ready" ? "✓" : "!"}
                            </div>
                            <div className="min-w-0">
                              <div className="text-[18px] font-bold leading-[1.25] tracking-tight text-[var(--text-primary)]">
                                {runtimeHealth?.level === "ready" ? "环境可运行" : "需要处理"}
                              </div>
                              <div className="mt-1 max-w-[620px] truncate text-[13px] leading-5 text-[var(--text-secondary)]" title={bootstrapStatus?.detail ?? runtimeHealth?.detail ?? "正在获取环境状态..."}>
                                {runtimeHealth?.level === "ready" ? "核心依赖已就绪，可运行人声分离。" : bootstrapStatus?.detail ?? runtimeHealth?.detail ?? "正在获取环境状态..."}
                              </div>
                            </div>
                          </div>
                          <div className="flex shrink-0 items-center gap-3">
                            <button
                              type="button"
                              className="inline-flex h-10 items-center justify-center whitespace-nowrap rounded-[12px] border border-[rgba(148,163,184,0.16)] px-4 text-[13px] font-semibold text-[var(--text-secondary)] transition-colors hover:bg-[rgba(148,163,184,0.08)] focus:outline-none focus-visible:ring-1 focus-visible:ring-[var(--accent)]"
                              onClick={() => void handleRefreshRuntimeHealth()}
                            >
                              重新检测
                            </button>
                            <button
                              type="button"
                              className="inline-flex h-10 min-w-[148px] items-center justify-center rounded-[12px] bg-[var(--accent)] px-[18px] text-[13px] font-bold text-white transition-colors hover:bg-[var(--accent-hover)] focus:outline-none focus-visible:ring-1 focus-visible:ring-[var(--accent)] disabled:cursor-not-allowed disabled:opacity-60"
                              onClick={() => void handleBootstrapInstall()}
                              disabled={bootstrapInstalling}
                            >
                              {bootstrapInstalling ? "安装中..." : "安装/修复运行环境"}
                            </button>
                          </div>
                        </div>
                        {bootstrapMessage && (
                          <div className="mt-3 rounded-[12px] border border-[color-mix(in_srgb,var(--accent)_22%,transparent)] bg-[color-mix(in_srgb,var(--accent)_8%,transparent)] px-3 py-2 text-[12px] text-[var(--accent)]">{bootstrapMessage}</div>
                        )}
                      </div>

                      <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
                        {[
                          ["NVIDIA GPU", runtimeHasNvidiaGpu ? "已检测" : "未检测"],
                          ["Torch 版本", runtimeTorchVersion ?? "未安装"],
                          ["Torch CUDA", runtimeTorchCudaAvailable ? "可用" : "不可用"],
                          ["CUDA 版本", runtimeTorchCudaVersion ?? "无"],
                          ["运行设备", runtimeSelectedDevice],
                          ["GPU 设备名", runtimeTorchCudaDeviceName ?? "无"],
                        ].map(([label, value]) => (
                          <div key={label} className="runtime-info-card flex h-16 min-w-0 flex-col justify-center rounded-[12px] border border-[rgba(148,163,184,0.16)] bg-[var(--bg-card)] px-4">
                            <div className="truncate text-[12px] font-semibold text-[var(--text-muted)]">{label}</div>
                            <div className="mt-1 truncate text-[15px] font-bold text-[var(--text-primary)]" title={value}>{value}</div>
                          </div>
                        ))}
                      </div>

                      <div className="flex items-center justify-between gap-4">
                        <div className="text-[16px] font-bold text-[var(--text-primary)]">检测项目</div>
                        <div className="ui-chip text-[12px]" title="当前已返回项目数 / 预期检测项目数">
                          <span>{runtimeCheckCountLabel}</span>
                        </div>
                      </div>

                      <div data-debug-id="dependency-list" className="dependency-list grid w-full gap-3 lg:grid-cols-2">
                        {displayedRuntimeChecks.map((check) => (
                          <div
                            key={check.name}
                            data-debug-id="dependency-card"
                            className="dependency-card flex h-[68px] min-w-0 items-center gap-3 rounded-[14px] border border-[rgba(148,163,184,0.16)] bg-[var(--bg-card)] px-4 transition-colors"
                          >
                            <span
                              data-debug-id="status-dot"
                              className={`h-2.5 w-2.5 shrink-0 rounded-full ${
                                check.ok
                                  ? "bg-emerald-300"
                                  : check.severity === "warning"
                                    ? "bg-amber-300"
                                    : check.severity === "info"
                                      ? "bg-sky-300"
                                      : "bg-rose-300"
                              }`}
                            />
                            <div className="min-w-0 flex-1">
                              <div className="truncate text-[15px] font-bold leading-[1.25] tracking-tight text-[var(--text-primary)]">
                                {check.name}
                              </div>
                              {check.detail && (
                                <div className="mt-1 truncate text-[12px] leading-[1.3] text-[var(--text-muted)]" title={check.detail}>{check.detail}</div>
                              )}
                            </div>
                            <div
                              data-debug-id="status-badge"
                              className={`status-badge ${
                                check.ok
                                  ? "status-badge-ok"
                                  : check.severity === "warning"
                                    ? "status-badge-warning"
                                    : check.severity === "info"
                                      ? "status-badge-info"
                                      : "status-badge-error"
                              }`}
                            >
                              {check.ok ? "正常" : check.severity === "warning" ? "注意" : check.severity === "info" ? "未确认" : "异常"}
                            </div>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              </div>
            </main>
          </div>
        </div>
      )}

      {lyricsCandidateSong && lyricsCandidateOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-8">
          <div
            className="absolute inset-0 bg-black/55 backdrop-blur-[2px]"
            onClick={closeLyricsCandidateModal}
          />
          <div className="theme-aware-surface relative flex flex-col w-full overflow-hidden rounded-[22px] border border-[var(--panel-accent-border)] bg-[var(--bg-secondary)] shadow-[0_0_0_1px_var(--panel-inner-border),0_24px_70px_rgba(0,0,0,0.42),0_14px_38px_var(--panel-glow)]"
            style={{ width: "min(820px, calc(100vw - 64px))", maxHeight: "min(720px, calc(100vh - 64px))" }}>
            {/* Header */}
            <div className="grid grid-cols-[minmax(0,1fr)_auto] items-start gap-4 px-7 pt-6" style={{ padding: "24px 28px 14px" }}>
              <div className="min-w-0">
                <div className="text-[24px] font-extrabold leading-[1.2] text-[var(--text-primary)]">选择歌词候选</div>
                <div className="ui-text-ellipsis mt-1.5 text-[14px] font-semibold leading-[1.35] text-[var(--text-secondary)]" title={lyricsCandidateSong.name}>{lyricsCandidateSong.name}</div>
              </div>
              <button
                type="button"
                className="shrink-0 h-[32px] w-[32px] rounded-[8px] text-[18px] font-normal text-[var(--text-secondary)] bg-transparent hover:bg-[var(--ghost-button-hover-bg)] hover:text-[var(--text-primary)] focus:outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--focus-ring)]"
                onClick={closeLyricsCandidateModal}
                aria-label="关闭"
              >
                ×
              </button>
            </div>

            {/* SearchBar */}
            <div className="grid grid-cols-[minmax(0,1fr)_88px] gap-3 items-center px-[28px]" style={{ padding: "0 28px 16px" }}>
              <div className="relative flex h-[42px] min-w-0 flex-1 items-center overflow-hidden rounded-[13px] border border-[var(--input-border)] bg-[var(--input-bg)]">
                <span className="flex h-full w-[40px] flex-shrink-0 items-center justify-center">
                  <AppSearchIcon className="h-[18px] w-[18px] text-[var(--text-muted)]" />
                </span>
                <input
                  type="text"
                  value={lyricsSearchQuery}
                  onChange={(event) => setLyricsSearchQuery(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key !== "Enter" || !lyricsCandidateSong || lyricsCandidateLoading) return;
                    event.preventDefault();
                    void handleSearchLyrics(lyricsCandidateSong, lyricsSearchQuery);
                  }}
                  placeholder="输入关键词，例如歌手名、歌名、专辑名"
                  className="flex-1 min-w-0 h-full bg-transparent pr-[14px] text-[15px] text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none"
                />
              </div>
              <button
                type="button"
                className="h-[42px] min-w-[88px] px-[16px] rounded-[13px] text-[15px] font-bold text-[var(--primary-button-text)] bg-[var(--primary-button-bg)] hover:bg-[var(--accent-hover)] disabled:cursor-not-allowed disabled:opacity-60"
                disabled={lyricsCandidateLoading || !lyricsCandidateSong || !lyricsSearchQuery.trim()}
                onClick={() => {
                  if (lyricsCandidateSong) {
                    void handleSearchLyrics(lyricsCandidateSong, lyricsSearchQuery);
                  }
                }}
              >
                {lyricsCandidateLoading ? "搜索中..." : "搜索"}
              </button>
            </div>

            {/* CandidateList */}
            <div className="flex-1 overflow-y-auto px-[28px] pb-[18px] flex flex-col gap-3" style={{ padding: "12px 28px 18px" }}>
            {lyricsCandidateError && (
              <div className="max-h-40 overflow-auto rounded-[13px] border border-[var(--danger-border)] bg-[var(--danger-soft)] px-4 py-3 text-[14px] leading-6 text-[var(--danger)]">
                {lyricsCandidateError}
              </div>
            )}

            {lyricsCandidateLoading && !lyricsCandidates && !lyricsCandidateError && (
              <div className="rounded-[13px] border border-[var(--border)] bg-[var(--bg-card)] px-4 py-3 text-[14px] text-[var(--text-secondary)]">
                搜索中...
              </div>
            )}

            {lyricsCandidates && lyricsCandidates.length > 0 && (
              <div className="flex flex-col gap-3 pb-2">
                {lyricsCandidates.map((candidate) => (
                  <button
                    key={candidate.id}
                    type="button"
                    onClick={() => void handleApplyLyricsCandidate(candidate)}
                    className="group relative grid grid-cols-[minmax(0,1fr)_auto] gap-4 p-[16px_18px] min-h-[128px] rounded-[16px] border border-[var(--border-soft)] bg-[var(--surface-card)] text-left transition-all hover:border-[var(--selected-border)] hover:bg-[var(--bg-tertiary)]"
                  >
                    {/* MainContent */}
                    <div className="min-w-0 overflow-hidden">
                      <div className="text-[16px] font-extrabold leading-[1.3] text-[var(--text-primary)] line-clamp-2" title={candidate.title}>{candidate.title}</div>
                      <div className="ui-text-ellipsis mt-1 text-[13px] font-semibold leading-[1.35] text-[var(--text-secondary)]" title={`${candidate.artist || "未知歌手"}${candidate.album ? ` · ${candidate.album}` : ""}`}>
                        {candidate.artist || "未知歌手"}
                        {candidate.album ? ` · ${candidate.album}` : ""}
                      </div>
                      <div className="mt-2 text-[14px] leading-[1.55] text-[var(--text-secondary)] line-clamp-4 overflow-wrap-anywhere" style={{ overflowWrap: "anywhere", wordBreak: "break-word" }}>
                        {candidate.preview || "（无预览）"}
                      </div>
                      <div className="mt-3 text-[12px] text-[var(--text-muted)]">
                        点击采用此候选
                      </div>
                    </div>
                    {/* MetaColumn */}
                    <div className="flex flex-col items-end gap-2 pt-1" style={{ minWidth: "84px", maxWidth: "128px" }}>
                      <span className="h-[28px] max-w-[112px] overflow-hidden text-ellipsis whitespace-nowrap rounded-[999px] border border-[var(--chip-border)] bg-[var(--chip-bg)] px-[10px] text-[12px] font-bold leading-[28px] text-[var(--text-secondary)]" title={candidate.sourceLabel}>
                        {candidate.sourceLabel}
                      </span>
                      <span className="h-[28px] rounded-[999px] border border-[var(--border-soft)] bg-[var(--surface-muted)] px-[10px] text-[12px] font-bold leading-[28px] text-[var(--accent)]">
                        {candidate.score}
                      </span>
                    </div>
                  </button>
                ))}
              </div>
            )}

            {lyricsCandidates && lyricsCandidates.length === 0 && !lyricsCandidateLoading && !lyricsCandidateError && (
              <div className="rounded-[13px] border border-[var(--border)] bg-[var(--bg-card)] px-4 py-3 text-[14px] text-[var(--text-secondary)]">
                没有找到可用的歌词候选，可以换个关键词继续搜索。
              </div>
            )}
            </div>

            {/* Footer */}
            <div className="flex items-center justify-end gap-3 px-[28px] py-5" style={{ padding: "16px 28px 24px" }}>
              <button
                type="button"
                className="h-[40px] min-w-[88px] rounded-[12px] px-[18px] text-[14px] font-bold text-[var(--text-secondary)] bg-transparent hover:bg-[var(--button-hover-bg)] focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-ring)]"
                onClick={closeLyricsCandidateModal}
              >
                取消
              </button>
            </div>
          </div>
        </div>
      )}

    </div>
  );
}

export default App;
