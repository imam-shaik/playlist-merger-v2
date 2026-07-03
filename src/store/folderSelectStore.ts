import { create } from 'zustand';
import type { SubfolderInfo } from '@/tauri/commands';

interface FolderSelectState {
  isOpen: boolean;
  rootPath: string | null;
  subfolders: SubfolderInfo[];
  selectedPaths: Set<string>;
  resolveRef: ((paths: string[] | null) => void) | null;
  rejectRef: (() => void) | null;
}

interface FolderSelectActions {
  openFolderSelect: (
    rootPath: string,
    subfolders: SubfolderInfo[]
  ) => Promise<string[] | null>;
  selectFolder: (path: string) => void;
  deselectFolder: (path: string) => void;
  toggleFolder: (path: string) => void;
  selectAll: () => void;
  selectNone: () => void;
  confirm: () => void;
  cancel: () => void;
}

type FolderSelectStore = FolderSelectState & FolderSelectActions;

export const useFolderSelectStore = create<FolderSelectStore>()((set, get) => ({
  isOpen: false,
  rootPath: null,
  subfolders: [],
  selectedPaths: new Set(),
  resolveRef: null,
  rejectRef: null,

  openFolderSelect: (rootPath, subfolders) => {
    return new Promise<string[] | null>((resolve, reject) => {
      const initialSelected = new Set(subfolders.map((s) => s.path));
      set({
        isOpen: true,
        rootPath,
        subfolders,
        selectedPaths: initialSelected,
        resolveRef: resolve,
        rejectRef: reject,
      });
      console.log('[FOLDER_AUDIT] STORE UPDATED - isOpen:', true, 'folderCount:', subfolders.length, 'folderNames:', subfolders.map(f => f.name));
    });
  },

  selectFolder: (path) => {
    set((state) => ({
      selectedPaths: new Set([...state.selectedPaths, path]),
    }));
  },

  deselectFolder: (path) => {
    set((state) => {
      const next = new Set(state.selectedPaths);
      next.delete(path);
      return { selectedPaths: next };
    });
  },

  toggleFolder: (path) => {
    const { selectedPaths } = get();
    if (selectedPaths.has(path)) {
      get().deselectFolder(path);
    } else {
      get().selectFolder(path);
    }
  },

  selectAll: () => {
    set((state) => ({
      selectedPaths: new Set(state.subfolders.map((s) => s.path)),
    }));
  },

  selectNone: () => {
    set({ selectedPaths: new Set() });
  },

  confirm: () => {
    const { selectedPaths, resolveRef } = get();
    set({ isOpen: false, resolveRef: null, rejectRef: null });
    if (resolveRef) {
      resolveRef([...selectedPaths]);
    }
  },

  cancel: () => {
    const { rejectRef } = get();
    set({ isOpen: false, resolveRef: null, rejectRef: null });
    if (rejectRef) {
      rejectRef();
    }
  },
}));