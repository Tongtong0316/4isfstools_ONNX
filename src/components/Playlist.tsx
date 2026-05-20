import React, { useEffect, useMemo, useRef, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Song, STAGE_LABELS, STAGE_ICONS, STATUS_ICONS, ProcessingStage } from "../types";

interface PlaylistProps {
  songs: Song[];
  currentSong: Song | null;
  onSelectSong: (song: Song) => void;
  onDeleteSong: (id: string) => void;
  onCancelProcess: (song: Song) => void;
  onStartProcess: (song: Song) => void;
  onMoveSongToFolder: (songId: string, folderName: string | null) => Promise<void> | void;
  onRenameSong: (songId: string, newName: string) => Promise<void> | void;
  onRenameFolder: (oldName: string, newName: string) => Promise<void> | void;
  onSearchLyrics: (song: Song) => Promise<void> | void;
  onImportLyricsLrc: (song: Song) => Promise<void> | void;
  onGenerateLyricsDraft: (song: Song) => Promise<void> | void;
}

type SongMenuState = {
  kind: "song";
  x: number;
  y: number;
  song: Song;
};

type FolderMenuState = {
  kind: "folder";
  x: number;
  y: number;
  folderName: string;
};

type ContextMenuState = SongMenuState | FolderMenuState;

type InputDialogState =
  | { kind: "create-folder"; value: string }
  | { kind: "rename-song"; song: Song; value: string }
  | { kind: "rename-folder"; folderName: string; value: string }
  | { kind: "move-song"; song: Song; value: string };

type ConfirmDialogState =
  | { kind: "delete-song"; song: Song }
  | { kind: "delete-folder"; folderName: string };

const FOLDER_STORAGE_KEY = "4isfstools.playlistFolders";
const COLLAPSED_STORAGE_KEY = "4isfstools.playlistCollapsedFolders";
const VIEW_MODE_STORAGE_KEY = "4isfstools.playlistViewMode";
const DEFAULT_FOLDER = "未分组";
type PlaylistViewMode = "cards" | "list";

const iconStroke = "currentColor";
const SONG_CONTEXT_MENU_WIDTH = 240;
const FOLDER_CONTEXT_MENU_WIDTH = 240;
const SONG_CONTEXT_MENU_HEIGHT = 300;
const FOLDER_CONTEXT_MENU_HEIGHT = 152;

