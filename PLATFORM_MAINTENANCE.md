# Platform Maintenance Policy

## Current Default

As of 2026-05-14, macOS is frozen and Windows is the default development target.

When a request does not explicitly name macOS, treat it as Windows work. macOS should not be rebuilt, retested, or modified unless the user specifically asks for macOS maintenance.

## Platform Status

- macOS:
  - Status: frozen usable baseline
  - Anchor: `MACOS_FROZEN_2026-05-14.md`
  - Short-term maintenance: paused
- Windows:
  - Status: active development and delivery target
  - Current focus: runtime deployment, separation pipeline, portable/installable delivery, and Windows-specific regression checks
  - Handoff: `WINDOWS_AGENT_HANDOFF.md`
  - Fix log: `WINDOWS_FIX_LOG.md`

## Shared-Code Rules

Shared files still exist. The most important ones are:

- `src/App.tsx`
- `src/components/lyrics/LyricsPanel.tsx`
- `src/components/VocalWaveformPreview.tsx`
- `src-tauri/src/lib.rs`
- `src-tauri/src/process_control.rs`
- `runtime-manifest.json`

Before changing shared files for Windows, check whether the behavior is platform-specific. Prefer explicit platform branching over changing macOS behavior indirectly.

## Windows-First Checklist

1. Confirm the change is needed for Windows.
2. Keep macOS artifacts untouched unless macOS is named in the task.
3. If editing shared Rust runtime code, verify Windows source and task-generated scripts both reflect the change.
4. For separation pipeline changes, inspect the generated `separator.py` in the fresh Windows `song_*` directory, not only `src-tauri/src/lib.rs`.
5. Record Windows-specific fixes in `WINDOWS_FIX_LOG.md`.
6. Keep `MACOS_FROZEN_2026-05-14.md` unchanged unless creating a new macOS anchor.

## Known Trap

The Windows 20% separation issue can appear to regress when an old `song_*` task directory still contains an old generated `separator.py`. Always validate a fresh task directory before concluding that the new executable has failed.

