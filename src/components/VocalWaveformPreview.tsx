import { useEffect, useMemo, useRef, useState } from "react";

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
  const [viewportWidth, setViewportWidth] = useState(0);
  const [displayTime, setDisplayTime] = useState(currentTime);
  const animationRef = useRef<number | null>(null);
  const playheadRef = useRef({
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
    playheadRef.current.targetTime = currentTime;
    playheadRef.current.startedTime = currentTime;
    playheadRef.current.startedAt = performance.now();
    setDisplayTime(currentTime);
  }, [currentTime, isPlaying, peaks]);

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
      const elapsed = now - playheadRef.current.startedAt;
      const nextTime = Math.min(duration, playheadRef.current.startedTime + elapsed);
      setDisplayTime(nextTime);
      animationRef.current = requestAnimationFrame(tick);
    };

    animationRef.current = requestAnimationFrame(tick);
    return () => {
      if (animationRef.current !== null) {
        cancelAnimationFrame(animationRef.current);
        animationRef.current = null;
      }
    };
  }, [duration, isPlaying, peaks]);

  const barWidth = 1.25;
  const gap = 0.55;
  const step = barWidth + gap;
  const totalPeaks = peaks?.length ?? 0;
  const currentIndex = useMemo(() => {
    if (!peaks || peaks.length === 0 || duration <= 0) return 0;
    const ratio = Math.max(0, Math.min(1, displayTime / duration));
    return Math.max(0, Math.min(peaks.length - 1, Math.floor(ratio * (peaks.length - 1))));
  }, [displayTime, duration, peaks]);

  const waveformHeight = 120;
  const centerY = waveformHeight / 2;
  const waveformWidth = Math.max(step * Math.max(totalPeaks, 1), viewportWidth);
  const visible = totalPeaks > 0;
  const displayRatio = duration > 0 ? Math.max(0, Math.min(1, displayTime / duration)) : 0;
  const playheadX = waveformWidth * displayRatio;
  const rawOffset = viewportWidth > 0 ? (viewportWidth / 2) - playheadX : 0;
  const centerOffset = waveformWidth <= viewportWidth
    ? (viewportWidth - waveformWidth) / 2
    : Math.min(0, Math.max(viewportWidth - waveformWidth, rawOffset));

  const playbackLabel = duration > 0
    ? `${formatTime(displayTime)} / ${formatTime(duration)}`
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
          className="relative h-[120px] overflow-hidden"
        >
          <div className="vocal-waveform-centerline pointer-events-none absolute inset-x-0 top-1/2 z-10 h-px -translate-y-1/2" />
          <div className="vocal-waveform-playhead pointer-events-none absolute left-1/2 top-2 z-20 h-[calc(100%-1rem)] w-[2px] -translate-x-1/2" />

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
            <svg
              className="absolute top-0 h-full"
              width={waveformWidth}
              height={waveformHeight}
              style={{
                left: 0,
                transform: `translateX(${centerOffset}px)`,
              }}
              viewBox={`0 0 ${waveformWidth} ${waveformHeight}`}
              preserveAspectRatio="none"
              aria-hidden="true"
            >
              <defs>
                <linearGradient id="vocal-waveform-fill" x1="0%" y1="0%" x2="0%" y2="100%">
                  <stop offset="0%" stopColor="var(--waveform-original)" stopOpacity="0.58" />
                  <stop offset="100%" stopColor="var(--waveform-original)" stopOpacity="0.26" />
                </linearGradient>
              </defs>

              {peaks?.map((peak, index) => {
                const height = Math.max(1.6, peak * 44);
                const x = index * step;
                const y = centerY - height;
                const highlight = index === currentIndex;
                return (
                  <rect
                    key={`${index}-${peak.toFixed(3)}`}
                    x={x}
                    y={y}
                    width={barWidth}
                    height={height * 2}
                  rx={0}
                  fill="url(#vocal-waveform-fill)"
                  opacity={highlight ? 1 : 0.92}
                  />
                );
              })}

              {currentIndex >= 0 && currentIndex < peaks!.length && (
                <rect
                  x={currentIndex * step}
                  y={Math.max(2, centerY - Math.max(1.6, peaks![currentIndex] * 46))}
                  width={barWidth}
                  height={Math.max(3.2, Math.max(1.6, peaks![currentIndex] * 46) * 2)}
                  fill="var(--waveform-playhead)"
                />
              )}
            </svg>
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
