import React, { useState, useEffect, useRef } from 'react';
import { motion } from 'framer-motion';
import { cn } from '@/utils/cn';
import { PlaylistToolbar } from '@/components/playlist/PlaylistToolbar';
import { PlaylistList } from '@/components/playlist/PlaylistList';
import { HomeScreen } from '@/features/home/HomeScreen';
import { useDropZone } from '@/hooks/useDropZone';
import { useFileImport } from '@/hooks/useFileImport';
import { usePlaylistKeyboardShortcuts } from '@/hooks/useKeyboardShortcuts';
import { usePlaylistStore } from '@/store/playlistStore';
import { useAppStore } from '@/store/appStore';
import { AnimatePresence } from 'framer-motion';
import { MergeConfigModal } from './MergeConfigModal';
import { tauriCommands, tauriEvents } from '@/tauri/commands';

export function PlaylistScreen() {
  // ── Selective store subscriptions ─────────────────────────────────────
  // Subscribe to individual slices instead of the full store to prevent
  // unnecessary re-renders when unrelated properties (selectedIds, sort, etc.) change.
  const entries = usePlaylistStore((s) => s.entries);
  const scanningFiles = usePlaylistStore((s) => s.scanningFiles);

  const { pickAndAddFiles, handleNativeDrop } = useFileImport();
  const [isMergeConfigOpen, setIsMergeConfigOpen] = useState(false);

  usePlaylistKeyboardShortcuts();

  // ── Filesystem watcher ──────────────────────────────────────────────
  // Start watching all unique parent directories when entries change.
  // This enables auto-detection when subtitle files are added next to videos.
  useEffect(() => {
    const entries = usePlaylistStore.getState().entries;
    if (entries.length === 0) return;

    // Collect unique parent directories
    const dirs = new Set<string>();
    entries.forEach(e => {
      try {
        const path = e.path.replace(/\\/g, '/');
        const lastSlash = path.lastIndexOf('/');
        if (lastSlash > 0) {
          dirs.add(path.substring(0, lastSlash));
        }
      } catch { /* non-critical */ }
    });

    const dirArray = Array.from(dirs);
    if (dirArray.length > 0) {
      tauriCommands.watchDirectories(dirArray).catch(err => {
        console.warn('[FsWatcher] Failed to start watcher:', err);
      });
    }

    // Cleanup: stop watcher on unmount
    return () => {
      tauriCommands.stopWatchingDirs().catch(() => {});
    };
  }, [entries.length]);

  // ── Fs change event listener ────────────────────────────────────────
  // When the watcher detects new/modified video/subtitle files, auto-refresh.
  // Uses a debounce ref so rapid successive changes only trigger one refresh.
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | null = null;

    const handleFsChange = (event: import('@/tauri/commands').FsChangeEvent) => {
      // Check if the change involves subtitle files
      const hasSubChange = event.paths.some(p => {
        const ext = p.split('.').pop()?.toLowerCase();
        return ext === 'srt' || ext === 'vtt' || ext === 'ass' || ext === 'ssa' || ext === 'sub';
      });

      if (hasSubChange) {
        // Debounce: cancel any pending refresh and schedule a new one after 2s
        if (debounceRef.current) {
          clearTimeout(debounceRef.current);
        }
        debounceRef.current = setTimeout(() => {
          useAppStore.getState().refreshMediaMetadata();
          debounceRef.current = null;
        }, 2000);
      }
    };

    const setup = async () => {
      try {
        unlisten = await tauriEvents.onFsChange(handleFsChange);
      } catch (err) {
        console.warn('[FsWatcher] Failed to set up fs-change listener:', err);
      }
    };

    setup();

    return () => {
      // Clean up listener and any pending debounce
      if (unlisten) unlisten();
      if (debounceRef.current) {
        clearTimeout(debounceRef.current);
        debounceRef.current = null;
      }
    };
  }, []);

  // ── Drop zone ─────────────────────────────────
  const { isDragOver, dropHandlers } = useDropZone({
    onDropFiles: handleNativeDrop,
  });

  const hasEntries = entries.length > 0;

  if (!hasEntries && scanningFiles) {
    return (
      <div className="flex flex-col h-full">
        <div className="flex-1 flex flex-col items-center justify-center gap-3">
          <div className="w-8 h-8 rounded-full border-2 border-accent-500/30 border-t-accent-400 animate-spin" />
          <p className="text-xs text-text-muted font-medium">Scanning folder for video files…</p>
          <p className="text-2xs text-text-muted/60">Please wait while the directory is being indexed</p>
        </div>
      </div>
    );
  }

  if (!hasEntries) {
    return <HomeScreen />;
  }

  return (
    <div
      className={cn(
        'flex flex-col h-full relative',
        'transition-all duration-200',
      )}
      {...dropHandlers}
    >
      <div className="flex-1 flex flex-col min-h-0">
        <PlaylistToolbar onMergeClick={() => setIsMergeConfigOpen(true)} />
        <div className="flex-1 flex flex-col min-h-0">
          <PlaylistList onAddFiles={pickAndAddFiles} />
        </div>
      </div>

      {/* Merge config modal */}
      <AnimatePresence>
        {isMergeConfigOpen && (
          <MergeConfigModal
            isOpen={isMergeConfigOpen}
            onClose={() => setIsMergeConfigOpen(false)}
          />
        )}
      </AnimatePresence>

      {/* Drag overlay */}
      <AnimatePresence>
        {isDragOver && (
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.15 }}
            className="absolute inset-0 z-20 pointer-events-none flex items-center justify-center"
          >
            <div className="absolute inset-0 bg-accent-500/5 border-2 border-dashed border-accent-500/50 rounded-lg m-2" />
            <div className="relative z-10 bg-bg-elevated/90 backdrop-blur-md rounded-xl px-6 py-4 border border-accent-500/30 shadow-glow">
              <p className="text-sm font-semibold text-accent-300">Drop to add videos</p>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
