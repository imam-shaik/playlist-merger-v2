import { create } from 'zustand';
import type { AppScreen, AppSettings, FfmpegPaths } from '@/types';
import { generateId } from '@/utils';

interface AppState {
  screen: AppScreen;
  settings: AppSettings | null;
  ffmpegPaths: FfmpegPaths | null;
  isLoadingSettings: boolean;
  ffmpegMissing: boolean;
  toasts: ToastMessage[];

  setScreen: (screen: AppScreen) => void;
  setSettings: (settings: AppSettings) => void;
  setFfmpegPaths: (paths: FfmpegPaths) => void;
  setFfmpegMissing: (missing: boolean) => void;
  showToast: (message: Omit<ToastMessage, 'id'> & { id?: string }) => void;
  dismissToast: (id: string) => void;
  refreshMediaMetadata: () => Promise<void>;
}

export interface ToastMessage {
  id: string;
  type: 'success' | 'error' | 'warning' | 'info';
  title: string;
  description?: string;
  technicalDetails?: string;
  durationMs?: number;
}

const MAX_TOASTS = 5;

export const useAppStore = create<AppState>()((set) => ({
  screen: 'playlist',
  settings: null,
  ffmpegPaths: null,
  isLoadingSettings: true,
  ffmpegMissing: false,
  toasts: [],

  setScreen: (screen) => set({ screen }),
  setSettings: (settings) => set({ settings, isLoadingSettings: false }),
  setFfmpegPaths: (paths) => set({ ffmpegPaths: paths }),
  setFfmpegMissing: (missing) => set({ ffmpegMissing: missing }),

  refreshMediaMetadata: async () => {
    try {
      const { tauriCommands } = await import('@/tauri/commands');
      const { usePlaylistStore } = await import('@/store/playlistStore');
      const ps = usePlaylistStore.getState();

      if (ps.entries.length === 0) return;

      const paths = ps.entries.map(e => e.path);
      const results = await tauriCommands.batchProbe(paths);

      let updatedCount = 0;
      results.forEach((res, idx) => {
        if (typeof res !== 'string') {
          const entry = ps.entries[idx];
          if (entry) {
            ps.setMediaInfo(entry.id, res);
            updatedCount++;
          }
        }
      });

      // Show a toast notification
      if (updatedCount > 0) {
        try {
          const { useAppStore } = await import('@/store/appStore');
          useAppStore.getState().showToast({
            type: 'success',
            title: 'Metadata refreshed',
            description: `${updatedCount} files re-scanned for subtitles and metadata`,
            durationMs: 3000,
          });
        } catch { /* non-critical */ }
      }
    } catch (err) {
      console.error('Metadata refresh failed:', err);
    }
  },

  showToast: (message) =>
    set((state) => {
      const newToast: ToastMessage = { ...message, id: message.id ?? generateId() };
      const next = [newToast, ...state.toasts].slice(0, MAX_TOASTS);
      return { toasts: next };
    }),

  dismissToast: (id) =>
    set((state) => ({
      toasts: state.toasts.filter((t) => t.id !== id),
    })),
}));
