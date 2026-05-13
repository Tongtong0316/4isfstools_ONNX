export type LyricTokenKind = "char" | "word" | "punctuation" | "breath" | "unknown";

export interface LyricToken {
  id: string;
  lineId: string;
  index: number;
  text: string;
  startMs: number;
  endMs: number;
  confidence: number;
  kind: LyricTokenKind;
}

export interface LyricLineDoc {
  id: string;
  index: number;
  startMs: number;
  endMs: number;
  text: string;
  confidence: number;
  edited: boolean;
  locked: boolean;
  tokens: LyricToken[];
}

export interface LyricDocument {
  songId: string;
  version: number;
  language: string | null;
  source: string;
  alignmentEngine: string;
  createdAt: number;
  updatedAt: number;
  globalOffsetMs: number;
  dirty: boolean;
  qualityScore: number;
  lines: LyricLineDoc[];
}
