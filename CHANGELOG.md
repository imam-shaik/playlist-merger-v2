# Changelog

## v1.0.0 (2026-06-06)

- fix: resolve all pre-release test failures (5/5 verified)
- fix: incremental purge race condition
- fix: TempCleanup race condition
- fix: mutex poisoning in merge pipeline
- fix: timebase false-positive abort
- fix: probe failures propagate correctly
- fix: ffmpeg/ffprobe discovery and path escaping in concat list
- fix: block lossless merge on codec/resolution/audio mismatch
- fix: batchSetThumbnailPath type narrowing in playlistStore
- perf: merge startup optimization
- perf: blocking probe removal
- perf: thumbnail batching
- perf: probe cache persistence
- feat: virtualized playlist (1000+ files at 60fps)
- feat: drag & drop file/folder import with OS file manager
- feat: DnD reorder with @dnd-kit
- feat: inline rename (double-click or F2)
- feat: multi-select (Shift, Ctrl, Ctrl+A)
- feat: sort by filename, duration, size, resolution, fps, date
- feat: ffprobe metadata probe with async background probing
- feat: thumbnail cache in OS cache dir
- feat: compatibility checker (codec/resolution/fps/audio mismatches)
- feat: merge progress screen (percent, speed, ETA, bytes written, cancel)
- feat: playlist persistence with JSON autosave and crash recovery
- feat: split feature with companion SRT support
- feat: segment timestamp tracking and live timeline
- feat: merge report export
- feat: subtitle encoding detection (UTF-8, UTF-16, ANSI)
- feat: configured ffmpeg/ffprobe path resolution
- feat: subtitle parallelization
- feat: health check system
- feat: error translation layer (Rust → frontend)
- feat: dark mode UI with indigo/violet accent
- chore: remediate audit findings and align system types
- chore: clean up unused imports across test files
