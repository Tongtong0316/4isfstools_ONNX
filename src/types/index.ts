export interface Song {
  id: string;
  name: string;
  originalPath: string;
  playlistFolder?: string | null;
  vocalsPath: string | null;
  instrumentalPath: string | null;
  originalMixPath: string | null;
  lyricsPath: string | null;
  duration: number;
  status: 'pending' | 'queued' | 'processing' | 'ready' | 'error' | 'cancelled' | 'cancelling';
  progress: number;
  processingStage?: ProcessingStage;
  error_message?: string;
  addedAt: number;
}

export type ProcessingStage =
  | 'checking_gpu'
  | 'gpu_available'
  | 'cpu_fallback'
  | 'separating'
  | 'aligning'
  | 'complete'
  | 'queued'
  | 'cancelling'
  | 'cancelled'
  | 'error';

export interface ProcessingStatus {
  song_id: string;
  stage: ProcessingStage;
  progress: number;
  message?: string;
  estimated_time?: number;
  error?: string;
}

export interface ModelStatus {
  status: 'idle' | 'checking_demucs' | 'downloading_demucs' | 'checking_whisper' | 'downloading_whisper' | 'complete' | 'error';
  progress: number;
  message?: string;
}

export interface LyricLine {
  time: number;
  text: string;
}

export type PlayerState = 'idle' | 'playing' | 'paused';

export const STAGE_LABELS: Record<ProcessingStage, string> = {
  'checking_gpu': '检测 GPU...',
  'gpu_available': 'GPU 可用',
  'cpu_fallback': 'CPU 处理中',
  'separating': '人声分离中',
  'aligning': '歌词同步中',
  'complete': '处理完成',
  'queued': '排队中',
  'cancelling': '正在取消',
  'cancelled': '已取消',
  'error': '处理失败'
};

export const STATUS_LABELS: Record<string, string> = {
  'pending': '待处理',
  'queued': '排队中',
  'processing': '处理中',
  'ready': '可唱',
  'error': '失败',
  'cancelled': '已取消',
  'cancelling': '取消中'
};

export const STATUS_ICONS: Record<string, string> = {
  'pending': '📁',
  'queued': '⏳',
  'processing': '⚙️',
  'ready': '🎤',
  'error': '❌',
  'cancelled': '⏸',
  'cancelling': '🔄'
};

export const STAGE_ICONS: Record<ProcessingStage, string> = {
  'checking_gpu': '🔍',
  'gpu_available': '🚀',
  'cpu_fallback': '💻',
  'separating': '🎤',
  'aligning': '🎯',
  'complete': '✅',
  'queued': '⏳',
  'cancelling': '🔄',
  'cancelled': '⏸',
  'error': '❌'
};
