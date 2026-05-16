import { useState, useRef, useEffect, useCallback } from "react";
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
  const [settingsPane, setSettingsPane] = useState<"paths" | "runtime" | "audioOutput">("runtime");
  const [fileStorageSettingsSaving, setFileStorageSettingsSaving] = useState(false);
  const [fileStorageSettingsMessage, setFileStorageSettingsMessage] = useState<string | null>(null);
  const [runtimeHealth, setRuntimeHealth] = useState<RuntimeHealthReport | null>(null);
  const [bootstrapStatus, setBootstrapStatus] = useState<BootstrapStatus | null>(null);
  const [bootstrapInstalling, setBootstrapInstalling] = useState(false);
  const [bootstrapMessage, setBootstrapMessage] = useState<string | null>(null);

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
  const gpuRuntimePreferenceStorageKey = "4isfstools.prefer_demucs_cuda";
  const [preferDemucsCuda, setPreferDemucsCuda] = useState(() => {
    if (typeof window === "undefined") {
      return false;
    }
    try {
      return window.localStorage.getItem(gpuRuntimePreferenceStorageKey) === "true";
    } catch {
      return false;
    }
  });

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }
    try {
      window.localStorage.setItem(
        gpuRuntimePreferenceStorageKey,
        preferDemucsCuda ? "true" : "false"
      );
    } catch {
      // ignore persistence failures
    }
  }, [preferDemucsCuda]);

  const demucsGpuRequested = runtimeHasNvidiaGpu && preferDemucsCuda;

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

  const applyAudioOutputDevice = useCallback(async (audio: HTMLAudioElement) => {
    const deviceId = audioOutputDeviceIdRef.current;
    if (!deviceId || deviceId === "default") return;
    // Prefer AudioContext.setSinkId (routes entire Web Audio graph)
    const ctx = audioAnalyserContextRef.current as (AudioContext & { setSinkId?: (id: string) => Promise<void> }) | null;
    if (ctx && typeof ctx.setSinkId === "function") {
      try {
        await ctx.setSinkId(deviceId);
        return;
      } catch (e) {
        console.warn("[audio] AudioContext.setSinkId failed:", e);
      }
    }
    // Fallback: HTMLAudioElement.setSinkId
    try {
      if (typeof audio.setSinkId === "function") {
        await audio.setSinkId(deviceId);
      }
    } catch (e) {
      console.warn("[audio] setSinkId failed, using default output:", e);
    }
  }, []);

  const applyToAllAudioOutputs = useCallback(async () => {
    const deviceId = audioOutputDeviceIdRef.current;
    // Apply to AudioContext if available
    const ctx = audioAnalyserContextRef.current as (AudioContext & { setSinkId?: (id: string) => Promise<void> }) | null;
    if (ctx && typeof ctx.setSinkId === "function" && deviceId && deviceId !== "default") {
      try { await ctx.setSinkId(deviceId); } catch { /* fallback below */ }
    }
    // Apply to active HTMLAudioElements
    if (audioRef.current) void applyAudioOutputDevice(audioRef.current);
    if (originalAudioRef.current) void applyAudioOutputDevice(originalAudioRef.current);
  }, [applyAudioOutputDevice]);

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
    void applyAudioOutputDevice(audio);
    return audio;
  }, [applyAudioOutputDevice]);

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

  const createTrackGraph = useCallback(async (audio: HTMLAudioElement): Promise<TrackGraph | null> => {
    if (!audioAnalyserContextRef.current) {
      audioAnalyserContextRef.current = new AudioContext();
    }
    const context = audioAnalyserContextRef.current;
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
  }, []);

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
      setTrackLevels({
        instrumental: captureLevel(audioGraphRef.current.instrumental),
        vocals: captureLevel(audioGraphRef.current.vocals),
      });
    }, 250);
    return () => window.clearInterval(interval);
  }, []);

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

  const handleBootstrapInstall = useCallback(async () => {
    if (!isDesktopRuntime) return;
    setBootstrapInstalling(true);
    setBootstrapMessage("正在安装运行时与模型...");
    try {
      const status = await invoke<BootstrapStatus>("bootstrap_install_minimal", {
        preferDemucsCuda,
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
  }, [isDesktopRuntime, preferDemucsCuda]);

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
            await invoke("start_process", { songId: song.id, preferDemucsCuda: demucsGpuRequested });
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
  }, [demucsGpuRequested]);

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
      await invoke(command, { songId: song.id, preferDemucsCuda: demucsGpuRequested });
      setSongs((prev) => prev.map((item) =>
        item.id === song.id && item.status !== "processing" && item.status !== "cancelling"
          ? { ...item, status: "queued" as const, progress: 0, processingStage: "queued" as ProcessingStage, error_message: undefined }
          : item
      ));
    } catch (e) {
      console.error("Failed to start separation:", e);
    }
  }, [demucsGpuRequested]);

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
      const instrumentalGraph = await createTrackGraph(audioRef.current);
      if (instrumentalGraph) {
        audioGraphRef.current.instrumental = instrumentalGraph;
      }

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
        const vocalsGraph = await createTrackGraph(originalAudioRef.current);
        if (vocalsGraph) {
          audioGraphRef.current.vocals = vocalsGraph;
        }
      } else {
        originalAudioRef.current = null;
        audioGraphRef.current.vocals = undefined;
      }

      applyModeRouting(volume, nextMode);
      await ensureAudioContextRunning();
      await startPlayback(nextMode, true);
    } catch (e) {
      console.error("Failed to play:", e);
      setPlaybackError(`播放失败: ${e}`);
      setPlayerState("idle");
    }
  }, [songs, loadLyrics, volume, playbackMode, applyModeRouting, stopAllAudio, startPlayback, createAudioTrack, bindAudioError, createTrackGraph, ensureAudioContextRunning]);

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
      await ensureAudioContextRunning();
      await startPlayback(playbackMode, false);
    }
  }, [currentSong, handleSelectSong, playerState, playbackMode, pausePlayback, startPlayback, ensureAudioContextRunning]);

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
      await ensureAudioContextRunning();
    }
  }, [volume, currentSong, applyModeRouting, playerState, ensureAudioContextRunning]);

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
        if (audio) setCurrentTime(audio.currentTime * 1000);
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
      <div className="p-[24px] flex flex-col gap-[18px] h-full">
        {/* Header */}
        <header className="h-14 shrink-0 rounded-xl border border-[var(--border)] bg-[var(--bg-secondary)] flex items-center justify-between" style={{ paddingLeft: '24px', paddingRight: '24px' }}>
          <div className="flex items-center gap-4">
            <img src="/icon.png" alt="Macaron Singer" className="w-7 h-7 rounded-lg object-cover" onError={(e) => e.currentTarget.style.display = 'none'} />
            <h1 className="text-base font-semibold tracking-tight">Macaron Singer</h1>
            <button
              type="button"
              onClick={() => {
                setFileStorageSettingsOpen(true);
                setSettingsPane("runtime");
              }}
              className="ml-2 inline-flex items-center gap-2 rounded-full border border-white/[0.08] bg-white/[0.03] px-3 py-1.5 text-xs text-[#d4d4d8] transition-colors hover:bg-white/[0.05]"
              aria-label="查看运行环境状态"
            >
              <span
                className={`h-2 w-2 rounded-full ${
                  runtimeHealth?.level === "ready"
                    ? "bg-emerald-400"
                    : runtimeHealth?.level === "warning"
                      ? "bg-amber-400"
                      : "bg-rose-400"
                }`}
              />
              <span className="whitespace-nowrap">
                {runtimeHealth?.label ?? "检测中..."}
              </span>
            </button>
            <label
              className={`ml-1 inline-flex items-center gap-2 rounded-full border border-white/[0.08] bg-white/[0.03] px-3 py-1.5 text-xs text-[#d4d4d8] transition-colors ${
                runtimeHasNvidiaGpu ? "hover:bg-white/[0.05] cursor-pointer" : "cursor-not-allowed opacity-45"
              }`}
              title={
                runtimeHasNvidiaGpu
                  ? demucsGpuRequested
                    ? "Demucs 将在可用时优先请求 GPU"
                    : "未启用 GPU 运行"
                  : "未检测到 NVIDIA GPU"
              }
            >
              <input
                type="checkbox"
                checked={demucsGpuRequested}
                disabled={!runtimeHasNvidiaGpu}
                onChange={(event) => setPreferDemucsCuda(event.target.checked)}
                className="h-3.5 w-3.5 rounded border-white/20 bg-white/5 text-indigo-500 accent-indigo-500 focus:ring-0 disabled:cursor-not-allowed"
              />
              <span className="whitespace-nowrap">GPU 运行</span>
            </label>
          </div>
          <div className="flex items-center gap-5">
            <div className="flex items-center gap-2 text-xs text-[var(--text-muted)] whitespace-nowrap">
              <span>已收录</span>
              <span className="text-[13px] font-semibold text-[var(--text-primary)]">
                {readySongCount}
              </span>
              <span>首</span>
            </div>
            <button
              onClick={() => {
                setFileStorageSettingsOpen(true);
                setSettingsPane("paths");
              }}
              className="inline-flex h-9 min-w-[88px] items-center justify-center rounded-full border border-white/10 bg-white/[0.03] px-4 text-xs font-semibold leading-none text-[#d4d4d8] transition-colors hover:bg-white/[0.06]"
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
              className="inline-flex h-9 min-w-[96px] items-center justify-center rounded-full bg-[#6366f1] px-4 text-xs font-semibold leading-none text-white shadow-lg transition-colors hover:bg-[#5558e3] whitespace-nowrap"
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
          <div className="min-w-0 flex-1 rounded-xl border border-[var(--border)] bg-[var(--bg-secondary)] overflow-hidden flex flex-col">
            {currentSong ? (
              <div className="relative flex-1 flex flex-col min-h-0 h-full">
                {/* Track meter */}
                <div className="shrink-0 relative mt-2 h-[88px]">
                  <div className="pointer-events-none absolute left-1/2 top-3 z-20 w-[min(50vw,640px)] -translate-x-1/2">
                    <div className="rounded-[10px] border border-white/[0.06] bg-white/[0.04] px-5 py-3 shadow-[0_14px_32px_rgba(0,0,0,0.22)] backdrop-blur-sm">
                      <div className="space-y-1">
                        {([
                          ["伴奏", trackLevels.instrumental, "#6366f1"],
                          ["人声", trackLevels.vocals, "#22c55e"],
                        ] as Array<[string, number, string]>).map(([label, level, color]) => (
                          <div key={label} className="flex items-center gap-3 text-[9px] text-white/50">
                            <span className="w-7 shrink-0 text-right font-medium">{label}</span>
                            <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-[#2a2a4a]">
                              <div
                                className="h-full rounded-full transition-all duration-150"
                                style={{
                                  width: `${Math.max(2, Math.min(100, level * 200))}%`,
                                  background: color,
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
                <div className="min-h-0 flex-1 flex flex-col px-6 pt-2 pb-[156px]">
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
                      <div className="text-[#3f3f46] text-base text-center py-8">
                        暂无歌词
                      </div>
                    )}
                  </div>
                </div>
                {vocalWaveformEnabled && (
                  <div className="pointer-events-none absolute left-6 right-6 bottom-[126px] z-20">
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
                <div className="shrink-0 h-[112px] px-8 pt-2 pb-2 flex flex-col justify-start border-t border-white/[0.03]">
                  {/* Song Info */}
                  <div className="flex items-center gap-3 mb-2">
                    <div className="w-10 h-10 flex items-center justify-center text-xl">🎵</div>
                    <div className="flex-1 min-w-0">
                      <div className="font-medium text-sm truncate">{currentSong.name}</div>
                      <div className="text-xs text-[#71717a]">
                        {playbackMode === "original" ? "原唱模式" : playbackMode === "vocals" ? "人声模式" : "伴奏模式"}
                      </div>
                      {whisperDraftLoadingSongId === currentSong.id && (
                        <div className="text-xs text-[#a855f7]">AI 听写生成中...</div>
                      )}
                      {whisperDraftError && whisperDraftLoadingSongId !== currentSong.id && (
                        <div className="text-xs text-[#fca5a5] truncate">{whisperDraftError}</div>
                      )}
                      {lyricsImportLoadingSongId === currentSong.id && (
                        <div className="text-xs text-[#60a5fa]">LRC 导入中...</div>
                      )}
                      {lyricsImportError && lyricsImportLoadingSongId !== currentSong.id && (
                        <div className="text-xs text-[#fca5a5] truncate">{lyricsImportError}</div>
                      )}
                      {playbackError && (
                        <div className="text-xs text-[#ef4444]">{playbackError}</div>
                      )}
                    </div>
                  </div>
                  {/* Progress Bar */}
                  <div className="relative mt-1">
                    <button
                      onClick={() => setVocalWaveformEnabled((value) => !value)}
                      className={`absolute right-4 -top-8 z-10 h-8 w-[88px] rounded-full text-[11px] font-semibold leading-none transition-all ${
                        vocalWaveformEnabled ? "bg-[#a855f7] text-white" : "bg-[#1e1e1e] text-[#a1a1aa] hover:bg-[#2a2a4a]"
                      }`}
                    >
                      {vocalWaveformEnabled ? "显示原唱波形" : "隐藏原唱波形"}
                    </button>
                    <div className="flex items-center gap-3">
                      <span className="text-xs text-[#71717a] w-10 text-right font-mono">
                        {formatTime(currentTime)}
                      </span>
                      <div
                        className="flex-1 h-2 bg-[#2a2a4a] rounded-full cursor-pointer"
                        onClick={(e) => {
                          const rect = e.currentTarget.getBoundingClientRect();
                          const pct = (e.clientX - rect.left) / rect.width;
                          if (currentSong.duration > 0) {
                            handleSeek(pct * currentSong.duration);
                          }
                        }}
                      >
                        <div
                          className="h-full bg-gradient-to-r from-[#6366f1] to-[#a855f7] rounded-full transition-all"
                          style={{ width: `${currentSong.duration > 0 ? (currentTime / currentSong.duration) * 100 : 0}%` }}
                        />
                      </div>
                      <span className="text-xs text-[#71717a] w-10 font-mono">
                        {formatTime(currentSong.duration)}
                      </span>
                    </div>
                  </div>

                  <div className="h-1" />

                  {/* Controls Row - centered with enforced separation */}
                  <div className="flex flex-wrap items-center justify-center gap-3">
                    <button onClick={handlePrev} className="p-2 hover:bg-white/10 rounded-full transition-colors text-white/70 hover:text-white">
                      <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                        <path d="M6 6h2v12H6V6zm3.5 6l8.5 6V6l-8.5 6z"/>
                      </svg>
                    </button>
                    <button
                      onClick={handlePlayPause}
                      className="w-10 h-10 bg-white rounded-full flex items-center justify-center hover:scale-105 transition-transform shadow-lg"
                    >
                      {playerState === "playing" ? (
                        <svg className="w-4 h-4 text-[#0f0f23]" fill="currentColor" viewBox="0 0 24 24">
                          <path d="M6 4h4v16H6V4zm8 0h4v16h-4V4z"/>
                        </svg>
                      ) : (
                        <svg className="w-4 h-4 text-[#0f0f23] ml-0.5" fill="currentColor" viewBox="0 0 24 24">
                          <path d="M8 5v14l11-7z"/>
                        </svg>
                      )}
                    </button>
                    <button onClick={handleNext} className="p-2 hover:bg-white/10 rounded-full transition-colors text-white/70 hover:text-white">
                      <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                        <path d="M6 18l8.5-6L6 6v12zm2-8.14L11.03 12 8 14.14V9.86zM16 6h2v12h-2V6z"/>
                      </svg>
                    </button>
                    <button
                      onClick={() => handleModeChange("original")}
                      className={`ml-3 h-9 w-[96px] rounded-full text-sm font-semibold leading-none transition-all ${
                        playbackMode === "original" ? "bg-[#6366f1] text-white" : "bg-[#1e1e1e] text-[#a1a1aa] hover:bg-[#2a2a4a]"
                      }`}
                    >
                      原唱
                    </button>
                    <button
                      onClick={() => handleModeChange("instrumental")}
                      className={`h-9 w-[96px] rounded-full text-sm font-semibold leading-none transition-all ${
                        playbackMode === "instrumental" ? "bg-[#6366f1] text-white" : "bg-[#1e1e1e] text-[#a1a1aa] hover:bg-[#2a2a4a]"
                      }`}
                    >
                      伴奏
                    </button>
                    <button
                      onClick={() => handleModeChange("vocals")}
                      className={`h-9 w-[96px] rounded-full text-sm font-semibold leading-none transition-all ${
                        playbackMode === "vocals" ? "bg-[#22c55e] text-white" : "bg-[#1e1e1e] text-[#a1a1aa] hover:bg-[#2a2a4a]"
                      }`}
                    >
                      人声
                    </button>
                    <div className="flex items-center gap-2 ml-3">
                      <button onClick={() => handleVolumeChange(volume > 0 ? 0 : 80)} className="p-1.5 hover:bg-white/10 rounded-full transition-colors text-white/70 hover:text-white">
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
                        className="w-24 h-2 bg-[#2a2a4a] rounded-full cursor-pointer"
                        onClick={(event) => {
                          const rect = event.currentTarget.getBoundingClientRect();
                          const pct = (event.clientX - rect.left) / rect.width;
                          const next = Math.max(0, Math.min(100, Math.round(pct * 100)));
                          handleVolumeChange(next);
                        }}
                      >
                        <div className="h-full bg-[#6366f1] rounded-full transition-all" style={{ width: `${volume}%` }} />
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            ) : (
              <div className="h-full flex flex-col items-center justify-center text-[#71717a]">
                <div className="text-4xl mb-4">🎤</div>
                <div className="text-sm">从左侧列表选择歌曲</div>
                <div className="text-xs text-[#3f3f46] mt-2">使用右上“导入歌曲”按钮添加音乐</div>
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
          <div data-debug-id="preferences-modal" className="relative flex h-[78vh] w-full max-w-6xl overflow-hidden rounded-[24px] border border-white/[0.10] bg-[rgba(18,18,20,0.88)] shadow-[0_20px_60px_rgba(0,0,0,0.45)] backdrop-blur-xl">
            <button
              type="button"
              className="absolute right-6 top-5 z-10 rounded-[10px] px-[10px] py-[6px] text-sm font-medium text-[#d4d4d8] transition-colors hover:bg-white/[0.06]"
              onClick={() => setFileStorageSettingsOpen(false)}
            >
              关闭
            </button>
            <aside
              data-debug-id="settings-sidebar"
              className="settings-sidebar flex w-[280px] min-w-[260px] shrink-0 flex-col box-border border-r border-white/[0.06] bg-white/[0.025] pl-[32px] pr-[20px] pt-[28px] pb-[24px]"
              style={{ width: 280, flexShrink: 0, paddingTop: 28, paddingRight: 20, paddingBottom: 24, paddingLeft: 32, boxSizing: "border-box" }}
            >
              <div className="settings-sidebar-header">
                <div className="settings-sidebar-title text-[18px] font-bold leading-[1.25] tracking-tight text-[#f5f5f5]">偏好设置</div>
              </div>
              <div aria-hidden="true" className="h-[12px]" />
              <div className="settings-sidebar-nav flex flex-col gap-2">
                {([
                  ["依赖与模型", "runtime", "启动环境与模型状态"],
                  ["声音输出源", "audioOutput", "音频播放设备选择"],
                  ["自定义路径", "paths", "文件归档位置"],
                ] as Array<[string, typeof settingsPane, string]>).map(([label, pane, hint]) => {
                  const active = settingsPane === pane;
                  return (
                    <button
                      key={pane}
                      type="button"
                      onClick={() => setSettingsPane(pane)}
                      className={`settings-nav-item flex w-full flex-col items-start box-border rounded-[16px] px-[14px] py-[12px] text-left transition-colors ${
                        active ? "bg-white/[0.08] text-[#fafafa]" : "text-[#d4d4d8] hover:bg-white/[0.05]"
                      }`}
                    >
                      <span className="text-[15px] font-semibold leading-[1.25] tracking-tight">{label}</span>
                      <span className="mt-[4px] text-[12px] leading-[1.35] text-[#8a8a94]">{hint}</span>
                    </button>
                  );
                })}
              </div>
            </aside>

            <main className="min-w-0 flex-1 overflow-y-auto">
              <div
                data-debug-id="settings-main"
                className="settings-main flex min-h-full w-full box-border overflow-auto pl-[48px] pr-[40px] pb-[40px] pt-[28px]"
                style={{ paddingLeft: 48, paddingRight: 40, paddingTop: 28, paddingBottom: 40 }}
              >
                <div data-debug-id="settings-main-inner" className="settings-main-inner flex w-full max-w-[1040px] flex-col box-border">
                  <div className="settings-page-header mb-[28px] max-w-[760px]">
                    <div data-debug-id="settings-page-title" className="settings-page-title text-[30px] font-[750] leading-[1.15] tracking-tight text-[#f5f5f5]">
                      {settingsPane === "runtime" ? "依赖与模型" : settingsPane === "audioOutput" ? "声音输出源" : "自定义路径"}
                    </div>
                    <div className="settings-page-description mt-2 text-[14px] leading-6 text-white/55">
                      {settingsPane === "runtime"
                        ? "启动时会做最小环境检测，核心依赖异常时会以颜色提示。"
                        : settingsPane === "audioOutput"
                          ? "选择音频播放的输出设备，切换后立即生效。"
                          : "伴奏、人声、歌词会自动归档到指定目录，保存后可随时迁移历史文件。"}
                    </div>
                  </div>

                  {settingsPane === "paths" ? (
                    !fileStorageSettings ? (
                      <div className="path-card rounded-[18px] border border-white/[0.08] bg-white/[0.035] px-[24px] py-[22px] text-sm text-[#a1a1aa]">
                        正在加载文件管理设置...
                      </div>
                    ) : (
                      <div className="flex w-full flex-col gap-[20px]">
                        {([
                          ["伴奏目录", "instrumentalRoot", "自动保存分离后的伴奏文件"],
                          ["人声目录", "vocalsRoot", "自动保存分离后的人声文件"],
                          ["歌词目录", "lyricsRoot", "自动保存歌词 JSON / LRC 文件"],
                        ] as Array<[string, keyof FileStorageSettings, string]>).map(([label, field, hint]) => (
                          <div
                            key={field}
                            className="path-card rounded-[18px] border border-white/[0.08] bg-white/[0.035] px-[24px] py-[22px] transition-colors hover:bg-white/[0.05]"
                          >
                            <div className="path-card-header mb-[18px] flex items-start justify-between gap-5">
                              <div className="flex min-w-0 items-start gap-4">
                                <div className="min-w-0">
                                  <div className="path-card-title text-[16px] font-semibold leading-[1.3] tracking-tight text-[#f5f5f5]">
                                    {label}
                                  </div>
                                  <div className="path-card-description mt-[5px] text-[13px] leading-[1.4] text-white/52">{hint}</div>
                                </div>
                              </div>
                              <button
                                type="button"
                                className="path-card-action inline-flex h-[30px] shrink-0 items-center justify-center rounded-full bg-white/[0.05] px-[14px] text-xs font-medium whitespace-nowrap text-[#d4d4d8] transition-colors hover:bg-white/[0.08]"
                                onClick={() => void handleChooseStorageFolder(field)}
                                disabled={fileStorageSettingsSaving}
                              >
                                选择目录
                              </button>
                            </div>
                            <input
                              type="text"
                              value={fileStorageSettings[field]}
                              onChange={(event) =>
                                setFileStorageSettings((prev) =>
                                  prev ? { ...prev, [field]: event.target.value } : prev
                                )
                              }
                              placeholder="留空则恢复默认目录"
                              className="path-input mt-0 h-[54px] w-full rounded-[16px] border border-white/[0.10] bg-white/[0.04] px-[18px] text-sm text-[#fafafa] outline-none transition-colors placeholder:text-[#52525b] focus:border-[#818cf8]"
                            />
                          </div>
                        ))}

                        {fileStorageSettingsMessage && (
                          <div className="rounded-2xl border border-white/[0.08] bg-white/[0.03] px-4 py-3 text-sm text-[#d4d4d8]">
                            {fileStorageSettingsMessage}
                          </div>
                        )}

                        <div className="settings-actions mt-[28px] flex items-center justify-between gap-4">
                          <button
                            type="button"
                            className="inline-flex min-w-[92px] items-center justify-center whitespace-nowrap rounded-full px-6 py-2.5 text-sm font-medium text-[#d4d4d8] transition-colors hover:bg-white/[0.06]"
                            onClick={handleResetStorageSettings}
                            disabled={fileStorageSettingsSaving || !fileStorageSettings}
                          >
                            恢复默认
                          </button>
                          <div className="settings-actions-right flex items-center gap-[14px]">
                            <button
                              type="button"
                              className="inline-flex min-w-[92px] items-center justify-center whitespace-nowrap rounded-full px-6 py-2.5 text-sm font-medium text-[#d4d4d8] transition-colors hover:bg-white/[0.06]"
                              onClick={() => setFileStorageSettingsOpen(false)}
                            >
                              取消
                            </button>
                            <button
                              type="button"
                              className="inline-flex min-w-[112px] items-center justify-center whitespace-nowrap rounded-full bg-[#6366f1] px-6 py-2.5 text-sm font-semibold text-white transition-colors hover:bg-[#5558e3] disabled:cursor-not-allowed disabled:opacity-60"
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
                    <div className="flex w-full flex-col gap-[20px]">
                      <div className="rounded-[18px] border border-white/[0.08] bg-white/[0.035] px-[24px] py-[22px]">
                        <div className="flex items-center justify-between gap-4">
                          <div>
                            <div className="text-[16px] font-semibold leading-[1.3] tracking-tight text-[#f5f5f5]">声音输出源</div>
                            <div className="mt-[5px] text-[13px] leading-[1.4] text-white/52">
                              选择音频播放的输出设备。需要浏览器授予音频设备权限。
                            </div>
                          </div>
                          <button
                            type="button"
                            className="inline-flex shrink-0 items-center gap-1.5 rounded-full border border-white/[0.10] px-3 py-1.5 text-[12px] font-medium text-[#d4d4d8] transition-colors hover:bg-white/[0.06]"
                            onClick={() => void refreshAudioOutputDevices()}
                          >
                            刷新设备
                          </button>
                        </div>
                        <div className="mt-3">
                          <select
                            value={audioOutputDeviceId}
                            onChange={(e) => setAudioOutputDeviceId(e.target.value)}
                            className="w-full max-w-[420px] rounded-[12px] border border-white/[0.10] bg-white/[0.05] px-4 py-2.5 text-[14px] text-[#f5f5f5] outline-none transition-colors focus:border-[#6366f1]/60"
                          >
                            <option value="default">系统默认</option>
                            {audioOutputDevices.map((d) => (
                              <option key={d.deviceId} value={d.deviceId}>
                                {d.label}
                              </option>
                            ))}
                          </select>
                          {audioOutputDeviceId !== "default" && (
                            <div className="mt-2 text-[12px] text-white/40">
                              当前输出：{audioOutputDevices.find((d) => d.deviceId === audioOutputDeviceId)?.label ?? audioOutputDeviceId}
                            </div>
                          )}
                          {audioOutputSupport === "unsupported" && (
                            <div className="mt-2 rounded-lg border border-amber-400/20 bg-amber-400/[0.06] px-3 py-2 text-[12px] text-amber-200/80">
                              当前环境不支持选择输出设备，声音将使用系统默认输出。
                            </div>
                          )}
                        </div>
                      </div>
                    </div>
                  ) : (
                    <div className="flex w-full flex-col gap-[20px]">
                      <div data-debug-id="env-summary-card" className="env-summary-card rounded-[18px] border border-white/[0.08] bg-white/[0.04] p-[22px] shadow-[0_1px_0_rgba(255,255,255,0.03)_inset]">
                        <div className="flex items-start justify-between gap-5">
                          <div className="min-w-0 max-w-[620px]">
                            <div className="text-[16px] font-semibold leading-[1.3] tracking-tight text-[#f5f5f5]">启动环境检测</div>
                            <div className="mt-[5px] text-[13px] leading-[1.4] text-white/52">
                              启动后自动检测最小运行条件，核心依赖异常会标红或标黄。
                            </div>
                          </div>
                          <div
                            className={`inline-flex shrink-0 items-center gap-2 rounded-full px-3 py-1.5 text-xs font-medium ${
                              runtimeHealth?.level === "ready"
                                ? "border border-emerald-400/20 bg-emerald-400/10 text-emerald-200"
                                : runtimeHealth?.level === "warning"
                                  ? "border border-amber-400/20 bg-amber-400/10 text-amber-200"
                                  : "border border-rose-400/20 bg-rose-400/10 text-rose-200"
                            }`}
                          >
                            <span
                              className={`h-2 w-2 rounded-full ${
                                runtimeHealth?.level === "ready"
                                  ? "bg-emerald-300"
                                  : runtimeHealth?.level === "warning"
                                    ? "bg-amber-300"
                                    : "bg-rose-300"
                              }`}
                            />
                            <span>{runtimeHealth?.label ?? "检测中..."}</span>
                          </div>
                        </div>
                        <div className="mt-4 text-[14px] leading-6 text-[#e5e7eb]">
                          {bootstrapStatus?.detail ?? runtimeHealth?.detail ?? "正在获取环境状态..."}
                        </div>
                        <div className="mt-4 grid gap-2 rounded-2xl border border-white/[0.06] bg-black/20 p-3 text-[12px] leading-5 text-[#d4d4d8] sm:grid-cols-2 lg:grid-cols-3">
                          <div className="flex items-center justify-between gap-3">
                            <span className="text-[#8f8f99]">NVIDIA GPU</span>
                            <span className="font-medium text-[#f5f5f5]">{runtimeHasNvidiaGpu ? "已检测" : "未检测"}</span>
                          </div>
                          <div className="flex items-center justify-between gap-3">
                            <span className="text-[#8f8f99]">Torch 版本</span>
                            <span className="font-medium text-[#f5f5f5]">{runtimeTorchVersion ?? "未安装"}</span>
                          </div>
                          <div className="flex items-center justify-between gap-3">
                            <span className="text-[#8f8f99]">Torch CUDA</span>
                            <span className="font-medium text-[#f5f5f5]">{runtimeTorchCudaAvailable ? "可用" : "不可用"}</span>
                          </div>
                          <div className="flex items-center justify-between gap-3">
                            <span className="text-[#8f8f99]">CUDA 版本</span>
                            <span className="font-medium text-[#f5f5f5]">{runtimeTorchCudaVersion ?? "无"}</span>
                          </div>
                          <div className="flex items-center justify-between gap-3">
                            <span className="text-[#8f8f99]">运行设备</span>
                            <span className="font-medium text-[#f5f5f5]">{runtimeSelectedDevice}</span>
                          </div>
                          <div className="flex items-center justify-between gap-3">
                            <span className="text-[#8f8f99]">GPU 设备名</span>
                            <span className="font-medium text-[#f5f5f5]">{runtimeTorchCudaDeviceName ?? "无"}</span>
                          </div>
                        </div>
                        <div className="mt-4 flex items-center justify-between gap-4">
                          <div className="text-[12px] leading-5 text-[#9ca3af]">
                            最小壳模式下将按需安装 Python 运行时与模型（支持后续镜像源扩展）。
                          </div>
                          <button
                            type="button"
                            className="inline-flex min-w-[128px] items-center justify-center rounded-full bg-[#6366f1] px-4 py-2 text-xs font-semibold text-white transition-colors hover:bg-[#5558e3] disabled:cursor-not-allowed disabled:opacity-60"
                            onClick={() => void handleBootstrapInstall()}
                            disabled={bootstrapInstalling}
                          >
                            {bootstrapInstalling ? "安装中..." : "一键安装运行环境"}
                          </button>
                        </div>
                        {bootstrapMessage && (
                          <div className="mt-2 text-[12px] text-[#a5b4fc]">{bootstrapMessage}</div>
                        )}
                      </div>

                      <div data-debug-id="dependency-list" className="dependency-list flex w-full flex-col gap-[10px]">
                        {(runtimeHealth?.checks ?? []).map((check) => (
                          <div
                            key={check.name}
                            data-debug-id="dependency-card"
                            className="dependency-card relative flex h-[60px] min-h-[60px] max-h-[64px] items-center gap-[14px] rounded-[16px] border border-white/[0.08] bg-white/[0.035] px-[22px] transition-colors hover:bg-white/[0.055]"
                            style={{ height: 60, minHeight: 60, maxHeight: 64, paddingLeft: 22, paddingRight: 22, gap: 14 }}
                          >
                            <span
                              data-debug-id="status-dot"
                              className={`status-dot static h-[8px] w-[8px] shrink-0 flex-[0_0_8px] rounded-full ${
                                check.ok
                                  ? "bg-emerald-300"
                                  : check.severity === "warning"
                                    ? "bg-amber-300"
                                    : check.severity === "info"
                                      ? "bg-sky-300"
                                      : "bg-rose-300"
                              }`}
                              style={{ position: "static", width: 8, height: 8, marginLeft: 0, marginRight: 0, transform: "none", flex: "0 0 8px" }}
                            />
                            <div className="dependency-text flex min-w-0 flex-[0_1_auto] flex-col gap-[3px]">
                              <div className="dependency-title min-w-0 text-[15px] font-semibold leading-[1.25] tracking-tight text-[#f5f5f5]">
                                {check.name}
                              </div>
                              {check.detail && (
                                <div className="dependency-description min-w-0 text-[12px] leading-[1.3] text-[#8f8f99]">{check.detail}</div>
                              )}
                            </div>
                            <div className="dependency-spacer min-w-[16px] flex-[1_1_auto]" />
                            <div
                              data-debug-id="status-badge"
                              className={`status-badge static inline-flex h-[24px] w-auto flex-[0_0_auto] items-center justify-center rounded-full px-[9px] text-[12px] font-semibold ${
                                check.ok
                                  ? "border border-emerald-400/20 bg-emerald-400/10 text-emerald-200"
                                  : check.severity === "warning"
                                    ? "border border-amber-400/20 bg-amber-400/10 text-amber-200"
                                    : check.severity === "info"
                                      ? "border border-sky-400/20 bg-sky-400/10 text-sky-200"
                                      : "border border-rose-400/20 bg-rose-400/10 text-rose-200"
                              }`}
                              style={{ position: "static", width: "auto", height: 24, paddingLeft: 9, paddingRight: 9, marginLeft: 0, marginRight: 0, transform: "none", flex: "0 0 auto", alignSelf: "auto" }}
                            >
                              {check.ok ? "正常" : check.severity === "warning" ? "注意" : check.severity === "info" ? "信息" : "异常"}
                            </div>
                          </div>
                        ))}
                        {(!runtimeHealth?.checks || runtimeHealth.checks.length === 0) && (
                          <div className="rounded-2xl border border-white/[0.08] bg-white/[0.03] px-4 py-3 text-sm text-[#a1a1aa]">
                            暂无检测结果。
                          </div>
                        )}
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
        <div className="fixed inset-0 z-50 flex items-center justify-center p-6">
          <div
            className="absolute inset-0 bg-black/55 backdrop-blur-[2px]"
            onClick={closeLyricsCandidateModal}
          />
          <div className="relative w-full max-w-3xl rounded-3xl border border-white/[0.08] bg-[#171717] shadow-2xl shadow-black/50 p-7">
            <div className="flex items-start justify-between gap-4">
              <div>
                <div className="text-base font-semibold text-[#fafafa]">选择歌词候选</div>
                <div className="mt-2 text-sm text-[#8a8a94]">{lyricsCandidateSong.name}</div>
              </div>
                <button
                  type="button"
                  className="inline-flex min-w-[92px] items-center justify-center whitespace-nowrap rounded-full px-6 py-2.5 text-sm font-medium text-[#d4d4d8] hover:bg-white/[0.06]"
                  onClick={closeLyricsCandidateModal}
                >
                  关闭
                </button>
            </div>

            <div className="mt-4 flex items-center gap-3">
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
                className="h-11 flex-1 rounded-2xl border border-white/[0.10] bg-white/[0.04] px-4 text-sm text-[#fafafa] outline-none transition-colors placeholder:text-[#52525b] focus:border-[#818cf8]"
              />
              <button
                type="button"
                className="inline-flex h-11 min-w-[96px] items-center justify-center whitespace-nowrap rounded-full bg-[#6366f1] px-5 text-sm font-semibold text-white transition-colors hover:bg-[#5558e3] disabled:cursor-not-allowed disabled:opacity-60"
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

            {lyricsCandidateError && (
              <div className="mt-4 rounded-2xl border border-white/[0.08] bg-white/[0.03] px-4 py-3 text-sm text-[#fca5a5]">
                {lyricsCandidateError}
              </div>
            )}

            {lyricsCandidateLoading && !lyricsCandidates && !lyricsCandidateError && (
              <div className="mt-4 rounded-2xl border border-white/[0.08] bg-white/[0.03] px-4 py-3 text-sm text-[#a1a1aa]">
                搜索中...
              </div>
            )}

            {lyricsCandidates && lyricsCandidates.length > 0 && (
              <div className="mt-4 max-h-[60vh] overflow-y-auto pr-1 flex flex-col gap-3">
                {lyricsCandidates.map((candidate) => (
                  <button
                    key={candidate.id}
                    type="button"
                    onClick={() => void handleApplyLyricsCandidate(candidate)}
                    className="w-full rounded-2xl border border-white/[0.08] bg-white/[0.03] px-4 py-4 text-left transition-colors hover:bg-white/[0.06]"
                  >
                    <div className="flex items-center justify-between gap-3">
                      <div className="min-w-0">
                        <div className="truncate text-sm font-semibold text-[#fafafa]">{candidate.title}</div>
                        <div className="mt-1 text-xs text-[#a1a1aa]">
                          {candidate.artist || "未知歌手"}
                          {candidate.album ? ` · ${candidate.album}` : ""}
                        </div>
                      </div>
                      <div className="flex shrink-0 items-center gap-2">
                        <span className="rounded-full bg-[#1e1e1e] px-3 py-1 text-xs text-[#d4d4d8]">
                          {candidate.sourceLabel}
                        </span>
                        <span className="rounded-full bg-[#6366f1] px-3 py-1 text-xs font-semibold text-white">
                          {candidate.score}
                        </span>
                      </div>
                    </div>
                    <div className="mt-3 whitespace-pre-line text-sm leading-6 text-[#d4d4d8]">
                      {candidate.preview || "（无预览）"}
                    </div>
                    <div className="mt-3 text-xs text-[#71717a]">
                      点击即可采用此候选
                    </div>
                  </button>
                ))}
              </div>
            )}

            {lyricsCandidates && lyricsCandidates.length === 0 && !lyricsCandidateLoading && !lyricsCandidateError && (
              <div className="mt-4 rounded-2xl border border-white/[0.08] bg-white/[0.03] px-4 py-3 text-sm text-[#a1a1aa]">
                没有找到可用的歌词候选，可以换个关键词继续搜索。
              </div>
            )}

            <div className="mt-6 flex items-center justify-end">
              <button
                type="button"
                className="inline-flex min-w-[92px] items-center justify-center whitespace-nowrap rounded-full px-6 py-2.5 text-sm font-medium text-[#d4d4d8] hover:bg-white/[0.06]"
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
