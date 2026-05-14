import { useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";

interface ImportViewProps {
  onFilesSelected: (paths: string[]) => void;
}

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

export default function ImportView({ onFilesSelected }: ImportViewProps) {
  const handleClick = useCallback(async () => {
    try {
      const selected = await open({
        multiple: true,
        filters: [{
          name: "Audio / Video",
          extensions: MEDIA_IMPORT_EXTENSIONS,
        }]
      });
      if (selected) {
        const paths = Array.isArray(selected) ? selected : [selected];
        if (paths.length > 0) {
          onFilesSelected(paths);
        }
      }
    } catch (e) {
      console.error("Failed to select files:", e);
    }
  }, [onFilesSelected]);

  return (
    <div
      onClick={handleClick}
      className="group relative flex flex-col items-center justify-center w-full max-w-md aspect-[2/1] rounded-2xl border-2 border-dashed border-[#2e2e2e] bg-[#141414] hover:border-[#6366f1] hover:bg-[#1a1a2e] transition-all duration-300 cursor-pointer overflow-hidden mx-4"
    >
      {/* 背景装饰 */}
      <div className="absolute inset-0 opacity-0 group-hover:opacity-100 transition-opacity">
        <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-64 h-64 bg-[#6366f1]/10 rounded-full blur-3xl" />
      </div>

      {/* 图标 */}
      <div className="relative mb-6">
        <div className="w-20 h-20 rounded-2xl bg-gradient-to-br from-[#6366f1]/20 to-[#a855f7]/20 flex items-center justify-center group-hover:scale-110 transition-transform duration-300">
          <svg className="w-10 h-10 text-[#6366f1] group-hover:text-[#818cf8] transition-colors" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="1.5" d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3" />
          </svg>
        </div>
      </div>

      {/* 文字 */}
      <div className="relative text-center">
        <p className="text-[#fafafa] font-medium mb-1">导入音频 / 视频文件</p>
        <p className="text-[#71717a] text-sm">点击或拖拽文件到这里，视频会自动抽取音轨</p>
      </div>

      {/* 支持格式 */}
      <div className="relative mt-6 flex gap-2">
        {["MP3", "WAV", "FLAC", "M4A", "MP4", "MOV"].map((fmt) => (
          <span key={fmt} className="px-2 py-0.5 text-xs rounded-full bg-[#1e1e1e] text-[#71717a]">
            {fmt}
          </span>
        ))}
      </div>
    </div>
  );
}
