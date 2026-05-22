import { useEffect, useRef, useState } from "react";

interface VocalWaveformPreviewProps {
  peaks: number[] | null;
  currentTime: number;
  duration: number;
  isPlaying: boolean;
  loading?: boolean;
  error?: string | null;
}

const BAR_COUNT = 6000;

const formatTime = (ms: number) => {
  const totalSeconds = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = (totalSeconds % 60).toString().padStart(2, "0");
  return `${minutes}:${seconds}`;
};

export function buildWaveformPeaks(buffer: AudioBuffer, peakCount = BAR_COUNT) {
  const peaks = new Array<number>(peakCount).fill(0);
  const channels = Array.from({ length: buffer.numberOfChannels }, (_, index) =>
    buffer.getChannelData(index)
  );
  const segmentLength = Math.max(1, Math.floor(buffer.length / peakCount));

  for (let i = 0; i < peakCount; i += 1) {
    const start = i * segmentLength;
    const end = Math.min(buffer.length, start + segmentLength);
    let max = 0;
    const stride = Math.max(1, Math.floor((end - start) / 256));

    for (let sample = start; sample < end; sample += stride) {
      for (const channel of channels) {
        const amplitude = Math.abs(channel[sample] ?? 0);
        if (amplitude > max) {
          max = amplitude;
        }
      }
    }

    peaks[i] = Math.pow(Math.max(0, Math.min(1, max)), 0.52);
  }

  return peaks;
}

