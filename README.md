# PlaylistMerger

> Ultra-fast playlist-based video merger — lossless, beautiful, minimal.

A production-grade Tauri desktop app for merging multiple videos into one with zero quality loss. Built for creators who manage large video collections.

---

## Features

| Feature | Detail |
|---|---|
| **Lossless merge** | `ffmpeg -f concat -safe 0 -i list.txt -c copy output.mp4` — no re-encoding |
| **Smart merge** | Re-encode to a common format when files are incompatible |
| **Virtualized playlist** | Handles 1000+ files at 60fps using `@tanstack/react-virtual` |
| **Drag & drop** | Drop files or entire folders from your OS file manager |
| **DnD reorder** | Drag single or multiple selected items with `@dnd-kit` |
| **Inline rename** | Double-click any item, or press F2 |
| **Multi-select** | Shift-click ranges, Ctrl/Cmd toggle, Ctrl+A all |
| **Sort options** | Filename, duration, size, resolution, fps, date modified/created, manual |
| **FFprobe metadata** | Per-file codec, resolution, fps, audio info — async background probing |
| **Thumbnail cache** | Async thumbnail generation stored in OS cache dir |
| **Compatibility check** | Detects codec/resolution/fps/audio mismatches before merge |
| **Progress screen** | Real-time percent, speed, ETA, bytes written — cancel anytime |
| **Playlist persistence** | JSON autosave with debounce, crash recovery, session restore |
| **Native save dialog** | OS-native file picker for output path |
| **Dark mode first** | Premium dark UI with indigo/violet accent |

---

## Tech Stack

```
Frontend            Backend
─────────────────   ──────────────────
React 18            Rust (Tauri 2)
TypeScript          tokio async runtime
Tailwind CSS        serde_json
Framer Motion       walkdir (dir scan)
@dnd-kit/sortable   uuid
@tanstack/virtual   chrono
Zustand             env_logger / log
Lucide icons        dirs (paths)
Vite 6              anyhow / thiserror
```

---

## Project Structure

```
playlist-merger/
├── src/                         # React frontend
│   ├── app/
│   │   └── App.tsx              # Root layout + screen router
│   ├── features/
│   │   ├── home/HomeScreen.tsx  # Drop zone + recent exports
│   │   ├── playlist/            # Playlist management screen
│   │   ├── merge/MergePanel.tsx # Merge config + compat check
│   │   ├── progress/            # Real-time merge progress
│   │   └── settings/            # FFmpeg paths + behavior
│   ├── components/
│   │   ├── playlist/
│   │   │   ├── PlaylistItem.tsx  # Single virtualized row
│   │   │   ├── PlaylistList.tsx  # DnD sortable virtual list
│   │   │   ├── PlaylistToolbar.tsx
│   │   │   └── ContextMenu.tsx
│   │   └── ui/
│   │       ├── Button.tsx / Input.tsx / Badge.tsx
│   │       ├── Toast.tsx
│   │       └── Sidebar.tsx
│   ├── hooks/
│   │   ├── useProbe.ts          # Background ffprobe with concurrency
│   │   ├── useThumbnails.ts     # Thumbnail generation queue
│   │   ├── useMerge.ts          # Merge orchestration + event listeners
│   │   ├── useDropZone.ts       # OS drag-drop file intake
│   │   ├── useAutosave.ts       # Debounced playlist JSON save
│   │   └── useKeyboardShortcuts.ts
│   ├── store/
│   │   ├── playlistStore.ts     # Zustand — all playlist state
│   │   ├── mergeStore.ts        # Zustand — merge job + config
│   │   └── appStore.ts          # Zustand — screen, settings, toast
│   ├── tauri/
│   │   └── commands.ts          # Typed IPC wrappers + dialog helpers
│   ├── types/index.ts           # All TypeScript types
│   ├── constants/index.ts       # No magic numbers — all constants here
│   └── utils/index.ts           # Pure helpers: format, sanitize, paths
│
└── src-tauri/                   # Rust backend
    └── src/
        ├── lib.rs               # Tauri builder + state setup
        ├── types.rs             # Shared Rust types (serde)
        ├── ffmpeg/
        │   ├── mod.rs           # Binary discovery + concat list writer
        │   ├── probe.rs         # ffprobe parsing + compat check
        │   ├── concat.rs        # Merge execution + arg builder
        │   └── progress.rs      # FFmpeg stderr progress parser
        ├── commands/
        │   ├── fs.rs            # scan_directory, disk_space, open
        │   ├── media.rs         # probe_video, batch_probe, thumbnails
        │   ├── merge.rs         # start_merge, cancel_merge, status
        │   ├── playlist.rs      # save/load/list/delete playlists
        │   └── settings.rs      # get/save settings, ffmpeg paths
        └── services/
            └── settings.rs      # Settings load helper
```

---

## Prerequisites

