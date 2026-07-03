// ────────────────────────────────────────────────
// useFileImport — Centralized File Import Workflow
//
// Single source of truth for adding files to the
// playlist. Eliminates duplicate logic across:
//   - HomeScreen
//   - PlaylistToolbar
//   - PlaylistScreen
//   - MergePanel (direct merge flows)
//
// Handles: scanning, adding, probing, thumbnails.
// ────────────────────────────────────────────────

import { useCallback, useRef } from 'react';
import { usePlaylistStore } from '@/store/playlistStore';
import { useProbe, useThumbnails } from './useProbe';
import { tauriCommands, openVideoFilesDialog, openFolderDialog } from '@/tauri/commands';
import { useFolderSelectStore } from '@/store/folderSelectStore';
import { SUPPORTED_VIDEO_EXTENSIONS } from '@/constants';
import type { ScannedFile } from '@/types';

/** Result of a file import operation */
export interface FileImportResult {
  addedCount: number;
  newEntryIds: string[];
  /** Files that were rejected because they already exist in the playlist */
  deduplicatedCount: number;
}

export function useFileImport() {
  const store = usePlaylistStore();
  const { probeEntries } = useProbe();
  const { generateThumbnails } = useThumbnails();
  const isPickingRef = useRef(false);

  /** Derive playlist name from import source path. Only overwrites default or when playlist is empty. */
  const maybeSetPlaylistName = useCallback((sourcePath: string) => {
    const state = usePlaylistStore.getState();
    const current = state.playlistName;
    const hasEntries = state.entries.length > 0;
    // Only set if playlist is empty (fresh import after clear) or still default name
    if (hasEntries && current !== 'New Playlist' && current.trim() !== '') return;
    // Extract the last non-empty path segment as the folder/file name
    const parts = sourcePath.replace(/\\/g, '/').split('/').filter(Boolean);
    const name = parts[parts.length - 1] ?? sourcePath;
    if (name) usePlaylistStore.getState().setPlaylistName(name);
  }, []);

  /** Add pre-scanned files, trigger probe + thumbnails */
  const addScannedFiles = useCallback(
    (files: ScannedFile[]): FileImportResult => {
      if (files.length === 0) return { addedCount: 0, newEntryIds: [], deduplicatedCount: 0 };

      const prevCount = usePlaylistStore.getState().entries.length;
      store.addFiles(files);
      const newCount = usePlaylistStore.getState().entries.length;
      const addedCount = newCount - prevCount;
      const deduplicatedCount = files.length - addedCount;

      console.log(
        `[SELECT] Scanned: ${files.length} files | Added: ${addedCount} | Deduplicated: ${deduplicatedCount}`
      );

      const currentEntries = usePlaylistStore.getState().entries;
      const newEntryIds = currentEntries
        .filter((e) => files.some((f) => f.path === e.path) && !e.mediaInfo)
        .map((e) => e.id);

      if (newEntryIds.length > 0) {
        console.log(`[SELECT] Probe queued: ${newEntryIds.length} files`);
        probeEntries(newEntryIds);
        // Defer thumbnail generation by 500ms to let probes start first.
        // Starting 6 ffprobe + 3 ffmpeg processes simultaneously overwhelms
        // disk I/O and causes system-wide slowdown on large imports.
        setTimeout(() => generateThumbnails(newEntryIds), 500);
      }

      return { addedCount, newEntryIds, deduplicatedCount };
    },
    [store, probeEntries, generateThumbnails]
  );

  /** Convert paths to ScannedFile format and add */
  const addFilesFromPaths = useCallback(
    (paths: string[]): FileImportResult => {
      if (paths.length === 0) return { addedCount: 0, newEntryIds: [], deduplicatedCount: 0 };

      const files: ScannedFile[] = paths.map((p) => {
        const parts = p.replace(/\\/g, '/').split('/');
        const filename = parts.pop() ?? p;
        const parent = parts.pop() ?? undefined;
        return {
          path: p,
          name: filename,
          size: 0,
          extension: p.split('.').pop()?.toLowerCase() ?? '',
          parentFolder: parent,
        };
      });

      // Derive playlist name from common parent of selected files
      if (paths.length > 0) {
        const parents = paths.map((p) => {
          const parts = p.replace(/\\/g, '/').split('/');
          parts.pop(); // remove filename
          return parts.join('/');
        });
        // Find the longest common prefix
        let common = parents[0];
        for (const p of parents.slice(1)) {
          while (!p.startsWith(common)) {
            common = common.slice(0, common.lastIndexOf('/'));
          }
        }
        if (common) maybeSetPlaylistName(common + '/x');
      }

      return addScannedFiles(files);
    },
    [addScannedFiles, maybeSetPlaylistName]
  );

  /** Open native file picker dialog and import selected files */
  const pickAndAddFiles = useCallback(async (): Promise<FileImportResult> => {
    if (isPickingRef.current) return { addedCount: 0, newEntryIds: [], deduplicatedCount: 0 };
    isPickingRef.current = true;
    try {
      const paths = await openVideoFilesDialog();
      return addFilesFromPaths(paths);
    } catch (err) {
      console.error('File picker failed:', err);
      return { addedCount: 0, newEntryIds: [], deduplicatedCount: 0 };
    } finally {
      isPickingRef.current = false;
    }
  }, [addFilesFromPaths]);

  /** Open native folder picker, scan recursively, and import */
  const pickAndAddFolder = useCallback(async (): Promise<FileImportResult> => {
    if (isPickingRef.current) return { addedCount: 0, newEntryIds: [], deduplicatedCount: 0 };
    isPickingRef.current = true;
    try {
      const dir = await openFolderDialog();
      if (!dir) return { addedCount: 0, newEntryIds: [], deduplicatedCount: 0 };
      maybeSetPlaylistName(dir);

      const subfolders = await tauriCommands.scanSubfolders(dir);
      console.log('[FOLDER_AUDIT] scanSubfolders() returned:', subfolders.length, 'folders:', subfolders.map(f => ({ path: f.path, name: f.name })));

      let pathsToScan: string[] = [dir];

      if (subfolders.length > 1) {
        console.log('[FOLDER_AUDIT] OPEN_FOLDER_SELECT_CALLED - subfolders.length:', subfolders.length);
        const selectedPaths = await useFolderSelectStore.getState().openFolderSelect(dir, subfolders);
        if (selectedPaths === null || selectedPaths.length === 0) {
          return { addedCount: 0, newEntryIds: [], deduplicatedCount: 0 };
        }
        pathsToScan = selectedPaths;
      } else if (subfolders.length === 1) {
        pathsToScan = [subfolders[0].path];
      }

      usePlaylistStore.getState().setScanningFiles(true);

      try {
        const allFiles: ScannedFile[] = [];
        for (const path of pathsToScan) {
          const scanned = await tauriCommands.scanDirectory(path, {
            recursive: true,
            sortBy: 'name',
          });
          allFiles.push(...scanned);
        }
        return addScannedFiles(allFiles);
      } catch (err) {
        console.error('Folder scan failed:', err);
        return { addedCount: 0, newEntryIds: [], deduplicatedCount: 0 };
      } finally {
        usePlaylistStore.getState().setScanningFiles(false);
      }
    } catch (err) {
      console.error('Folder picker failed:', err);
      return { addedCount: 0, newEntryIds: [], deduplicatedCount: 0 };
    } finally {
      isPickingRef.current = false;
    }
  }, [addScannedFiles, maybeSetPlaylistName]);

  /** Handle native file drop event (paths from OS) */
  const handleNativeDrop = useCallback(
    async (paths: string[]): Promise<FileImportResult> => {
      const videoFiles: ScannedFile[] = [];
      const dirPaths: string[] = [];

      for (const p of paths) {
        const ext = p.split('.').pop()?.toLowerCase() ?? '';
        if (SUPPORTED_VIDEO_EXTENSIONS.includes(ext as typeof SUPPORTED_VIDEO_EXTENSIONS[number])) {
          const parts = p.replace(/\\/g, '/').split('/');
          const filename = parts.pop() ?? p;
          const parent = parts.pop() ?? undefined;
          videoFiles.push({
            path: p,
            name: filename,
            size: 0,
            extension: ext,
            parentFolder: parent,
          });
        } else if (!p.includes('.') || p.endsWith('\\') || p.endsWith('/')) {
          dirPaths.push(p);
        }
      }

      if (dirPaths.length > 0) {
        usePlaylistStore.getState().setScanningFiles(true);
      }

      // Scan dropped directories
      for (const dir of dirPaths) {
        try {
          const scanned = await tauriCommands.scanDirectory(dir, {
            recursive: true,
            sortBy: 'name',
          });
          videoFiles.push(...scanned);
        } catch (err) {
          console.error(`Failed to scan directory ${dir}:`, err);
        }
      }

      // Derive playlist name from first dropped source
      if (dirPaths.length > 0) {
        maybeSetPlaylistName(dirPaths[0]);
      } else if (videoFiles.length > 0) {
        maybeSetPlaylistName(videoFiles[0].path);
      }

      if (dirPaths.length > 0) {
        usePlaylistStore.getState().setScanningFiles(false);
      }

      return addScannedFiles(videoFiles);
    },
    [addScannedFiles, maybeSetPlaylistName]
  );

  return {
    addScannedFiles,
    addFilesFromPaths,
    pickAndAddFiles,
    pickAndAddFolder,
    handleNativeDrop,
  };
}