function MusicNoteIcon({ className = "" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="M9 18.5a2.5 2.5 0 1 1-2-2.45V6.5l10-2v9.55a2.5 2.5 0 1 1-2-2.45V8.4l-6 1.2v8.9Z" stroke={iconStroke} strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function FolderPlusIcon({ className = "" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="M4.5 7.5h5l1.8 2h8.2v8.8a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2V9.5a2 2 0 0 1 2-2Z" stroke={iconStroke} strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M12 13.2v4M10 15.2h4" stroke={iconStroke} strokeWidth="1.7" strokeLinecap="round" />
    </svg>
  );
}

function GridIcon({ className = "" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="M5 5h5v5H5V5Zm9 0h5v5h-5V5ZM5 14h5v5H5v-5Zm9 0h5v5h-5v-5Z" stroke={iconStroke} strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function SearchIcon({ className = "" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="m17.2 17.2 3.3 3.3M10.8 18a7.2 7.2 0 1 1 0-14.4 7.2 7.2 0 0 1 0 14.4Z" stroke={iconStroke} strokeWidth="1.9" strokeLinecap="round" />
    </svg>
  );
}

function MicIcon({ className = "" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="M8.2 11.8 17.6 2.4a3.2 3.2 0 0 1 4.5 4.5l-9.4 9.4" stroke={iconStroke} strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
      <path d="m7 13 4 4-5.5 3.5-2-2L7 13Z" fill="currentColor" opacity="0.22" />
      <path d="m7 13 4 4-5.5 3.5-2-2L7 13Z" stroke={iconStroke} strokeWidth="1.8" strokeLinejoin="round" />
      <path d="m15.7 4.3 4 4" stroke={iconStroke} strokeWidth="1.8" strokeLinecap="round" />
    </svg>
  );
}

function ClockIcon({ className = "" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Z" stroke={iconStroke} strokeWidth="1.8" />
      <path d="M12 7.5V12l3.1 2" stroke={iconStroke} strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function MoreIcon({ className = "" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <circle cx="6" cy="12" r="1.6" />
      <circle cx="12" cy="12" r="1.6" />
      <circle cx="18" cy="12" r="1.6" />
    </svg>
  );
}

function SplitAudioIcon({ className = "" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="M5 7.5h3.2c2.8 0 4.1 2.1 5.5 4.5 1.4 2.4 2.7 4.5 5.3 4.5" stroke={iconStroke} strokeWidth="1.8" strokeLinecap="round" />
      <path d="M5 16.5h3.2c1.6 0 2.7-.7 3.6-1.8M16.4 7.5H19m0 0-2-2m2 2-2 2M16.4 16.5H19m0 0-2-2m2 2-2 2" stroke={iconStroke} strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function FileTextIcon({ className = "" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="M6.5 3.5h7l4 4v13h-11v-17Z" stroke={iconStroke} strokeWidth="1.8" strokeLinejoin="round" />
      <path d="M13.5 3.5v4h4M8.8 12h6.4M8.8 15.5h6.4" stroke={iconStroke} strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function SparklesIcon({ className = "" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="m12 3 1.5 4.4L18 9l-4.5 1.6L12 15l-1.5-4.4L6 9l4.5-1.6L12 3Z" stroke={iconStroke} strokeWidth="1.7" strokeLinejoin="round" />
      <path d="m18.5 14 .8 2.2 2.2.8-2.2.8-.8 2.2-.8-2.2-2.2-.8 2.2-.8.8-2.2ZM5.5 14.5l.6 1.6 1.6.6-1.6.6-.6 1.6-.6-1.6-1.6-.6 1.6-.6.6-1.6Z" stroke={iconStroke} strokeWidth="1.5" strokeLinejoin="round" />
    </svg>
  );
}

function PencilIcon({ className = "" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="m4.5 16.8-.7 3.4 3.4-.7L18.9 7.8a2.4 2.4 0 0 0-3.4-3.4L4.5 16.8Z" stroke={iconStroke} strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
      <path d="m14.2 5.8 4 4" stroke={iconStroke} strokeWidth="1.8" strokeLinecap="round" />
    </svg>
  );
}

function FolderInputIcon({ className = "" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="M4 7.2h5l1.7 2h8.8v8.9a2 2 0 0 1-2 2H4.8a2 2 0 0 1-2-2V9.2a2 2 0 0 1 1.2-2Z" stroke={iconStroke} strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M9 14h6m0 0-2-2m2 2-2 2" stroke={iconStroke} strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function RevealIcon({ className = "" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="M4 8h4l1.5 2h6.5v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V10a2 2 0 0 1 1-2Z" stroke={iconStroke} strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M9 17V7l8 5-8 5Z" stroke={iconStroke} strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function TrashIcon({ className = "" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="M5 7h14M9 7V5.5A1.5 1.5 0 0 1 10.5 4h3A1.5 1.5 0 0 1 15 5.5V7m2 0-.7 12.2A2 2 0 0 1 14.3 21H9.7a2 2 0 0 1-2-1.8L7 7" stroke={iconStroke} strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M10 11v5M14 11v5" stroke={iconStroke} strokeWidth="1.8" strokeLinecap="round" />
    </svg>
  );
}

function XIcon({ className = "" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="m6.5 6.5 11 11M17.5 6.5l-11 11" stroke={iconStroke} strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
}

function ChevronToggleIcon({ className = "" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="m9 6 6 6-6 6" stroke={iconStroke} strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function ContextMenuItem({
  icon,
  children,
  destructive = false,
  onClick,
  ariaLabel,
}: {
  icon: React.ReactNode;
  children: React.ReactNode;
  destructive?: boolean;
  onClick: () => void;
  ariaLabel?: string;
}) {
  return (
    <button
      type="button"
      className={`context-menu-item ${destructive ? "context-menu-item-danger" : ""}`}
      data-danger={destructive || undefined}
      aria-label={ariaLabel}
      onClick={onClick}
    >
      <span className="context-menu-icon">{icon}</span>
      <span className="context-menu-label">{children}</span>
    </button>
  );
}

export default function Playlist({
  songs,
  currentSong,
  onSelectSong,
  onDeleteSong,
  onCancelProcess,
  onStartProcess,
  onMoveSongToFolder,
  onRenameSong,
  onRenameFolder,
  onSearchLyrics,
  onImportLyricsLrc,
  onGenerateLyricsDraft,
}: PlaylistProps) {
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
  const [draggedSongId, setDraggedSongId] = useState<string | null>(null);
  const [folderNames, setFolderNames] = useState<string[]>([]);
  const [collapsedFolders, setCollapsedFolders] = useState<Record<string, boolean>>({});
  const [inputDialog, setInputDialog] = useState<InputDialogState | null>(null);
  const [confirmDialog, setConfirmDialog] = useState<ConfirmDialogState | null>(null);
  const [searchText, setSearchText] = useState("");
  const [viewMode, setViewMode] = useState<PlaylistViewMode>(() => {
    try {
      return window.localStorage.getItem(VIEW_MODE_STORAGE_KEY) === "list" ? "list" : "cards";
    } catch {
      return "cards";
    }
  });
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    try {
      const storedFolders = window.localStorage.getItem(FOLDER_STORAGE_KEY);
      if (storedFolders) {
        const parsed = JSON.parse(storedFolders);
        if (Array.isArray(parsed)) {
          setFolderNames(parsed.filter((item) => typeof item === "string"));
        }
      }
    } catch {
      setFolderNames([]);
    }

    try {
      const storedCollapsed = window.localStorage.getItem(COLLAPSED_STORAGE_KEY);
      if (storedCollapsed) {
        const parsed = JSON.parse(storedCollapsed);
        if (parsed && typeof parsed === "object") {
          setCollapsedFolders(parsed as Record<string, boolean>);
        }
      }
    } catch {
      setCollapsedFolders({});
    }
  }, []);

  useEffect(() => {
    window.localStorage.setItem(FOLDER_STORAGE_KEY, JSON.stringify(folderNames));
  }, [folderNames]);

  useEffect(() => {
    window.localStorage.setItem(COLLAPSED_STORAGE_KEY, JSON.stringify(collapsedFolders));
  }, [collapsedFolders]);

  useEffect(() => {
    window.localStorage.setItem(VIEW_MODE_STORAGE_KEY, viewMode);
  }, [viewMode]);

  useEffect(() => {
    if (inputDialog) {
      window.requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [inputDialog]);

  const formatDuration = (ms: number) => {
    const s = Math.floor(ms / 1000);
    return `${Math.floor(s / 60)}:${(s % 60).toString().padStart(2, "0")}`;
  };

  const getStatusIcon = (song: Song) => {
    if (song.status === "processing") {
      return song.processingStage ? STAGE_ICONS[song.processingStage as ProcessingStage] : "⚙️";
    }
    return STATUS_ICONS[song.status] || "📄";
  };

  const normalizedSongFolder = (song: Song) => song.playlistFolder?.trim() || DEFAULT_FOLDER;

  const ensureFolderInState = useCallback((folderName: string) => {
    const trimmed = folderName.trim();
    if (!trimmed || trimmed === DEFAULT_FOLDER) return;
    setFolderNames((prev) => (prev.includes(trimmed) ? prev : [...prev, trimmed]));
  }, []);

  const allFolders = useMemo(() => {
    const seen = new Set<string>();
    const folders: string[] = [DEFAULT_FOLDER];
    for (const folderName of folderNames) {
      const trimmed = folderName.trim();
      if (trimmed && !seen.has(trimmed) && trimmed !== DEFAULT_FOLDER) {
        seen.add(trimmed);
        folders.push(trimmed);
      }
    }
    for (const song of songs) {
      const folderName = normalizedSongFolder(song);
      if (!seen.has(folderName) && folderName !== DEFAULT_FOLDER) {
        seen.add(folderName);
        folders.push(folderName);
      }
    }
    return folders;
  }, [folderNames, songs]);

  const songsByFolder = useMemo(() => {
    const map = new Map<string, Song[]>();
    for (const folderName of allFolders) {
      map.set(folderName, []);
    }
    for (const song of songs) {
      const folderName = normalizedSongFolder(song);
      if (!map.has(folderName)) {
        map.set(folderName, []);
      }
      map.get(folderName)!.push(song);
    }
    return map;
  }, [songs, allFolders]);

  const normalizedSearch = searchText.trim().toLowerCase();

  const filteredSongsByFolder = useMemo(() => {
    if (!normalizedSearch) return songsByFolder;
    const map = new Map<string, Song[]>();
    for (const folderName of allFolders) {
      const songsInFolder = songsByFolder.get(folderName) || [];
      const filtered = songsInFolder.filter((song) => {
        const folderValue = normalizedSongFolder(song).toLowerCase();
        return (
          song.name.toLowerCase().includes(normalizedSearch) ||
          song.id.toLowerCase().includes(normalizedSearch) ||
          folderValue.includes(normalizedSearch)
        );
      });
      if (filtered.length > 0) {
        map.set(folderName, filtered);
      }
    }
    return map;
  }, [allFolders, normalizedSearch, songsByFolder]);

  const visibleFolders = useMemo(() => {
    if (!normalizedSearch) return allFolders;
    return allFolders.filter((folderName) => (filteredSongsByFolder.get(folderName) || []).length > 0);
  }, [allFolders, normalizedSearch, filteredSongsByFolder]);

  const totalVisibleSongs = useMemo(() => {
    let count = 0;
    for (const folderName of visibleFolders) {
      count += (filteredSongsByFolder.get(folderName) || []).length;
    }
    return count;
  }, [visibleFolders, filteredSongsByFolder]);

  const closeContextMenu = () => setContextMenu(null);

  const clampMenuPosition = (x: number, y: number, kind: ContextMenuState["kind"]) => {
    const menuWidth = kind === "folder" ? FOLDER_CONTEXT_MENU_WIDTH : SONG_CONTEXT_MENU_WIDTH;
    const menuHeight = kind === "folder" ? FOLDER_CONTEXT_MENU_HEIGHT : SONG_CONTEXT_MENU_HEIGHT;
    const viewportPadding = 12;
    const pointerOffset = 6;
    const preferredX = x + pointerOffset;
    const preferredY = y + pointerOffset;
    const clampedX = Math.min(Math.max(viewportPadding, preferredX), window.innerWidth - menuWidth - viewportPadding);
    const clampedY = Math.min(Math.max(viewportPadding, preferredY), window.innerHeight - menuHeight - viewportPadding);
    return { x: clampedX, y: clampedY };
  };

  const openSongMenu = (e: React.MouseEvent, song: Song) => {
    e.preventDefault();
    const pos = clampMenuPosition(e.clientX, e.clientY, "song");
    setContextMenu({ kind: "song", x: pos.x, y: pos.y, song });
  };

  const openSongMenuFromButton = (e: React.MouseEvent, song: Song) => {
    e.preventDefault();
    e.stopPropagation();
    const rect = e.currentTarget.getBoundingClientRect();
    const pos = clampMenuPosition(rect.right - 8, rect.bottom + 6, "song");
    setContextMenu({ kind: "song", x: pos.x, y: pos.y, song });
  };

  const openFolderMenu = (e: React.MouseEvent, folderName: string) => {
    e.preventDefault();
    const pos = clampMenuPosition(e.clientX, e.clientY, "folder");
    setContextMenu({ kind: "folder", x: pos.x, y: pos.y, folderName });
  };

  const openCreateFolderDialog = useCallback(() => {
    setInputDialog({ kind: "create-folder", value: "" });
  }, []);

  const handleCreateFolderSubmit = useCallback((value: string) => {
    const trimmed = value.trim();
    if (!trimmed) return;
    ensureFolderInState(trimmed);
    setInputDialog(null);
  }, [ensureFolderInState]);

  const handleRenameFolder = async (folderName: string, nextName: string) => {
    const trimmed = nextName.trim();
    if (!trimmed || trimmed === folderName) return;
    await onRenameFolder(folderName, trimmed);
    setFolderNames((prev) => prev.map((item) => (item === folderName ? trimmed : item)));
    setCollapsedFolders((prev) => {
      const next = { ...prev };
      if (folderName in next) {
        next[trimmed] = next[folderName];
        delete next[folderName];
      }
      return next;
    });
  };

  const handleDeleteFolder = async (folderName: string) => {
    const songsInFolder = songsByFolder.get(folderName) || [];
    for (const song of songsInFolder) {
      await onMoveSongToFolder(song.id, null);
    }
    setFolderNames((prev) => prev.filter((item) => item !== folderName));
    setCollapsedFolders((prev) => {
      const next = { ...prev };
      delete next[folderName];
      return next;
    });
  };

  const openRenameSongDialog = useCallback((song: Song) => {
    setInputDialog({ kind: "rename-song", song, value: song.name });
  }, []);

  const openRenameFolderDialog = useCallback((folderName: string) => {
    setInputDialog({ kind: "rename-folder", folderName, value: folderName });
  }, []);

  const openMoveSongDialog = useCallback((song: Song) => {
    setInputDialog({ kind: "move-song", song, value: song.playlistFolder || "" });
  }, []);

  const openDeleteSongDialog = useCallback((song: Song) => {
    setConfirmDialog({ kind: "delete-song", song });
  }, []);

  const openDeleteFolderDialog = useCallback((folderName: string) => {
    setConfirmDialog({ kind: "delete-folder", folderName });
  }, []);

  const handleSongMove = async (song: Song, folderName: string | null) => {
    const normalized = folderName?.trim();
    await onMoveSongToFolder(song.id, normalized && normalized !== DEFAULT_FOLDER ? normalized : null);
    if (normalized && normalized !== DEFAULT_FOLDER) {
      ensureFolderInState(normalized);
    }
    closeContextMenu();
  };

  const handleSongRename = async (song: Song) => {
    openRenameSongDialog(song);
    closeContextMenu();
  };

  const handleSongClick = (song: Song) => {
    if (song.status === "queued") {
      onSelectSong(song);
      return;
    }
    if (song.status === "pending" || song.status === "error" || song.status === "cancelled") {
      onStartProcess(song);
      return;
    }
    onSelectSong(song);
  };

  const renderSongMeta = (song: Song) => {
    if (song.status === "ready" && song.duration > 0) {
      return formatDuration(song.duration);
    }
    if (song.status === "processing") {
      return song.processingStage
        ? `${STAGE_LABELS[song.processingStage as ProcessingStage]} ${song.progress}%`
        : `${song.progress}%`;
    }
    if (song.status === "pending") return "待处理";
    if (song.status === "queued") return "排队中";
    if (song.status === "cancelled") return "已取消";
    if (song.status === "cancelling") return "取消中";
    if (song.status === "error") return song.error_message || "处理失败";
    return "";
  };

  const renderSongCard = (song: Song) => {
    const meta = renderSongMeta(song);
    const active = currentSong?.id === song.id;
    const dimmed = song.status !== "ready" && song.status !== "processing" && song.status !== "error";

    return (
      <div
        key={song.id}
        className={`group relative cursor-pointer overflow-hidden border transition-colors ${
          viewMode === "list"
            ? "flex items-center border-transparent hover:bg-[var(--bg-tertiary)]"
            : "flex items-center border-[rgba(148,163,184,0.18)] bg-[var(--bg-card)] hover:border-[color-mix(in_srgb,var(--accent)_35%,transparent)] hover:bg-[var(--bg-tertiary)]"
        } ${song.status === "error" ? "opacity-80" : ""} ${dimmed ? "opacity-70" : ""} ${
          draggedSongId === song.id ? "ring-1 ring-[var(--accent)]/50 bg-[var(--bg-tertiary)]" : ""
        }`}
        style={{
          height: viewMode === "list" ? 40 : 84,
          margin: viewMode === "list" ? "4px 10px" : "8px 10px",
          padding: viewMode === "list" ? "0 12px" : "12px",
          gap: viewMode === "list" ? 8 : 12,
          borderRadius: 14,
          background:
            viewMode === "cards"
              ? "linear-gradient(135deg, color-mix(in srgb, var(--bg-card) 92%, var(--accent)), var(--bg-card) 68%)"
              : undefined,
          boxShadow: viewMode === "cards" ? "0 10px 26px rgba(0,0,0,0.18), inset 0 1px 0 rgba(255,255,255,0.035)" : "none",
        }}
        draggable
        onDragStart={() => setDraggedSongId(song.id)}
        onDragEnd={() => setDraggedSongId(null)}
        onClick={() => handleSongClick(song)}
        onContextMenu={(e) => openSongMenu(e, song)}
      >
        {active && (
          <div className={`${viewMode === "list" ? "top-2 bottom-2" : "top-3 bottom-3"} absolute left-0 w-[3px] rounded-r-full bg-[var(--accent)]/80`} />
        )}
        {viewMode === "cards" && (
          <span
            className="flex shrink-0 items-center justify-center text-[var(--accent)]"
            style={{
              width: 48,
              height: 48,
              borderRadius: 12,
              background: "color-mix(in srgb, var(--accent) 14%, var(--bg-tertiary))",
              boxShadow: "inset 0 1px 0 rgba(255,255,255,0.04)",
            }}
          >
            {song.status === "ready" ? <MicIcon className="h-7 w-7" /> : getStatusIcon(song)}
          </span>
        )}
        <div className={viewMode === "list" ? "grid min-w-0 flex-1 grid-cols-[minmax(0,1fr)_auto] items-center gap-3" : "min-w-0 flex-1 self-center"}>
          <div className={`${viewMode === "list" ? "text-[13px]" : "text-[15px]"} ui-text-ellipsis min-w-0 font-semibold text-[var(--text-primary)]`} title={song.name}>
            {song.name}
          </div>
          {viewMode === "list" ? (
            <div className="max-w-[82px] truncate text-right text-[13px] text-[var(--text-muted)]">
              {meta}
            </div>
          ) : (
            <div className="mt-1.5 min-w-0 text-[13px] text-[var(--text-muted)]">
              {song.status === "processing" ? (
                <div className="flex items-center gap-2">
                  <div className="h-1 flex-1 overflow-hidden rounded-full bg-[var(--border)]">
                    <div
                      className="h-full transition-all duration-500"
                      style={{ width: `${song.progress}%`, background: "var(--accent)" }}
                    />
                  </div>
                  <span className="shrink-0 font-mono text-[11px] text-[var(--accent)]">{song.progress}%</span>
                </div>
              ) : (
                <span className={`${song.status === "error" ? "block truncate text-[#ef4444]" : "inline-flex items-center gap-1.5"}`}>
                  {song.status === "ready" && <ClockIcon className="h-3.5 w-3.5" />}
                  {meta}
                </span>
              )}
            </div>
          )}
        </div>
        {viewMode === "cards" && (
          <button
            type="button"
            aria-label="更多操作"
            className="ml-1 flex shrink-0 items-center justify-center rounded-full text-[var(--text-muted)] opacity-80 transition-colors hover:bg-[var(--bg-tertiary)] hover:text-[var(--text-primary)]"
            style={{ width: 24, height: 24 }}
            onClick={(e) => openSongMenuFromButton(e, song)}
          >
            <MoreIcon className="h-5 w-5" />
          </button>
        )}
      </div>
    );
  };

  return (
    <div
      className="flex h-full flex-1 flex-col overflow-hidden border bg-[var(--bg-secondary)]"
      style={{
        borderColor: "var(--panel-accent-border)",
        borderRadius: 14,
        background:
          "linear-gradient(180deg, color-mix(in srgb, var(--bg-secondary) 94%, var(--accent)) 0%, var(--bg-secondary) 34%, var(--bg-primary) 100%)",
        boxShadow: "0 0 0 1px var(--panel-inner-border), 0 12px 32px var(--panel-glow)",
      }}
    >
      <div className="border-b border-[var(--border)]" style={{ padding: "16px 16px 10px" }}>
        <div className="flex items-center justify-between gap-3">
          <div className="flex min-w-0 items-center gap-3">
            <span className="ui-text-ellipsis shrink text-[22px] font-bold leading-none text-[var(--text-primary)]">播放列表</span>
          </div>
          <div className="flex shrink-0 items-center gap-2 text-sm font-medium text-[var(--text-muted)]">
            <span>{songs.length} 首</span>
            <span
              className="flex items-center justify-center border border-[rgba(148,163,184,0.18)] bg-[var(--bg-card)] text-[15px] text-[var(--text-muted)]"
              style={{ width: 36, height: 36, borderRadius: 10 }}
            >
              <MusicNoteIcon className="h-5 w-5" />
            </span>
          </div>
        </div>
        <div className="flex items-center justify-between" style={{ height: 40, gap: 12, marginTop: 20 }}>
          <button
            type="button"
            className="ui-button max-w-[48%] flex-1 border border-[rgba(148,163,184,0.18)] bg-[var(--bg-card)] text-[13px] font-semibold text-[var(--text-secondary)] transition-colors hover:bg-[var(--bg-tertiary)]"
            style={{ height: 40, borderRadius: 10, padding: "0 12px" }}
            onClick={openCreateFolderDialog}
          >
            <FolderPlusIcon className="h-4.5 w-4.5 shrink-0" />
            <span className="truncate">新建文件夹</span>
          </button>
          <button
            type="button"
            onClick={() => setViewMode((mode) => mode === "cards" ? "list" : "cards")}
            className="ui-button max-w-[48%] flex-1 border border-[color-mix(in_srgb,var(--accent)_35%,transparent)] bg-[var(--bg-card)] text-[13px] font-semibold text-[var(--accent)] transition-colors hover:bg-[var(--bg-tertiary)]"
            style={{ height: 40, borderRadius: 10, padding: "0 12px" }}
          >
            <GridIcon className="h-4.5 w-4.5 shrink-0" />
            <span>{viewMode === "cards" ? "卡片视图" : "列表视图"}</span>
          </button>
        </div>
      </div>
      <div className="border-b border-[var(--border)]" style={{ padding: "10px 16px 12px" }}>
        <div className="ui-search">
          <span className="ui-search-icon">
            <SearchIcon className="h-4.5 w-4.5" />
          </span>
          <input
            type="text"
            value={searchText}
            onChange={(e) => setSearchText(e.target.value)}
            placeholder="搜索歌曲 / 文件夹"
            className="ui-field ui-search-input border-[rgba(148,163,184,0.18)] bg-[var(--bg-card)] text-[13px] placeholder:text-[var(--text-muted)] focus:border-[var(--accent)]"
            style={{ height: 40, borderRadius: 10 }}
          />
        </div>
      </div>

      <div className="flex-1 overflow-y-auto py-0">
        {songs.length === 0 && allFolders.length === 1 ? (
          <div className="p-8 text-center text-[#71717a] text-sm">暂无歌曲，点击右上“导入歌曲”开始添加</div>
        ) : normalizedSearch && totalVisibleSongs === 0 ? (
          <div className="p-8 text-center text-[#71717a] text-sm">没有匹配歌曲，换个关键词试试</div>
        ) : (
          <div className="flex flex-col">
            {visibleFolders.map((folderName) => {
              const folderSongs = filteredSongsByFolder.get(folderName) || [];
              const isCollapsed = collapsedFolders[folderName] ?? false;
              const isDropActive = draggedSongId !== null;
              return (
                <div key={folderName} className="overflow-hidden border-b border-[var(--border)] bg-[var(--bg-secondary)]">
                  <div
                    className={`flex items-center justify-between bg-[var(--bg-card)] ${isDropActive ? "transition-colors" : ""}`}
                    style={{ height: 36, padding: "0 16px" }}
                    onClick={() => setCollapsedFolders((prev) => ({ ...prev, [folderName]: !isCollapsed }))}
                    onContextMenu={(e) => openFolderMenu(e, folderName)}
                    onDragOver={(e) => {
                      if (!draggedSongId) return;
                      e.preventDefault();
                      e.dataTransfer.dropEffect = "move";
                    }}
                    onDrop={async (e) => {
                      e.preventDefault();
                      if (!draggedSongId) return;
                      const draggedSong = songs.find((song) => song.id === draggedSongId);
                      if (!draggedSong) return;
                      await handleSongMove(draggedSong, folderName === DEFAULT_FOLDER ? null : folderName);
                      setDraggedSongId(null);
                    }}
                  >
                    <div className="flex min-w-0 items-center gap-2">
                      <span className="text-[var(--text-muted)] text-xs leading-none">{isCollapsed ? "▸" : "▾"}</span>
                      <span className="ui-text-ellipsis truncate text-[16px] font-bold leading-none text-[var(--text-primary)]" title={folderName}>{folderName}</span>
                      <span className="text-[14px] font-semibold leading-none text-[var(--text-muted)]">{folderSongs.length}</span>
                    </div>
                    <button
                      className="flex h-6 w-6 items-center justify-center rounded-full text-[15px] text-[var(--text-muted)] hover:bg-[var(--bg-tertiary)] hover:text-[var(--text-primary)]"
                      onClick={(e) => {
                        e.stopPropagation();
                        setCollapsedFolders((prev) => ({ ...prev, [folderName]: !isCollapsed }));
                      }}
                    >
                      ›
                    </button>
                  </div>
                  {!isCollapsed && (
                    <div className={`${viewMode === "list" ? "py-1" : "py-0"} flex flex-col`}>
                      {folderSongs.length === 0 ? (
                        <div className="px-3 py-4 text-xs text-[#52525b] text-center border border-dashed border-white/[0.06] rounded-lg">
                          可将歌曲拖到这里创建/整理文件夹
                        </div>
                      ) : (
                        folderSongs.map(renderSongCard)
                      )}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>

      {contextMenu && (
        <>
          <div className="fixed inset-0 z-[80]" onClick={closeContextMenu} />
          <div
            className="context-menu"
            style={{ left: contextMenu.x, top: contextMenu.y }}
          >
            {contextMenu.kind === "song" ? (
              <>
                {contextMenu.song.status !== "processing" && contextMenu.song.status !== "cancelling" && contextMenu.song.status !== "queued" && (
                  <ContextMenuItem
                    icon={<SplitAudioIcon className="h-5 w-5" />}
                    onClick={() => {
                      onStartProcess(contextMenu.song);
                      closeContextMenu();
                    }}
                  >
                    剥离伴奏
                  </ContextMenuItem>
                )}
                {contextMenu.song.status !== "processing" && contextMenu.song.status !== "cancelling" && (
                  <ContextMenuItem
                    icon={<SearchIcon className="h-5 w-5" />}
                    onClick={() => {
                      void onSearchLyrics(contextMenu.song);
                      closeContextMenu();
                    }}
                  >
                    搜索匹配歌词
                  </ContextMenuItem>
                )}
                <ContextMenuItem
                  icon={<FileTextIcon className="h-5 w-5" />}
                  onClick={() => {
                    void onImportLyricsLrc(contextMenu.song);
                    closeContextMenu();
                  }}
                >
                  导入 LRC 歌词
                </ContextMenuItem>
                {contextMenu.song.status !== "processing" && contextMenu.song.status !== "cancelling" && (
                  <ContextMenuItem
                    icon={<SparklesIcon className="h-5 w-5" />}
                    onClick={() => {
                      void onGenerateLyricsDraft(contextMenu.song);
                      closeContextMenu();
                    }}
                  >
                    AI 听写（草稿）
                  </ContextMenuItem>
                )}
                {(contextMenu.song.status === "processing" || contextMenu.song.status === "queued" || contextMenu.song.status === "cancelling") && (
                  <ContextMenuItem
                    destructive
                    ariaLabel="取消处理"
                    icon={<XIcon className="h-5 w-5" />}
                    onClick={() => {
                      onCancelProcess(contextMenu.song);
                      closeContextMenu();
                    }}
                  >
                    取消处理
                  </ContextMenuItem>
                )}
                <ContextMenuItem
                  icon={<PencilIcon className="h-5 w-5" />}
                  onClick={() => handleSongRename(contextMenu.song)}
                >
                  重命名
                </ContextMenuItem>
                <ContextMenuItem
                  icon={<FolderInputIcon className="h-5 w-5" />}
                  onClick={() => {
                    openMoveSongDialog(contextMenu.song);
                    closeContextMenu();
                  }}
                >
                  移动到...
                </ContextMenuItem>
                <ContextMenuItem
                  icon={<RevealIcon className="h-5 w-5" />}
                  onClick={async () => {
                    try {
                      await invoke("reveal_in_file_manager", { path: contextMenu.song.originalPath });
                    } catch (e) {
                      console.error("Failed to reveal in file manager:", e);
                    }
                    closeContextMenu();
                  }}
                >
                  {navigator.platform.startsWith("Mac") ? "在访达中打开" : "在资源管理器中打开"}
                </ContextMenuItem>
                {contextMenu.song.status !== "processing" && contextMenu.song.status !== "queued" && contextMenu.song.status !== "cancelling" && (
                  <>
                    <div className="context-menu-separator" />
                    <ContextMenuItem
                      destructive
                      ariaLabel="删除歌曲"
                      icon={<TrashIcon className="h-5 w-5" />}
                      onClick={() => {
                        openDeleteSongDialog(contextMenu.song);
                        closeContextMenu();
                      }}
                    >
                      删除
                    </ContextMenuItem>
                  </>
                )}
              </>
            ) : (
              <>
                <ContextMenuItem
                  icon={<PencilIcon className="h-5 w-5" />}
                  onClick={() => {
                    openRenameFolderDialog(contextMenu.folderName);
                    closeContextMenu();
                  }}
                >
                  重命名文件夹
                </ContextMenuItem>
                <ContextMenuItem
                  icon={<ChevronToggleIcon className="h-5 w-5" />}
                  onClick={() => {
                    setCollapsedFolders((prev) => ({ ...prev, [contextMenu.folderName]: !(prev[contextMenu.folderName] ?? false) }));
                    closeContextMenu();
                  }}
                >
                  切换折叠
                </ContextMenuItem>
                {contextMenu.folderName !== DEFAULT_FOLDER && (
                  <>
                    <div className="context-menu-separator" />
                    <ContextMenuItem
                      destructive
                      ariaLabel="删除文件夹"
                      icon={<TrashIcon className="h-5 w-5" />}
                      onClick={() => {
                        openDeleteFolderDialog(contextMenu.folderName);
                        closeContextMenu();
                      }}
                    >
                      删除文件夹
                    </ContextMenuItem>
                  </>
                )}
              </>
            )}
          </div>
        </>
      )}

      {inputDialog && (
        <>
          <div
            className="fixed inset-0 z-40 bg-black/55 backdrop-blur-[2px]"
            onClick={() => setInputDialog(null)}
          />
          <div className="fixed inset-0 z-50 flex items-center justify-center p-6">
            <div className="modal-shell">
              <div className="dialog-content">
                <div className="text-[18px] font-semibold leading-7 text-[var(--text-primary)]">
                  {inputDialog.kind === "create-folder" && "新建文件夹"}
                  {inputDialog.kind === "rename-song" && "重命名歌曲"}
                  {inputDialog.kind === "rename-folder" && "重命名文件夹"}
                  {inputDialog.kind === "move-song" && "移动到文件夹"}
                </div>
                <div className="modal-copy mt-2 text-sm">
                  {inputDialog.kind === "create-folder" && "输入名称后回车即可创建"}
                  {inputDialog.kind === "rename-song" && "输入新名称后回车保存"}
                  {inputDialog.kind === "rename-folder" && "输入新名称后回车保存"}
                  {inputDialog.kind === "move-song" && "输入目标文件夹后回车确认"}
                </div>
                <input
                  ref={inputRef}
                  value={inputDialog.value}
                  onChange={(e) => {
                    const value = e.target.value;
                    setInputDialog((prev) => prev ? { ...prev, value } as InputDialogState : prev);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      if (inputDialog.kind === "create-folder") {
                        handleCreateFolderSubmit(inputDialog.value);
                        return;
                      }
                      if (inputDialog.kind === "rename-song") {
                        void onRenameSong(inputDialog.song.id, inputDialog.value.trim());
                        setInputDialog(null);
                        return;
                      }
                      if (inputDialog.kind === "rename-folder") {
                        void handleRenameFolder(inputDialog.folderName, inputDialog.value);
                        setInputDialog(null);
                        return;
                      }
                      if (inputDialog.kind === "move-song") {
                        void handleSongMove(inputDialog.song, inputDialog.value);
                        setInputDialog(null);
                      }
                    }
                    if (e.key === "Escape") {
                      e.preventDefault();
                      setInputDialog(null);
                    }
                  }}
                  placeholder={
                    inputDialog.kind === "create-folder"
                      ? "例如：主歌、收藏、待整理"
                      : inputDialog.kind === "move-song"
                        ? "例如：待整理、演出备份"
                        : "请输入名称"
                  }
                  className="folder-name-input rounded-2xl border border-white/[0.08] bg-[var(--bg-primary)] px-5 py-0 text-base leading-[48px] text-[var(--text-primary)] outline-none ring-0 placeholder:text-[var(--text-muted)] focus:border-[var(--accent)] focus:ring-2 focus:ring-[var(--accent)]/25"
                />
                <div className="dialog-actions">
                  <button
                    type="button"
                    className="inline-flex min-w-[92px] items-center justify-center whitespace-nowrap rounded-full px-6 py-2.5 text-sm font-medium text-[var(--text-secondary)] hover:bg-white/[0.06]"
                    onClick={() => setInputDialog(null)}
                  >
                    取消
                  </button>
                  <button
                    type="button"
                    className="inline-flex min-w-[92px] items-center justify-center whitespace-nowrap rounded-full bg-[var(--accent)] px-6 py-2.5 text-sm font-semibold text-white hover:bg-[var(--accent-hover)]"
                    onClick={() => {
                      if (inputDialog.kind === "create-folder") {
                        handleCreateFolderSubmit(inputDialog.value);
                        return;
                      }
                      if (inputDialog.kind === "rename-song") {
                        void onRenameSong(inputDialog.song.id, inputDialog.value.trim());
                        setInputDialog(null);
                        return;
                      }
                      if (inputDialog.kind === "rename-folder") {
                        void handleRenameFolder(inputDialog.folderName, inputDialog.value);
                        setInputDialog(null);
                        return;
                      }
                      if (inputDialog.kind === "move-song") {
                        void handleSongMove(inputDialog.song, inputDialog.value);
                        setInputDialog(null);
                      }
                    }}
                  >
                    {inputDialog.kind === "create-folder" ? "创建" : "确认"}
                  </button>
                </div>
              </div>
            </div>
          </div>
        </>
      )}

      {confirmDialog && (
        <>
          <div
            className="fixed inset-0 z-50 bg-black/55 backdrop-blur-[2px]"
            onClick={() => setConfirmDialog(null)}
          />
          <div className="fixed inset-0 z-[60] flex items-center justify-center p-6">
            <div className="destructive-dialog">
              <div className="destructive-dialog-header">
                <div className="destructive-dialog-icon">
                  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/>
                    <line x1="12" y1="9" x2="12" y2="13"/>
                    <line x1="12" y1="17" x2="12.01" y2="17"/>
                  </svg>
                </div>
                <span className="destructive-dialog-title">
                  {confirmDialog.kind === "delete-song" ? "删除歌曲" : "删除文件夹"}
                </span>
              </div>
              <div className="destructive-dialog-body">
                <p className="primary-message">
                  {confirmDialog.kind === "delete-song"
                    ? <>确认删除「<span className="song-name">{confirmDialog.song.name}</span>」？</>
                    : <>确认删除文件夹「{confirmDialog.folderName}」？</>}
                </p>
                <p className="secondary-message">
                  {confirmDialog.kind === "delete-song"
                    ? "此操作会同时移除本地数据，删除后不可从本应用内恢复。"
                    : "里面的歌曲会回到未分组。"}
                </p>
              </div>
              <div className="destructive-dialog-footer">
                <button
                  type="button"
                  className="cancel-btn"
                  onClick={() => setConfirmDialog(null)}
                  autoFocus
                >
                  取消
                </button>
                <button
                  type="button"
                  className="delete-btn"
                  data-danger="true"
                  aria-label={confirmDialog.kind === "delete-song" ? `确认删除歌曲 ${confirmDialog.song.name}` : `确认删除文件夹 ${confirmDialog.folderName}`}
                  onClick={() => {
                    if (confirmDialog.kind === "delete-song") {
                      onDeleteSong(confirmDialog.song.id);
                    } else {
                      void handleDeleteFolder(confirmDialog.folderName);
                    }
                    setConfirmDialog(null);
                  }}
                >
                  删除
                </button>
              </div>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
