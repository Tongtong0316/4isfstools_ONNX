# macOS Frozen Baseline

- Created: 2026-05-14
- Status: macOS is frozen and considered usable.
- Product name: Macaron Singer
- Project directory: 4isfstools
- Rust executable name: forisfstools
- Maintenance policy: do not change macOS behavior unless the user explicitly asks for macOS work.

## Frozen macOS Delivery

- Local tested artifact:
  - `/Users/suntong/Downloads/Macaron Singer_0.1.0_aarch64.dmg`
- Build cache artifact:
  - `/Users/suntong/Library/Caches/banzou-master/cargo-target/release/bundle/dmg/Macaron Singer_0.1.0_aarch64.dmg`
- Current macOS compatibility setting:
  - `src-tauri/tauri.conf.json` -> `bundle.macOS.minimumSystemVersion = "10.13"`

## Reason For Freeze

macOS is currently usable enough for short-term suspension of maintenance. Recent work has shown that Windows delivery, runtime deployment, and separation pipeline behavior need platform-specific iteration. From this point forward, unqualified development requests should be interpreted as Windows-first.

## Frozen File Fingerprints

- `src/App.tsx`: `e9671bd13fed009e2efe46c0ce61497d08d4c3471bc292f0b9a2ff687bb9b862`
- `src/components/lyrics/LyricsPanel.tsx`: `cafc4c870c0252e788e76dfb958f6bb932fb722021e545c7ae0b7b4a774be450`
- `src/components/VocalWaveformPreview.tsx`: `dddfc2daf14744e432e833f6f6c76a00765d58e8dd5dcc45371af590616c95ca`
- `src-tauri/src/lib.rs`: `60d8b0f915af48c6efdbdea7f5c1007ec0214b8ee40e8f4caf5ab44a3acd68a8`
- `src-tauri/src/process_control.rs`: `2a46a556917de01deedea149415edc8d86d584d96834c4a25ec8e464353b9b74`
- `runtime-manifest.json`: `7b0745884826322ad2eb4078dce445e52b75fff3b416e66cc4f8ca32cdef66b0`
- `package.json`: `44fc1ca22c0ff16f55ad0272370a77c1a2241b73fb23dcf5e84dd55276122613`
- `src-tauri/tauri.conf.json`: `1093786122f94b0114b648a8a52a751ce831410fc2998f269f039b354e6dc593`

## Guardrails

- Do not rebuild or repackage macOS as part of normal Windows iteration.
- Do not change shared runtime behavior for macOS while debugging Windows unless the change is explicitly platform-gated.
- If a future change must affect shared files such as `src-tauri/src/lib.rs`, record the platform impact in `PLATFORM_MAINTENANCE.md`.
- If macOS work resumes, create a new dated anchor instead of mutating this file.

