// ────────────────────────────────────────────────
// ScreenshotStore — Manages captured screenshots
// and notes in the side panel.
// ────────────────────────────────────────────────

import { create } from 'zustand';
import type { Screenshot } from '@/types';

interface ScreenshotState {
  /** All captured screenshots, newest first */
  screenshots: Screenshot[];
  /** Whether the side panel is visible */
  panelOpen: boolean;
  /** Currently selected screenshot id */
  selectedId: string | null;

  // Actions
  addScreenshot: (screenshot: Screenshot) => void;
  removeScreenshot: (id: string) => void;
  updateNotes: (id: string, notes: string) => void;
  clearAll: () => void;
  setPanelOpen: (open: boolean) => void;
  togglePanel: () => void;
  setSelectedId: (id: string | null) => void;
}

let _idCounter = 0;

export const useScreenshotStore = create<ScreenshotState>((set) => ({
  screenshots: [],
  panelOpen: false,
  selectedId: null,

  addScreenshot: (screenshot) =>
    set((state) => ({
      screenshots: [screenshot, ...state.screenshots],
      panelOpen: true,
      selectedId: screenshot.id,
    })),

  removeScreenshot: (id) =>
    set((state) => ({
      screenshots: state.screenshots.filter((s) => s.id !== id),
      selectedId: state.selectedId === id ? null : state.selectedId,
    })),

  updateNotes: (id, notes) =>
    set((state) => ({
      screenshots: state.screenshots.map((s) =>
        s.id === id ? { ...s, notes } : s
      ),
    })),

  clearAll: () => set({ screenshots: [], selectedId: null }),

  setPanelOpen: (open) => set({ panelOpen: open }),

  togglePanel: () => set((state) => ({ panelOpen: !state.panelOpen })),

  setSelectedId: (id) => set({ selectedId: id }),
}));

/** Generate a unique screenshot ID */
export function generateScreenshotId(): string {
  _idCounter++;
  return `ss_${Date.now()}_${_idCounter}`;
}
