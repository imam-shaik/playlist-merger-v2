// ────────────────────────────────────────────────
// Workspace Store — Persistent UI Layout State
// Remembered across sessions: panel widths, sidebar
// state, collapsed sections, window bounds.
// ────────────────────────────────────────────────

import { create } from 'zustand';
import { persist } from 'zustand/middleware';
// Import only the values we actually use
const DEFAULT_MERGE_PANEL_WIDTH = 380;
const MIN_PANEL_WIDTH = 280;
const MAX_PANEL_WIDTH = 800;

export interface WorkspaceState {
  // Sidebar
  sidebarCollapsed: boolean;

  // Merge panel
  mergePanelWidth: number;
  mergePanelCollapsed: boolean;

  // Collapsible sections within merge panel
  sectionsCollapsed: {
    tasks: boolean;
    summary: boolean;
    encoding: boolean;
    advanced: boolean;
    naming: boolean;
    compatibility: boolean;
    split: boolean;
  };

  // Actions
  toggleSidebar: () => void;
  setMergePanelWidth: (w: number) => void;
  toggleMergePanel: () => void;
  setSectionCollapsed: (section: keyof WorkspaceState['sectionsCollapsed'], collapsed: boolean) => void;
  toggleSection: (section: keyof WorkspaceState['sectionsCollapsed']) => void;
  resetLayout: () => void;
}

const DEFAULT_SECTIONS = {
  tasks: true,
  summary: true,
  encoding: false,
  advanced: false,
  naming: false,
  compatibility: true,
  split: false,
};

export const useWorkspaceStore = create<WorkspaceState>()(
  persist(
    (set) => ({
      sidebarCollapsed: false,
      mergePanelWidth: DEFAULT_MERGE_PANEL_WIDTH,
      mergePanelCollapsed: false,
      sectionsCollapsed: { ...DEFAULT_SECTIONS },

      toggleSidebar: () =>
        set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),

      setMergePanelWidth: (w) =>
        set({ mergePanelWidth: Math.max(MIN_PANEL_WIDTH, Math.min(MAX_PANEL_WIDTH, w)) }),

      toggleMergePanel: () =>
        set((s) => ({ mergePanelCollapsed: !s.mergePanelCollapsed })),

      setSectionCollapsed: (section, collapsed) =>
        set((s) => ({
          sectionsCollapsed: { ...s.sectionsCollapsed, [section]: collapsed },
        })),

      toggleSection: (section) =>
        set((s) => ({
          sectionsCollapsed: {
            ...s.sectionsCollapsed,
            [section]: !s.sectionsCollapsed[section],
          },
        })),

      resetLayout: () =>
        set({
          sidebarCollapsed: false,
          mergePanelWidth: DEFAULT_MERGE_PANEL_WIDTH,
          mergePanelCollapsed: false,
          sectionsCollapsed: { ...DEFAULT_SECTIONS },
        }),
    }),
    {
      name: 'playlist-merger-workspace',
      partialize: (state) => ({
        sidebarCollapsed: state.sidebarCollapsed,
        mergePanelWidth: state.mergePanelWidth,
        mergePanelCollapsed: state.mergePanelCollapsed,
        sectionsCollapsed: state.sectionsCollapsed,
      }),
    }
  )
);
