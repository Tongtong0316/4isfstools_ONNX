# Daily Changelog 2026-05-21

## Commits

```
d66a445  fix:  reveal instrumental output folder; smooth onnx progress; replace emoji with svg
8b5274c  feat: add themes, badge polish, onnx improvements
f824de6  feat: add infinity/ironman/heavenly themes, remove glass, per-element stone cycling
```

## Changes

### Icon System
- Created `src/components/icons.tsx` with `icon()` factory + 12 SVG icon components
  (MicIcon, MusicNoteIcon, FolderIcon, SearchIcon, CheckIcon, SettingsIcon, etc.)
- Replaced all emoji across Player.tsx, Playlist.tsx, App.tsx, types/index.ts
- Added ThemeSwatch component with SVG `<polygon>` diagonal rendering

### Theme System
- **New themes**: 鬼花嫁 (red/black dark), 天使花嫁 (gold/white light), 零度天堂 (blue/white light)
- **Renames**: 零度天堂→梵高的星空, 风轻云淡→零度天堂
- **Reordered**: peach → aurora → 梵高的星空 → 津韵Double → 慵倦晚霞 → 天使花嫁 → 鬼花嫁 → 零度天堂 → graphite...
- Swatch rendering: CSS gradient → clean SVG polygon + clipPath (no aliasing)
- Light-theme group selectors: 零度天堂 (breeze) added to all groups

### Theme Card Badge ("已选择")
- Accent-following colors via `color-mix()` + `var(--accent)` (no hardcoded green)
- Positioned at bottom-right corner (`right-8 bottom-4`)
- Clean flex layout: centered, `gap-[8px]`, `whitespace-nowrap`, `leading-none`
- Dimensions: `h-8` (32px), `min-w-[92px]`, `px-[14px]`
- Subtle accent-colored box-shadow

### ONNX Engine
- Reduced `segment_size` from 512 to 256 for finer progress granularity
- Per-segment progress reporting: Python writes `separator_progress.json`, Rust monitors/emits
- Added `import torch` + MDXSTFT class with CoreML STFT support
- Added `coreml_provider_options()` with CPUAndGPU MLCompute units

### Theme Overhaul (f824de6)
- **New themes**: 无限 (infinity), I Am Iron Man, 奶油拿铁 (heavenly)
- **Removed**: 琉璃 (glass)
- **无限 (infinity)**: 6 MCU stone colors (purple/blue/red/orange/yellow/green) randomly assigned to 37 CSS var groups on each load via JS `applyInfinityColors()` / `clearInfinityColors()`
- **奶油拿铁 (heavenly)**: Warm cream `#F2E6D5` gradient bg, coffee brown `#8B6B4F` accent, caramel `#C9A87C` secondary. `backdrop-filter: blur(20px)` on panels. Replaces former dark 天上宫阙.
- **Stone color cycling**: Playlist songs, lyrics lines, and transport/mode/settings buttons get per-element stone color via `--accent` override and nth-child selectors
- **accent2 field**: Added to COLOR_THEMES schema for dual-accent preview rendering
- **ThemeSwatch**: Supports `imageUrl` prop for custom PNG preview (infinity uses `Infinity.png`)
- **Readability**: Light theme text contrast boosted (primary 0.95, secondary 0.80, muted 0.60), borders and surfaces more distinct
- **Light theme group**: 奶油拿铁 added to all light-theme selectors (context-menu, status-badge, theme-aware-surface, theme-subtle-surface)

### Reveal Folder
- `reveal_song_folder` now opens instrumental/ output directory
- Context menu: "在访达中打开伴奏位置"

### Misc
- Manifesto description changed to EVA quote: 「逃げちゃダメだ　逃げちゃダメだ　逃げちゃダメだ」
- Removed unused imports across multiple files
