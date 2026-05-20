import { Song, LyricLine, PlayerState } from "../types";
import { MusicNoteIcon } from "./icons";

interface PlayerProps {
  song: Song | null;
  playerState: PlayerState;
  currentTime: number;
  volume: number;
  showOriginal: boolean;
  lyrics: LyricLine[];
  currentLyricIndex: number;
  playbackError: string | null;
  onPlayPause: () => void;
  onSeek: (time: number) => void;
  onVolumeChange: (volume: number) => void;
  onToggleOriginal: () => void;
  onPrev: () => void;
  onNext: () => void;
}

export default function Player({
  song, playerState, currentTime, volume, showOriginal,
  lyrics, currentLyricIndex, playbackError, onPlayPause, onSeek, onVolumeChange,
  onToggleOriginal, onPrev, onNext,
}: PlayerProps) {
  const formatTime = (ms: number) => {
    const s = Math.floor(ms / 1000);
    return `${Math.floor(s / 60)}:${(s % 60).toString().padStart(2, "0")}`;
  };

  const progress = song && song.duration > 0 ? (currentTime / song.duration) * 100 : 0;

  return (
    <div className="h-[240px] rounded-xl border border-[#2a2a4a] bg-gradient-to-b from-[#1a1a2e] via-[#16213e] to-[#0f0f23] flex gap-6 overflow-hidden" style={{ paddingLeft: '32px', paddingRight: '32px', paddingTop: '24px', paddingBottom: '24px' }}>
      {/* 左侧：专辑封面 + 基本信息 */}
      <div className="w-[300px] h-full flex items-center gap-5">
        <div className="relative shrink-0">
          <div className="w-24 h-24 rounded-xl bg-gradient-to-br from-[#6366f1] to-[#a855f7] shadow-lg shadow-purple-500/20 flex items-center justify-center">
            <div className="w-20 h-20 rounded-lg bg-[#0f0f23] flex items-center justify-center">
              {song ? (
                <MusicNoteIcon className="w-9 h-9 text-white" />
              ) : (
                <MusicNoteIcon className="w-7 h-7 text-[#6366f1]" />
              )}
            </div>
          </div>
          {playerState === "playing" && (
            <div className="absolute -bottom-1 -right-1 w-6 h-6 rounded-full bg-[#22c55e] flex items-center justify-center animate-pulse">
              <div className="w-2 h-2 bg-white rounded-full" />
            </div>
          )}
        </div>
        <div className="flex-1 min-w-0 pt-1">
          <div className="font-medium text-sm truncate">{song?.name || "未选择歌曲"}</div>
          <div className="text-xs text-[#71717a] mt-0.5">
            {showOriginal ? "原声" : "伴奏"}
          </div>
          {playbackError && (
            <div className="text-xs text-[#ef4444] mt-1 truncate">{playbackError}</div>
          )}
        </div>
      </div>

      {/* 中间：控制栏 + 歌词 */}
      <div className="min-w-0 flex-1 flex flex-col">
        {/* 控制栏 */}
        <div className="h-14 shrink-0 flex items-center gap-3">
          <button onClick={onPrev} className="p-2 hover:bg-white/10 rounded-full transition-colors text-white/70 hover:text-white">
            <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
              <path d="M6 6h2v12H6V6zm3.5 6l8.5 6V6l-8.5 6z"/>
            </svg>
          </button>
          <button
            onClick={onPlayPause}
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
          <button onClick={onNext} className="p-2 hover:bg-white/10 rounded-full transition-colors text-white/70 hover:text-white">
            <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
              <path d="M6 18l8.5-6L6 6v12zm2-8.14L11.03 12 8 14.14V9.86zM16 6h2v12h-2V6z"/>
            </svg>
          </button>

          {/* 进度条 */}
          <div className="flex-1 mx-4 flex items-center gap-2">
            <span className="text-xs text-[#71717a] w-10 text-right font-mono">{formatTime(currentTime)}</span>
            <div className="relative flex-1 h-1 bg-[#2a2a4a] rounded-full group">
              <div
                className="absolute h-full bg-gradient-to-r from-[#6366f1] to-[#a855f7] rounded-full"
                style={{ width: `${progress}%` }}
              />
              <input
                type="range"
                min="0"
                max="100"
                value={progress}
                onChange={(e) => song && onSeek((parseFloat(e.target.value) / 100) * song.duration)}
                className="absolute inset-0 w-full opacity-0 cursor-pointer"
              />
            </div>
            <span className="text-xs text-[#71717a] w-10 font-mono">{song ? formatTime(song.duration) : "00:00"}</span>
          </div>

          {/* 原唱/伴奏切换 */}
          <button
            onClick={onToggleOriginal}
            className={`px-3 py-1.5 rounded-full text-xs font-medium transition-all ${
              showOriginal
                ? "bg-[#6366f1] text-white shadow-lg shadow-purple-500/30"
                : "bg-[#1e1e1e] text-[#a1a1aa] hover:bg-[#2a2a4a]"
            }`}
          >
            {showOriginal ? "原唱" : "伴奏"}
          </button>

          {/* 音量 */}
          <div className="flex items-center gap-2">
            <button onClick={() => onVolumeChange(volume > 0 ? 0 : 80)} className="p-1.5 hover:bg-white/10 rounded-full transition-colors text-white/70 hover:text-white">
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
            <div className="w-20 h-1 bg-[#2a2a4a] rounded-full">
              <div className="h-full bg-[#6366f1] rounded-full transition-all" style={{ width: `${volume}%` }} />
            </div>
          </div>
        </div>

        {/* 歌词区域 */}
        <div className="min-h-0 flex-1 flex items-center justify-center overflow-hidden">
          {lyrics.length > 0 ? (
            <div className="flex flex-col items-center gap-1">
              {lyrics.map((line, i) => {
                const isActive = i === currentLyricIndex;
                const isNear = Math.abs(i - currentLyricIndex) <= 2;
                return (
                  <div
                    key={i}
                    className={`text-center transition-all duration-300 ${
                      isActive
                        ? "text-white text-lg font-medium scale-105"
                        : isNear
                        ? "text-[#71717a] text-sm"
                        : "text-[#3f3f46] text-xs"
                    }`}
                    style={{ opacity: isNear ? 1 : 0.3 }}
                  >
                    {line.text || "· · ·"}
                  </div>
                );
              })}
            </div>
          ) : (
            <div className="text-[#3f3f46] text-sm">
              {song ? "歌词加载中..." : "选择歌曲开始播放"}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}