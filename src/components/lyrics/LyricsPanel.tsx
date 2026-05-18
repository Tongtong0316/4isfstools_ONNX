import { useEffect, useMemo, useRef, useState } from "react";
import type { LyricDocument, LyricLineDoc } from "../../types/lyrics";
import { findActiveLyricIndex, shiftLine } from "../../utils/lyrics";

interface LyricsPanelProps {
  document: LyricDocument | null;
  currentTime: number;
  isPlaying: boolean;
  onSeek: (timeMs: number) => void;
  onSaveDocument: (document: LyricDocument) => void;
}

export default function LyricsPanel({ document, currentTime, isPlaying, onSeek, onSaveDocument }: LyricsPanelProps) {
  const [editingLineId, setEditingLineId] = useState<string | null>(null);
  const [draftText, setDraftText] = useState("");
  const activeIndex = useMemo(() => {
    if (!document) return -1;
    const playbackLines = document.lines.map((line) => ({
      time: Math.max(0, line.startMs + document.globalOffsetMs),
      text: line.text,
    }));
    return findActiveLyricIndex(playbackLines, currentTime);
  }, [document, currentTime]);

  const activeRef = useRef<HTMLDivElement | null>(null);
  const scrollContainerRef = useRef<HTMLDivElement | null>(null);
  const resumeAutoCenterTimerRef = useRef<number | null>(null);
  const lastAutoCenterAtRef = useRef(0);
  const userInteractingRef = useRef(false);
  const isProgrammaticScrollRef = useRef(false);
  const releaseProgrammaticScrollTimerRef = useRef<number | null>(null);
  const [manualScrollHold, setManualScrollHold] = useState(false);
  const [edgeSpacerHeight, setEdgeSpacerHeight] = useState(260);

  const clearProgrammaticScrollTimer = () => {
    if (releaseProgrammaticScrollTimerRef.current !== null) {
      window.clearTimeout(releaseProgrammaticScrollTimerRef.current);
      releaseProgrammaticScrollTimerRef.current = null;
    }
  };

  const centerActiveLine = (behavior: ScrollBehavior) => {
    const container = scrollContainerRef.current;
    const active = activeRef.current;
    if (!container || !active) return;
    const targetTop = active.offsetTop - (container.clientHeight / 2) + (active.clientHeight / 2);
    const maxTop = Math.max(0, container.scrollHeight - container.clientHeight);
    const clampedTop = Math.min(maxTop, Math.max(0, targetTop));
    isProgrammaticScrollRef.current = true;
    container.scrollLeft = 0;
    container.scrollTo({ top: clampedTop, behavior });
    clearProgrammaticScrollTimer();
    releaseProgrammaticScrollTimerRef.current = window.setTimeout(() => {
      isProgrammaticScrollRef.current = false;
    }, 1000);
  };

  const clearResumeAutoCenterTimer = () => {
    if (resumeAutoCenterTimerRef.current !== null) {
      window.clearTimeout(resumeAutoCenterTimerRef.current);
      resumeAutoCenterTimerRef.current = null;
    }
  };

  const scheduleResumeAutoCenter = () => {
    clearResumeAutoCenterTimer();
    if (!isPlaying) return;
    resumeAutoCenterTimerRef.current = window.setTimeout(() => {
      userInteractingRef.current = false;
      setManualScrollHold(false);
      centerActiveLine("smooth");
      lastAutoCenterAtRef.current = Date.now();
    }, 900);
  };

  useEffect(() => {
    if (!editingLineId && isPlaying && !manualScrollHold) {
      centerActiveLine("smooth");
      lastAutoCenterAtRef.current = Date.now();
    }
  }, [activeIndex, editingLineId, isPlaying, manualScrollHold]);

  useEffect(() => {
    if (isPlaying && !manualScrollHold && !editingLineId) {
      centerActiveLine("smooth");
      lastAutoCenterAtRef.current = Date.now();
    }
  }, [isPlaying, manualScrollHold, editingLineId]);

  useEffect(() => {
    return () => {
      clearResumeAutoCenterTimer();
      clearProgrammaticScrollTimer();
    };
  }, []);

  useEffect(() => {
    const container = scrollContainerRef.current;
    if (!container) return;

    const updateSpacer = () => {
      const nextHeight = Math.max(Math.round(container.clientHeight * 0.58), 260);
      setEdgeSpacerHeight(nextHeight);
    };

    updateSpacer();

    if (typeof ResizeObserver === "undefined") {
      window.addEventListener("resize", updateSpacer);
      return () => window.removeEventListener("resize", updateSpacer);
    }

    const observer = new ResizeObserver(() => updateSpacer());
    observer.observe(container);
    window.addEventListener("resize", updateSpacer);

    return () => {
      observer.disconnect();
      window.removeEventListener("resize", updateSpacer);
    };
  }, []);

  if (!document || document.lines.length === 0) {
    return (
      <div className="px-6 py-8 text-center text-base leading-7 text-[var(--text-muted)]">
        暂无歌词
      </div>
    );
  }

  const saveLine = (line: LyricLineDoc, nextText: string) => {
    const nextDocument: LyricDocument = {
      ...document,
      updatedAt: Date.now(),
      dirty: true,
      lines: document.lines.map((item) =>
        item.id === line.id
          ? { ...item, text: nextText.trim(), edited: true }
          : item
      ),
    };
    setEditingLineId(null);
    setDraftText("");
    onSaveDocument(nextDocument);
  };

  const updateLineTime = (line: LyricLineDoc, deltaMs: number) => {
    const nextDocument: LyricDocument = {
      ...document,
      updatedAt: Date.now(),
      dirty: true,
      lines: document.lines.map((item) =>
        item.id === line.id ? shiftLine(item, deltaMs) : item
      ),
    };
    onSaveDocument(nextDocument);
  };

  const getUpcomingDots = (lineIndex: number) => {
    if (!isPlaying || !document) return "";
    const nextIndex = activeIndex < 0 ? 0 : activeIndex + 1;
    if (lineIndex !== nextIndex) return "";

    const nextLine = document.lines[lineIndex];
    if (!nextLine) return "";
    const nextStart = Math.max(0, nextLine.startMs + document.globalOffsetMs);
    const previousStart =
      lineIndex > 0
        ? Math.max(0, document.lines[lineIndex - 1].startMs + document.globalOffsetMs)
        : Math.max(0, nextStart - 3000);

    const total = Math.max(600, nextStart - previousStart);
    const elapsed = Math.max(0, Math.min(total, currentTime - previousStart));
    const remainingRatio = 1 - elapsed / total;

    if (remainingRatio > 0.66) return "...";
    if (remainingRatio > 0.33) return "..";
    if (remainingRatio > 0.08) return ".";
    return "";
  };

  return (
    <div
      ref={scrollContainerRef}
      className="flex h-full w-full flex-col items-stretch gap-2 overflow-y-auto overflow-x-hidden px-6 py-5"
      style={{ maxHeight: "100%" }}
      onWheel={() => {
        if (!isPlaying) return;
        userInteractingRef.current = true;
        setManualScrollHold(true);
        scheduleResumeAutoCenter();
      }}
      onTouchStart={() => {
        if (!isPlaying) return;
        userInteractingRef.current = true;
        setManualScrollHold(true);
        scheduleResumeAutoCenter();
      }}
      onMouseDown={() => {
        if (!isPlaying) return;
        userInteractingRef.current = true;
      }}
      onMouseUp={() => {
        if (!isPlaying) return;
        scheduleResumeAutoCenter();
      }}
      onScroll={() => {
        if (!isPlaying) return;
        if (isProgrammaticScrollRef.current) return;
        if (Date.now() - lastAutoCenterAtRef.current < 150) {
          return;
        }
        if (!userInteractingRef.current) {
          return;
        }
        setManualScrollHold(true);
        scheduleResumeAutoCenter();
      }}
    >
      <div className="w-full shrink-0" style={{ height: edgeSpacerHeight }} />
      {document.lines.map((line, index) => {
        const isActive = index === activeIndex;
        const isNear = Math.abs(index - activeIndex) <= 2;
        const isEditing = editingLineId === line.id;
        const upcomingDots = getUpcomingDots(index);
        return (
          <div
            key={line.id}
            ref={isActive ? activeRef : null}
            className={`group grid w-full grid-cols-[64px_minmax(0,1fr)_112px] items-center gap-3 rounded-xl px-5 py-3 transition-all duration-300 ${
              isActive
                ? "bg-[var(--selected-bg)] text-[var(--text-primary)] ring-1 ring-[var(--selected-border)]"
                : isNear
                ? "text-[var(--text-secondary)]"
                : "text-[var(--text-muted)]"
            }`}
            style={{ opacity: isNear || isActive ? 1 : 0.45 }}
            onClick={() => onSeek(Math.max(0, line.startMs + document.globalOffsetMs))}
            onDoubleClick={() => {
              setEditingLineId(line.id);
              setDraftText(line.text);
            }}
          >
            <span className="w-16 justify-self-start text-right font-mono text-[11px] text-[var(--text-muted)]">
              {formatTime(line.startMs + document.globalOffsetMs)}
            </span>
            <div className="relative flex min-h-[42px] w-full min-w-0 justify-center">
              <div
                className="w-full max-w-[860px] min-w-0 text-center"
                style={{ width: "min(860px, 100%)" }}
              >
                {isEditing ? (
                  <input
                    autoFocus
                    value={draftText}
                    className="ui-field block h-11 w-full text-center text-base outline-none"
                    onChange={(event) => setDraftText(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") saveLine(line, draftText);
                      if (event.key === "Escape") {
                        setEditingLineId(null);
                        setDraftText("");
                      }
                    }}
                    onBlur={() => saveLine(line, draftText)}
                  />
                ) : (
                  <>
                    {upcomingDots ? (
                      <div className="mb-1 text-[12px] tracking-[0.22em] text-[var(--accent)]">{upcomingDots}</div>
                    ) : null}
                    <div className={`${isActive ? "text-xl font-semibold" : "text-base"} min-w-0 overflow-hidden text-ellipsis leading-[1.55]`}>
                      {line.text || "· · ·"}
                    </div>
                  </>
                )}
              </div>
            </div>
            <div className="flex min-w-0 items-center justify-end gap-2 opacity-0 transition-opacity group-hover:opacity-100">
              <button
                className="ui-button h-8 min-h-8 px-3 text-[11px] text-[var(--text-secondary)] hover:bg-[var(--button-hover-bg)]"
                onClick={(event) => {
                  event.stopPropagation();
                  updateLineTime(line, -100);
                }}
              >
                -100ms
              </button>
              <button
                className="ui-button h-8 min-h-8 px-3 text-[11px] text-[var(--text-secondary)] hover:bg-[var(--button-hover-bg)]"
                onClick={(event) => {
                  event.stopPropagation();
                  updateLineTime(line, 100);
                }}
              >
                +100ms
              </button>
            </div>
          </div>
        );
      })}
      <div className="w-full shrink-0" style={{ height: edgeSpacerHeight }} />
    </div>
  );
}

function formatTime(ms: number) {
  const safeMs = Math.max(0, Math.floor(ms));
  const totalSeconds = Math.floor(safeMs / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  const centiseconds = Math.floor((safeMs % 1000) / 10);
  return `${minutes}:${seconds.toString().padStart(2, "0")}.${centiseconds.toString().padStart(2, "0")}`;
}