export default function VocalWaveformPreview({
  peaks,
  currentTime,
  duration,
  isPlaying,
  loading = false,
  error = null,
}: VocalWaveformPreviewProps) {
  const viewportRef = useRef<HTMLDivElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const canvasLayerRef = useRef<HTMLDivElement | null>(null);
  const playheadRef = useRef<HTMLDivElement | null>(null);
  const [viewportWidth, setViewportWidth] = useState(0);
  const [footerTime, setFooterTime] = useState(currentTime);
  const animationRef = useRef<number | null>(null);
  const footerUpdateRef = useRef(0);
  const playbackTimeRef = useRef(currentTime);
  const playbackCursorRef = useRef({
    targetTime: currentTime,
    startedAt: performance.now(),
    startedTime: currentTime,
  });

  useEffect(() => {
    const el = viewportRef.current;
    if (!el) return;

    const update = () => setViewportWidth(el.getBoundingClientRect().width);
    update();

    if (typeof ResizeObserver === "undefined") {
      window.addEventListener("resize", update);
      return () => window.removeEventListener("resize", update);
    }

    const observer = new ResizeObserver(() => update());
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    playbackCursorRef.current.targetTime = currentTime;
    playbackCursorRef.current.startedTime = currentTime;
    playbackCursorRef.current.startedAt = performance.now();
    playbackTimeRef.current = currentTime;
    setFooterTime(currentTime);
  }, [currentTime, isPlaying, peaks]);

  const barWidth = 1.25;
  const gap = 0.55;
  const step = barWidth + gap;
  const totalPeaks = peaks?.length ?? 0;
  const waveformHeight = 120;
  const centerY = waveformHeight / 2;
  const waveformWidth = Math.max(step * Math.max(totalPeaks, 1), viewportWidth);
  const visible = totalPeaks > 0;

  const getPlaybackLayout = (time: number) => {
    const ratio = duration > 0 ? Math.max(0, Math.min(1, time / duration)) : 0;
    const playheadX = waveformWidth * ratio;
    const maxScroll = Math.max(0, waveformWidth - viewportWidth);
    const offset = waveformWidth <= viewportWidth
      ? (viewportWidth - waveformWidth) / 2
      : Math.min(0, Math.max(-maxScroll, (viewportWidth / 2) - playheadX));
    const playheadViewportX = Math.max(0, Math.min(viewportWidth, playheadX + offset));
    return { offset, playheadViewportX };
  };

  const applyCanvasOffset = (time: number) => {
    const layer = canvasLayerRef.current;
    const playhead = playheadRef.current;
    if (!layer || duration <= 0) return;
    const { offset, playheadViewportX } = getPlaybackLayout(time);
    layer.style.transform = `translateX(${offset}px)`;
    if (playhead) {
      playhead.style.left = `${playheadViewportX}px`;
    }
  };

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !peaks || peaks.length === 0 || viewportWidth <= 0) return;

    const dpr = Math.max(1, window.devicePixelRatio || 1);
    canvas.width = Math.max(1, Math.ceil(waveformWidth * dpr));
    canvas.height = Math.max(1, Math.ceil(waveformHeight * dpr));
    canvas.style.width = `${waveformWidth}px`;
    canvas.style.height = `${waveformHeight}px`;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, waveformWidth, waveformHeight);

    const styles = getComputedStyle(document.documentElement);
    const waveformColor = styles.getPropertyValue("--waveform-original").trim() || "#60a5fa";
    const gradient = ctx.createLinearGradient(0, 0, 0, waveformHeight);
    gradient.addColorStop(0, waveformColor);
    gradient.addColorStop(1, waveformColor);
    ctx.fillStyle = gradient;
    ctx.globalAlpha = 0.78;

    for (let index = 0; index < peaks.length; index += 1) {
      const peak = peaks[index];
      const height = Math.max(1.6, peak * 44);
      const x = index * step;
      const y = centerY - height;
      ctx.fillRect(x, y, barWidth, height * 2);
    }
    ctx.globalAlpha = 1;
    applyCanvasOffset(playbackTimeRef.current);
  }, [barWidth, centerY, peaks, step, viewportWidth, waveformHeight, waveformWidth]);

  useEffect(() => {
    const observer = new MutationObserver(() => {
      const canvas = canvasRef.current;
      if (!canvas) return;
      const ctx = canvas.getContext("2d");
      if (!ctx || !peaks || peaks.length === 0) return;
      const dpr = Math.max(1, window.devicePixelRatio || 1);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, waveformWidth, waveformHeight);
      const styles = getComputedStyle(document.documentElement);
      ctx.fillStyle = styles.getPropertyValue("--waveform-original").trim() || "#60a5fa";
      ctx.globalAlpha = 0.78;
      for (let index = 0; index < peaks.length; index += 1) {
        const peak = peaks[index];
        const height = Math.max(1.6, peak * 44);
        ctx.fillRect(index * step, centerY - height, barWidth, height * 2);
      }
      ctx.globalAlpha = 1;
    });
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    return () => observer.disconnect();
  }, [barWidth, centerY, peaks, step, waveformHeight, waveformWidth]);

  useEffect(() => {
    applyCanvasOffset(currentTime);
  }, [currentTime, duration, viewportWidth, waveformWidth]);

  useEffect(() => {
    if (!isPlaying || !peaks || peaks.length === 0 || duration <= 0) {
      if (animationRef.current !== null) {
        cancelAnimationFrame(animationRef.current);
        animationRef.current = null;
      }
      return;
    }

    const tick = () => {
      const now = performance.now();
      const elapsed = now - playbackCursorRef.current.startedAt;
      const nextTime = Math.min(duration, playbackCursorRef.current.startedTime + elapsed);
      playbackTimeRef.current = nextTime;
      applyCanvasOffset(nextTime);
      if (now - footerUpdateRef.current > 250) {
        footerUpdateRef.current = now;
        setFooterTime(nextTime);
      }
      animationRef.current = requestAnimationFrame(tick);
    };

    animationRef.current = requestAnimationFrame(tick);
    return () => {
      if (animationRef.current !== null) {
        cancelAnimationFrame(animationRef.current);
        animationRef.current = null;
      }
    };
  }, [duration, isPlaying, peaks, viewportWidth, waveformWidth]);

  const playbackLabel = duration > 0
    ? `${formatTime(footerTime)} / ${formatTime(duration)}`
    : "0:00 / 0:00";

  return (
    <div className="vocal-waveform-panel">
      <div className="mb-[10px] flex items-center justify-between gap-3">
        <div className="min-w-0">
          <div className="vocal-waveform-title text-[14px] font-semibold leading-[1.25] tracking-tight">原唱波形</div>
          <div className="vocal-waveform-subtitle mt-[3px] text-[12px] leading-[1.35]">
            波形会随播放移动，保留更高解析度以提示换句时机
          </div>
        </div>
      </div>

      <div className="vocal-waveform-viewport-shell relative">
        <div
          ref={viewportRef}
          className="relative h-[120px] overflow-hidden select-none"
          onPointerDown={(event) => event.preventDefault()}
          onDragStart={(event) => event.preventDefault()}
        >
          <div className="vocal-waveform-centerline pointer-events-none absolute inset-x-0 top-1/2 z-10 h-px -translate-y-1/2" />
          <div
            ref={playheadRef}
            className="vocal-waveform-playhead pointer-events-none absolute top-2 z-20 h-[calc(100%-1rem)] w-[2px] -translate-x-1/2"
          />

          {!visible && !loading && !error && (
            <div className="vocal-waveform-empty flex h-full items-center justify-center px-4 text-[12px]">
              暂无原唱波形
            </div>
          )}

          {loading && (
            <div className="vocal-waveform-loading flex h-full items-center justify-center px-4 text-[12px]">
              原唱波形生成中...
            </div>
          )}

          {error && !loading && (
            <div className="vocal-waveform-error flex h-full items-center justify-center px-4 text-[12px]">
              {error}
            </div>
          )}

          {visible && !loading && !error && (
            <div
              ref={canvasLayerRef}
              className="absolute top-0 h-full will-change-transform"
              style={{
                left: 0,
                width: waveformWidth,
              }}
              aria-hidden="true"
            >
              <canvas
                ref={canvasRef}
                className="block h-full"
                draggable={false}
              />
            </div>
          )}
        </div>

        <div className="vocal-waveform-footer flex items-center justify-between gap-4 px-4 py-2 text-[11px]">
          <span>原唱提示 · 跟随播放定位</span>
          <span>{playbackLabel}</span>
        </div>
      </div>
    </div>
  );
}