### Required

- [Node.js 18+](https://nodejs.org)
- [Rust 1.75+](https://rustup.rs)
- [Tauri CLI v2](https://tauri.app/start/create-project/)
- **FFmpeg** — must be on system PATH, or configure a custom path in Settings

### Install Tauri CLI

```bash
cargo install tauri-cli --version "^2"
# or
npm install -g @tauri-apps/cli
```

### Install FFmpeg

**Windows:**
```powershell
winget install Gyan.FFmpeg
# or download from https://ffmpeg.org/download.html
```

**macOS:**
```bash
brew install ffmpeg
```

**Linux:**
```bash
sudo apt install ffmpeg        # Debian/Ubuntu
sudo dnf install ffmpeg        # Fedora
```

---

## Development

```bash
# Install frontend dependencies
npm install

# Start dev server (Vite + Tauri)
npm run tauri:dev
```

The app launches with hot-reload. Rust code recompiles automatically when you save.

---

## Production Build

```bash
# Build installer for current platform
npm run tauri:build
```

Outputs:
- **Windows:** `src-tauri/target/release/bundle/msi/PlaylistMerger_*.msi` (installer)
- **Windows:** `src-tauri/target/release/bundle/nsis/PlaylistMerger_*.exe` (NSIS installer)
- **macOS:** `src-tauri/target/release/bundle/dmg/PlaylistMerger_*.dmg`
- **Linux:** `.deb`, `.AppImage`

---

## FFmpeg Bundling (Recommended for Distribution)

Bundle FFmpeg binaries so users don't need to install separately:

1. Download static FFmpeg builds from https://ffmpeg.org/download.html
2. Place `ffmpeg.exe` and `ffprobe.exe` (Windows) in `src-tauri/binaries/`
3. The Rust code checks `./binaries/` before falling back to system PATH

```
src-tauri/
└── binaries/
    ├── ffmpeg.exe
    └── ffprobe.exe
```

---

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+A` | Select all |
| `Shift+Click` | Range select |
| `Ctrl+Click` | Toggle select |
| `↑` / `↓` | Navigate items |
| `Shift+↑/↓` | Extend selection |
| `Delete` | Remove selected |
| `Ctrl+D` | Duplicate focused item |
| `Double-click` | Rename inline |
| `Escape` | Clear selection / close modal |

---

## Merge Pipeline

```
User clicks Merge
    ↓
Native OS save dialog (tauri-plugin-dialog)
    ↓
Write concat list to temp file
    │  /tmp/playlist_merger/concat_<id>.txt
    │  file '/path/to/video1.mp4'
    │  file '/path/to/video2.mp4'
    ↓
Execute FFmpeg
    │  Lossless:  ffmpeg -f concat -safe 0 -i list.txt -c copy output.mp4
    │  Smart:     ffmpeg -f concat -safe 0 -i list.txt -c:v libx264 -crf 18 -c:a aac output.mp4
    ↓
Stream FFmpeg stderr → parse progress lines → emit Tauri events
    ↓
Frontend receives merge-progress events → update UI
    ↓
FFmpeg exits 0 → emit merge-complete event
    ↓
Cleanup temp files
    ↓
Success screen shown
```

---

## Architecture Decisions

### Why Tauri 2 (not Electron)?
- **~10× smaller binary** (10-20MB vs 100-200MB)
- Native OS system dialogs, file pickers, notifications
- Rust backend for FFmpeg process management — safe, predictable
- Lower RAM usage — no bundled Chromium

### Why Zustand over Redux?
- Zero boilerplate for this scale
- `subscribeWithSelector` enables efficient autosave
- Flat stores map cleanly to features

### Why Virtual List?
- 1000 items × 72px = 72000px DOM nodes would kill performance
- `@tanstack/react-virtual` renders only visible rows
- Combined with `@dnd-kit/sortable` via `SortableContext`

### Why no re-encoding by default?
- Stream copy is 20-100× faster than transcoding
- Zero quality loss — exact bit-for-bit copy of streams
- Only use Smart mode when files are incompatible

---

## Extending

### Add a new Tauri command

**Rust** (`src-tauri/src/commands/your_module.rs`):
```rust
#[tauri::command]
pub async fn your_command(arg: String) -> Result<String, String> {
    Ok(format!("Result: {}", arg))
}
```

Register in `lib.rs`:
```rust
.invoke_handler(tauri::generate_handler![
    commands::your_module::your_command,
    // ...
])
```

**TypeScript** (`src/tauri/commands.ts`):
```typescript
yourCommand: (arg: string): Promise<string> =>
  invoke('your_command', { arg }),
```

### Add a sort option

In `src/constants/index.ts`, add to `SORT_OPTIONS` (frontend toolbar already reads from this array).
In `src/store/playlistStore.ts`, add the case to `applySortToEntries`.

---

## License

MIT
