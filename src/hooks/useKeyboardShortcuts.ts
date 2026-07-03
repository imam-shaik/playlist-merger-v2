import { useEffect } from 'react';
import { usePlaylistStore } from '@/store/playlistStore';
import { useAppStore } from '@/store/appStore';
import { useScreenshotStore } from '@/store/screenshotStore';
import { tauriCommands } from '@/tauri/commands';
import { generateScreenshotId } from '@/store/screenshotStore';

export function useGlobalKeyboardShortcuts() {
  const screen = useAppStore((s) => s.screen);
  const setScreen = useAppStore((s) => s.setScreen);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      // Ignore when typing in inputs
      const target = e.target as HTMLElement;
      if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable) return;

      if (e.key === 'Escape') {
        if (screen === 'settings') {
          setScreen('playlist');
        }
      }
    };

    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [screen, setScreen]);
}

export function usePlaylistKeyboardShortcuts() {
  const selectedIds = usePlaylistStore((s) => s.selectedIds);
  const focusedId = usePlaylistStore((s) => s.focusedId);
  const selectAll = usePlaylistStore((s) => s.selectAll);
  const clearSelection = usePlaylistStore((s) => s.clearSelection);
  const removeEntries = usePlaylistStore((s) => s.removeEntries);
  const duplicateEntry = usePlaylistStore((s) => s.duplicateEntry);
  const selectEntry = usePlaylistStore((s) => s.selectEntry);
  const setFocused = usePlaylistStore((s) => s.setFocused);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable) return;

      const meta = e.ctrlKey || e.metaKey;

      // Ctrl+A — select all
      if (meta && e.key === 'a') {
        e.preventDefault();
        selectAll();
        return;
      }

      // Escape — clear selection
      if (e.key === 'Escape') {
        clearSelection();
        return;
      }

      // Delete / Backspace — remove selected
      if (e.key === 'Delete' || e.key === 'Backspace') {
        const selected = [...selectedIds];
        if (selected.length > 0) {
          removeEntries(selected);
        }
        return;
      }

      // Ctrl+D — duplicate focused
      if (meta && e.key === 'd') {
        e.preventDefault();
        if (focusedId) {
          duplicateEntry(focusedId);
        }
        return;
      }

      // S — capture screenshot frame from the focused/selected video
      // If no file is selected, toggle the screenshot panel open/closed
      if (e.key === 's' && !meta && !e.ctrlKey && !e.altKey) {
        e.preventDefault();
        const st = usePlaylistStore.getState();
        const targetId = focusedId || [...st.selectedIds][0];
        if (targetId) {
          const entry = st.entries.find((en) => en.id === targetId);
          if (entry && entry.mediaInfo) {
            const timestamp = (entry.mediaInfo.duration ?? 0) / 2;
            useScreenshotStore.getState().setPanelOpen(true);
            tauriCommands
              .captureFrame(entry.path, timestamp)
              .then((imagePath) => {
                useScreenshotStore.getState().addScreenshot({
                  id: generateScreenshotId(),
                  imagePath,
                  sourcePath: entry.path,
                  sourceName: entry.name,
                  timestamp,
                  capturedAt: new Date().toISOString(),
                  notes: '',
                });
                useAppStore.getState().showToast({
                  type: 'success',
                  title: 'Screenshot captured',
                  description: `${entry.name} @ ${Math.round(timestamp)}s`,
                });
              })
              .catch((err) => {
                console.error('Screenshot capture failed:', err);
                useAppStore.getState().showToast({
                  type: 'error',
                  title: 'Screenshot failed',
                  description: String(err),
                });
              });
          }
        } else {
          // No file focused/selected — toggle the panel
          useScreenshotStore.getState().togglePanel();
        }
        return;
      }

      // Arrow up/down — navigate focused
      if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
        e.preventDefault();
        const visible = usePlaylistStore.getState().getVisibleEntries();
        if (visible.length === 0) return;

        const currentIdx = visible.findIndex(e => e.id === focusedId);
        const delta = e.key === 'ArrowUp' ? -1 : 1;
        const nextIdx = Math.max(0, Math.min(visible.length - 1, currentIdx + delta));
        const nextId = visible[nextIdx].id;

        if (e.shiftKey) {
          selectEntry(nextId, 'range');
        } else {
          selectEntry(nextId, 'single');
        }
        setFocused(nextId);
        return;
      }
    };

    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [selectedIds, focusedId, selectAll, clearSelection, removeEntries, duplicateEntry, selectEntry, setFocused]);
}
