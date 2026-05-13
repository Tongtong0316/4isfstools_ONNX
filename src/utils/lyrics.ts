import type { LyricDocument, LyricLineDoc, LyricToken } from "../types/lyrics";
import type { LyricLine } from "../types";

export function documentToPlaybackLines(document: LyricDocument | null): LyricLine[] {
  if (!document) return [];
  return document.lines
    .filter((line) => line.text.trim().length > 0)
    .map((line) => ({
      time: Math.max(0, line.startMs + document.globalOffsetMs),
      text: line.text,
    }))
    .sort((a, b) => a.time - b.time);
}

export function findActiveLyricIndex(lines: LyricLine[], currentTime: number) {
  if (lines.length === 0) return -1;
  let low = 0;
  let high = lines.length - 1;
  let answer = -1;
  while (low <= high) {
    const mid = Math.floor((low + high) / 2);
    if (lines[mid].time <= currentTime) {
      answer = mid;
      low = mid + 1;
    } else {
      high = mid - 1;
    }
  }
  return answer;
}

export function normalizeLyricLine(line: LyricLineDoc): LyricLineDoc {
  const startMs = Math.max(0, Math.floor(line.startMs));
  const endMs = Math.max(startMs + 300, Math.floor(line.endMs));
  return {
    ...line,
    startMs,
    endMs,
    text: line.text.trim(),
    tokens: line.tokens.map((token) => normalizeLyricToken(token, startMs, endMs)),
  };
}

function normalizeLyricToken(token: LyricToken, lineStartMs: number, lineEndMs: number): LyricToken {
  const startMs = Math.max(lineStartMs, Math.floor(token.startMs));
  const endMs = Math.min(lineEndMs, Math.max(startMs + 50, Math.floor(token.endMs)));
  return { ...token, startMs, endMs };
}

export function shiftLine(line: LyricLineDoc, deltaMs: number): LyricLineDoc {
  return normalizeLyricLine({
    ...line,
    startMs: line.startMs + deltaMs,
    endMs: line.endMs + deltaMs,
    edited: true,
    tokens: line.tokens.map((token) => ({
      ...token,
      startMs: token.startMs + deltaMs,
      endMs: token.endMs + deltaMs,
    })),
  });
}
