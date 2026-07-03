import { useEffect, useRef } from 'react';
import { usePlaylistStore } from '@/store/playlistStore';
import { tauriCommands } from '@/tauri/commands';
import { AUTOSAVE_DEBOUNCE_MS } from '@/constants';

export function useAutosave(enabled: boolean) {
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isSavingRef = useRef(false);
  // Epoch counter: incremented on every state change.
  // We snapshot the epoch before saving, and compare after — if mismatched,
  // then state was modified during the save and we must NOT clear isDirty.
  const saveEpochRef = useRef(0);

  useEffect(() => {
    if (!enabled) return;

    // Subscribe to state changes using Zustand's subscribe
    const unsubscribe = usePlaylistStore.subscribe((state) => {
      if (!state.isDirty || state.entries.length === 0) return;

      saveEpochRef.current++;

      if (timerRef.current) clearTimeout(timerRef.current);

      timerRef.current = setTimeout(async () => {
        if (isSavingRef.current) return; // prevent concurrent saves
        isSavingRef.current = true;

        // Snapshot the epoch BEFORE reading state — this is the version we're saving
        const readEpoch = saveEpochRef.current;

        try {
          // Read fresh state at save time — avoids stale closure
          const current = usePlaylistStore.getState();
          if (!current.isDirty) return;

          const data = {
            id: current.playlistId,
            name: current.playlistName,
            entries: current.entries.map((e) => ({
              id: e.id,
              name: e.name,
              path: e.path,
              extension: e.extension,
              size: e.size,
              modified: e.modified,
              created: e.created,
              parentFolder: e.parentFolder,
              relativePath: e.relativePath,
            })),
            folders: current.folders.map((f) => ({
              id: f.id,
              path: f.path,
              name: f.name,
              order: f.order,
              isCollapsed: f.isCollapsed,
            })),
            createdAt: new Date().toISOString(),
            updatedAt: new Date().toISOString(),
            totalDuration: current.getTotalDuration(),
          };

          await tauriCommands.savePlaylist(current.playlistId, current.playlistName, data);

          // TOCTOU protection: only mark clean if no changes happened DURING the save.
          // If saveEpoch changed, a state mutation occurred — meaning isDirty was
          // already reset to true by the subscriber, and a new save is queued.
          if (readEpoch === saveEpochRef.current) {
            usePlaylistStore.getState().setDirty(false);
          }
        } catch (err) {
          console.error('Autosave failed:', err);
        } finally {
          isSavingRef.current = false;
        }
      }, AUTOSAVE_DEBOUNCE_MS);
    });

    return () => {
      unsubscribe();
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [enabled]);
}
