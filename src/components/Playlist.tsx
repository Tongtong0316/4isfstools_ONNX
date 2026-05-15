import React, { useEffect, useMemo, useRef, useState, useCallback } from "react";
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
const DEFAULT_FOLDER = "未分组";

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

  const clampMenuPosition = (x: number, y: number) => {
    const menuWidth = 240;
    const menuHeight = contextMenu?.kind === "folder" ? 180 : 348;
    const clampedX = Math.min(Math.max(12, x), window.innerWidth - menuWidth - 12);
    const clampedY = Math.min(Math.max(12, y), window.innerHeight - menuHeight - 12);
    return { x: clampedX, y: clampedY };
  };

  const openSongMenu = (e: React.MouseEvent, song: Song) => {
    e.preventDefault();
    const pos = clampMenuPosition(e.clientX, e.clientY);
    setContextMenu({ kind: "song", x: pos.x, y: pos.y, song });
  };

  const openFolderMenu = (e: React.MouseEvent, folderName: string) => {
    e.preventDefault();
    const pos = clampMenuPosition(e.clientX, e.clientY);
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
    if (song.status === "pending" || song.status === "error" || song.status === "cancelled" || song.status === "queued") {
      onStartProcess(song);
      return;
    }
    onSelectSong(song);
  };

  const renderSongCard = (song: Song) => (
    <div
      key={song.id}
      className={`group relative rounded-lg flex items-start gap-3 cursor-pointer hover:bg-[#1e1e1e] transition-colors ${song.status === "error" ? "opacity-80" : ""} ${song.status !== "ready" && song.status !== "processing" && song.status !== "error" ? "opacity-70" : ""} ${draggedSongId === song.id ? "ring-1 ring-[#6366f1] bg-[#1e1e1e]" : ""}`}
      style={{ padding: "12px 16px" }}
      draggable
      onDragStart={() => setDraggedSongId(song.id)}
      onDragEnd={() => setDraggedSongId(null)}
      onClick={() => handleSongClick(song)}
      onContextMenu={(e) => openSongMenu(e, song)}
    >
      {currentSong?.id === song.id && (
        <div className="absolute left-0 top-3 bottom-3 w-1 rounded-full bg-[#6366f1]" />
      )}
      <span className="text-lg" style={{ paddingLeft: "8px" }}>{getStatusIcon(song)}</span>
      <div className="flex-1 min-w-0">
        <div className="font-medium text-sm text-[#fafafa] truncate">{song.name}</div>
        <div className="mt-1">
          {song.status === "ready" && song.duration > 0 && (
            <span className="text-xs text-[#71717a]">{formatDuration(song.duration)}</span>
          )}
          {song.status === "processing" && (
            <div>
              <div className="flex items-center gap-2 mb-1">
                <div className="flex-1 h-1 bg-[#2e2e2e] rounded-full overflow-hidden">
                  <div
                    className="h-full bg-gradient-to-r from-[#f472b6] to-[#c084fc] transition-all duration-500"
                    style={{ width: `${song.progress}%` }}
                  />
                </div>
                <span className="text-xs text-[#f472b6] font-mono">{song.progress}%</span>
              </div>
              {song.processingStage && (
                <div className="text-xs text-[#f472b6]">
                  {STAGE_LABELS[song.processingStage as ProcessingStage]}
                </div>
              )}
            </div>
          )}
          {song.status === "pending" && (
            <span className="text-xs text-[#71717a]">点击开始处理</span>
          )}
          {song.status === "queued" && (
            <span className="text-xs text-[#60a5fa]">排队中...</span>
          )}
          {song.status === "error" && (
            <span className="text-xs text-[#ef4444] truncate block">
              {song.error_message || "处理失败"}
            </span>
          )}
          {song.status === "cancelled" && (
            <span className="text-xs text-[#f59e0b]">已取消，点击重新处理</span>
          )}
          {song.status === "cancelling" && (
            <span className="text-xs text-[#60a5fa]">正在取消...</span>
          )}
        </div>
      </div>
    </div>
  );

  return (
    <div className="h-full rounded-xl border border-[var(--border)] bg-[var(--bg-secondary)] overflow-hidden flex flex-col flex-1">
      <div className="py-4 font-semibold text-sm border-b border-[var(--border)] flex items-center justify-between" style={{ paddingLeft: "24px", paddingRight: "24px" }}>
        <div className="flex items-center gap-3">
          <span className="text-[#fafafa]">播放列表</span>
          <button
            type="button"
            className="rounded-full bg-[#1e1e1e] px-3 py-1 text-xs text-[#d4d4d8] hover:bg-[#2a2a4a]"
            onClick={openCreateFolderDialog}
          >
            新建文件夹
          </button>
        </div>
        <span className="text-xs text-[#71717a] bg-[#1e1e1e] px-3 py-0.5 rounded-full">
          {songs.filter((s) => s.status === "ready").length} / {songs.length}
        </span>
      </div>
      <div className="border-b border-[var(--border)]" style={{ padding: "10px 24px 12px" }}>
        <input
          type="text"
          value={searchText}
          onChange={(e) => setSearchText(e.target.value)}
          placeholder="搜索歌曲/文件夹"
          className="w-full h-9 rounded-lg border border-white/[0.08] bg-[#161616] px-3 text-sm text-[#fafafa] placeholder:text-[#6b7280] outline-none focus:border-[#6366f1] focus:ring-1 focus:ring-[#6366f1]"
        />
      </div>

      <div className="flex-1 overflow-y-auto" style={{ padding: "18px 18px 20px" }}>
        {songs.length === 0 && allFolders.length === 1 ? (
          <div className="p-8 text-center text-[#71717a] text-sm">暂无歌曲，点击右上“导入歌曲”开始添加</div>
        ) : normalizedSearch && totalVisibleSongs === 0 ? (
          <div className="p-8 text-center text-[#71717a] text-sm">没有匹配歌曲，换个关键词试试</div>
        ) : (
          <div className="flex flex-col gap-3">
            {visibleFolders.map((folderName) => {
              const folderSongs = filteredSongsByFolder.get(folderName) || [];
              const isCollapsed = collapsedFolders[folderName] ?? false;
              const isDropActive = draggedSongId !== null;
              return (
                <div key={folderName} className="rounded-xl border border-white/[0.04] bg-white/[0.015] overflow-hidden">
                  <div
                    className={`flex items-center justify-between px-4 py-3 border-b border-white/[0.04] ${isDropActive ? "transition-colors" : ""}`}
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
                    <div className="flex items-center gap-3 min-w-0">
                      <span className="text-[#71717a]">{isCollapsed ? "▸" : "▾"}</span>
                      <span className="text-sm font-medium text-[#fafafa] truncate">{folderName}</span>
                      <span className="text-xs text-[#71717a]">({folderSongs.length})</span>
                    </div>
                    <button
                      className="text-xs text-[#71717a] hover:text-white"
                      onClick={(e) => {
                        e.stopPropagation();
                        setCollapsedFolders((prev) => ({ ...prev, [folderName]: !isCollapsed }));
                      }}
                    >
                      {isCollapsed ? "展开" : "收起"}
                    </button>
                  </div>
                  {!isCollapsed && (
                    <div className="p-2 flex flex-col gap-2">
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
          <div className="fixed inset-0 z-40" onClick={closeContextMenu} />
          <div
            className="fixed bg-[#1e1e1e] border border-[#2e2e2e] rounded-lg shadow-xl py-1 z-50 min-w-56"
            style={{ left: contextMenu.x, top: contextMenu.y }}
          >
            {contextMenu.kind === "song" ? (
              <>
                {contextMenu.song.status !== "processing" && contextMenu.song.status !== "cancelling" && (
                  <button
                    className="w-full px-4 py-2.5 text-left text-sm text-[#fafafa] hover:bg-[#2e2e2e] flex items-center gap-3"
                    onClick={() => {
                      onStartProcess(contextMenu.song);
                      closeContextMenu();
                    }}
                  >
                    <span className="text-[#f472b6]">◒</span> 剥离伴奏
                  </button>
                )}
                {contextMenu.song.status !== "processing" && contextMenu.song.status !== "cancelling" && (
                  <button
                    className="w-full px-4 py-2.5 text-left text-sm text-[#fafafa] hover:bg-[#2e2e2e] flex items-center gap-3"
                    onClick={() => {
                      void onSearchLyrics(contextMenu.song);
                      closeContextMenu();
                    }}
                  >
                    <span className="text-[#22c55e]">🔎</span> 搜索匹配歌词
                  </button>
                )}
                <button
                  className="w-full px-4 py-2.5 text-left text-sm text-[#fafafa] hover:bg-[#2e2e2e] flex items-center gap-3"
                  onClick={() => {
                    void onImportLyricsLrc(contextMenu.song);
                    closeContextMenu();
                  }}
                >
                  <span className="text-[#60a5fa]">♫</span> 导入 LRC 歌词
                </button>
                {contextMenu.song.status !== "processing" && contextMenu.song.status !== "cancelling" && (
                  <button
                    className="w-full px-4 py-2.5 text-left text-sm text-[#fafafa] hover:bg-[#2e2e2e] flex items-center gap-3"
                    onClick={() => {
                      void onGenerateLyricsDraft(contextMenu.song);
                      closeContextMenu();
                    }}
                  >
                    <span className="text-[#a855f7]">♪</span> AI 听写（草稿）
                  </button>
                )}
                {contextMenu.song.status === "processing" && (
                  <button
                    className="w-full px-4 py-2.5 text-left text-sm text-[#ef4444] hover:bg-[#2e2e2e] flex items-center gap-3"
                    onClick={() => {
                      onCancelProcess(contextMenu.song);
                      closeContextMenu();
                    }}
                  >
                    <span className="text-[#ef4444]">✕</span> 取消处理
                  </button>
                )}
                <button
                  className="w-full px-4 py-2.5 text-left text-sm text-[#fafafa] hover:bg-[#2e2e2e] flex items-center gap-3"
                  onClick={() => handleSongRename(contextMenu.song)}
                >
                  <span className="text-[#60a5fa]">✎</span> 重命名
                </button>
                <button
                  className="w-full px-4 py-2.5 text-left text-sm text-[#fafafa] hover:bg-[#2e2e2e] flex items-center gap-3"
                  onClick={() => {
                    openMoveSongDialog(contextMenu.song);
                    closeContextMenu();
                  }}
                >
                  <span className="text-[#22c55e]">📂</span> 移动到...
                </button>
                {contextMenu.song.status !== "processing" && (
                  <button
                    className="w-full px-4 py-2.5 text-left text-sm text-[#ef4444] hover:bg-[#2e2e2e] flex items-center gap-3"
                    onClick={() => {
                      openDeleteSongDialog(contextMenu.song);
                      closeContextMenu();
                    }}
                  >
                    <span>✕</span> 删除
                  </button>
                )}
              </>
            ) : (
              <>
                <button
                  className="w-full px-4 py-2.5 text-left text-sm text-[#fafafa] hover:bg-[#2e2e2e] flex items-center gap-3"
                  onClick={() => {
                    openRenameFolderDialog(contextMenu.folderName);
                    closeContextMenu();
                  }}
                >
                  <span className="text-[#60a5fa]">✎</span> 重命名文件夹
                </button>
                <button
                  className="w-full px-4 py-2.5 text-left text-sm text-[#fafafa] hover:bg-[#2e2e2e] flex items-center gap-3"
                  onClick={() => {
                    setCollapsedFolders((prev) => ({ ...prev, [contextMenu.folderName]: !(prev[contextMenu.folderName] ?? false) }));
                    closeContextMenu();
                  }}
                >
                  <span className="text-[#f59e0b]">▸</span> 切换折叠
                </button>
                {contextMenu.folderName !== DEFAULT_FOLDER && (
                  <button
                    className="w-full px-4 py-2.5 text-left text-sm text-[#ef4444] hover:bg-[#2e2e2e] flex items-center gap-3"
                    onClick={() => {
                      openDeleteFolderDialog(contextMenu.folderName);
                      closeContextMenu();
                    }}
                  >
                    <span>✕</span> 删除文件夹
                  </button>
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
            <div className="w-full max-w-md rounded-3xl border border-white/[0.08] bg-[#171717] shadow-2xl shadow-black/50">
              <div className="dialog-content">
                <div className="text-base font-semibold text-[#fafafa]">
                  {inputDialog.kind === "create-folder" && "新建文件夹"}
                  {inputDialog.kind === "rename-song" && "重命名歌曲"}
                  {inputDialog.kind === "rename-folder" && "重命名文件夹"}
                  {inputDialog.kind === "move-song" && "移动到文件夹"}
                </div>
                <div className="mt-2 text-sm leading-6 text-[#8a8a94]">
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
                  className="folder-name-input rounded-2xl border border-white/[0.08] bg-[#101010] px-6 py-0 text-base leading-[48px] text-[#fafafa] outline-none ring-0 placeholder:text-[#5b5b66] focus:border-[#6366f1] focus:ring-2 focus:ring-[#6366f1]/25"
                />
                <div className="dialog-actions">
                  <button
                    type="button"
                    className="inline-flex min-w-[92px] items-center justify-center whitespace-nowrap rounded-full px-6 py-2.5 text-sm font-medium text-[#d4d4d8] hover:bg-white/[0.06]"
                    onClick={() => setInputDialog(null)}
                  >
                    取消
                  </button>
                  <button
                    type="button"
                    className="inline-flex min-w-[92px] items-center justify-center whitespace-nowrap rounded-full bg-[#6366f1] px-6 py-2.5 text-sm font-semibold text-white hover:bg-[#4f46e5]"
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
          <div className="fixed inset-0 z-40 bg-black/55 backdrop-blur-[2px]" onClick={() => setConfirmDialog(null)} />
          <div className="fixed inset-0 z-50 flex items-center justify-center p-6">
            <div className="w-full max-w-sm rounded-3xl border border-white/[0.08] bg-[#171717] shadow-2xl shadow-black/50 px-7 py-6">
              <div className="text-base font-semibold text-[#fafafa]">
                {confirmDialog.kind === "delete-song" ? "删除歌曲" : "删除文件夹"}
              </div>
              <div className="mt-2 text-sm text-[#8a8a94]">
                {confirmDialog.kind === "delete-song"
                  ? `确认删除「${confirmDialog.song.name}」？此操作会同时移除本地数据。`
                  : `确认删除文件夹「${confirmDialog.folderName}」？里面的歌曲会回到未分组。`}
              </div>
              <div className="mt-6 flex items-center justify-end gap-4">
                <button
                  type="button"
                  className="inline-flex min-w-[92px] items-center justify-center whitespace-nowrap rounded-full px-6 py-2.5 text-sm font-medium text-[#d4d4d8] hover:bg-white/[0.06]"
                  onClick={() => setConfirmDialog(null)}
                >
                  取消
                </button>
                <button
                  type="button"
                  className="inline-flex min-w-[92px] items-center justify-center whitespace-nowrap rounded-full bg-[#ef4444] px-6 py-2.5 text-sm font-semibold text-white hover:bg-[#dc2626]"
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
