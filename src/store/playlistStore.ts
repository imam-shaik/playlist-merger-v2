import { create } from 'zustand';
import type { PlaylistEntry, PlaylistSort, SortField, SortDirection, MediaInfo, FileHealth, Folder } from '@/types';
import { generateId, getBasename, getExtension, totalDuration, moveItems } from '@/utils';

interface PlaylistState {
  entries: PlaylistEntry[];
  folders: Folder[];
  selectedIds: Set<string>;
  searchQuery: string;
  filteredIds: string[] | null;
  sort: PlaylistSort;
  playlistId: string;
  playlistName: string;
  isDirty: boolean;
  focusedId: string | null;
  /** True while a folder scan is in progress (sets entries after complete) */
  scanningFiles: boolean;

  addFiles: (files: Array<{ path: string; name: string; size: number; extension: string; modified?: number; created?: number; parentFolder?: string; relativePath?: string }>) => void;
  removeEntries: (ids: string[]) => void;
  duplicateEntry: (id: string) => void;
  renameEntry: (id: string, name: string) => void;
  reorderEntry: (fromIndex: number, toIndex: number) => void;
  moveSelectedTo: (toIndex: number) => void;
  clearAll: () => void;
  setEntries: (entries: PlaylistEntry[]) => void;

  setProbing: (id: string, value: boolean) => void;
  setMediaInfo: (id: string, info: MediaInfo) => void;
  /** Batch update multiple entries at once — avoids re-render storm */
  batchSetMediaInfo: (updates: Map<string, MediaInfo>) => void;
  setThumbnailPath: (id: string, path: string | null) => void;
  /** Batch update multiple thumbnail paths — avoids re-render storm */
  batchSetThumbnailPath: (updates: Map<string, string | null>) => void;
  setLoadingThumbnail: (id: string, value: boolean) => void;
  /** Clear all thumbnail paths (call after cache cleanup to prevent 404s) */
  clearAllThumbnailPaths: () => void;
  setHealth: (id: string, health: FileHealth | null) => void;

  selectEntry: (id: string, mode: 'single' | 'toggle' | 'range') => void;
  selectAll: () => void;
  clearSelection: () => void;

  setSort: (field: SortField, direction?: SortDirection) => void;
  applySortToEntries: () => void;
  setSearchQuery: (query: string) => void;
  setPlaylistName: (name: string) => void;
  newPlaylist: () => void;
  setDirty: (dirty: boolean) => void;
  setFocused: (id: string | null) => void;
  setScanningFiles: (scanning: boolean) => void;

  getVisibleEntries: () => PlaylistEntry[];
  getTotalDuration: () => number;
  syncFolders: () => void;

  deleteFolder: (folderId: string) => void;
  renameFolder: (folderId: string, name: string) => void;
  moveFolderUp: (folderId: string) => void;
  moveFolderDown: (folderId: string) => void;
  toggleFolderCollapse: (folderId: string) => void;
  selectOnlyFolder: (folderId: string) => void;
  clearFolders: () => void;
}

const DEFAULT_SORT: PlaylistSort = { field: 'manual', direction: 'asc' };

export const usePlaylistStore = create<PlaylistState>()((set, get) => ({
  entries: [],
  folders: [],
  selectedIds: new Set(),
  searchQuery: '',
  filteredIds: null,
  sort: DEFAULT_SORT,
  playlistId: generateId(),
  playlistName: 'New Playlist',
  isDirty: false,
  focusedId: null,
  scanningFiles: false,

  addFiles: (files) => {
    set((state) => {
      const existingPaths = new Set(state.entries.map((e) => e.path));
      const newEntries: PlaylistEntry[] = files
        .filter((f) => !existingPaths.has(f.path))
        .map((f) => ({
          id: generateId(),
          name: getBasename(f.name || f.path),
          path: f.path,
          parentFolder: f.parentFolder,
          relativePath: f.relativePath,
          extension: f.extension || getExtension(f.path),
          size: f.size,
          mediaInfo: null,
          thumbnailPath: null,
          isProbing: false,
          isLoadingThumbnail: false,
          modified: f.modified,
          created: f.created,
        }));
      return { entries: [...state.entries, ...newEntries], isDirty: newEntries.length > 0 };
    });
    get().syncFolders();
  },

  removeEntries: (ids) => {
    const idSet = new Set(ids);
    set((state) => ({
      entries: state.entries.filter((e) => !idSet.has(e.id)),
      selectedIds: new Set([...state.selectedIds].filter((id) => !idSet.has(id))),
      focusedId: state.focusedId && idSet.has(state.focusedId) ? null : state.focusedId,
      isDirty: true,
    }));
    get().syncFolders();
  },

  duplicateEntry: (id) => {
    set((state) => {
      const idx = state.entries.findIndex((e) => e.id === id);
      if (idx === -1) return state;
      const dupe: PlaylistEntry = { ...state.entries[idx], id: generateId(), name: `${state.entries[idx].name} (copy)` };
      const next = [...state.entries];
      next.splice(idx + 1, 0, dupe);
      return { entries: next, isDirty: true };
    });
  },

  renameEntry: (id, name) => {
    set((state) => ({
      entries: state.entries.map((e) => e.id === id ? { ...e, name } : e),
      isDirty: true,
    }));
  },

  reorderEntry: (fromIndex, toIndex) => {
    set((state) => ({
      entries: moveItems(state.entries, fromIndex, toIndex),
      sort: DEFAULT_SORT,
      isDirty: true,
    }));
  },

  moveSelectedTo: (toIndex) => {
    set((state) => {
      const { selectedIds, entries } = state;
      if (selectedIds.size === 0) return state;
      const selected = entries.filter((e) => selectedIds.has(e.id));
      const rest = entries.filter((e) => !selectedIds.has(e.id));
      const insertAt = Math.max(0, Math.min(toIndex, rest.length));
      return {
        entries: [...rest.slice(0, insertAt), ...selected, ...rest.slice(insertAt)],
        isDirty: true,
      };
    });
  },

  clearAll: () => set({
    entries: [], folders: [], selectedIds: new Set(), searchQuery: '',
    filteredIds: null, isDirty: false,
  }),

  setEntries: (entries) => {
    set({ entries, isDirty: false });
    get().syncFolders();
  },

  setProbing: (id, value) => set((s) => ({
    entries: s.entries.map((e) => e.id === id ? { ...e, isProbing: value } : e),
  })),

  setMediaInfo: (id, info) => set((s) => ({
    entries: s.entries.map((e) => e.id === id ? { ...e, mediaInfo: info, isProbing: false } : e),
  })),

  batchSetMediaInfo: (updates) => set((s) => ({
    entries: s.entries.map((e) => {
      const info = updates.get(e.id);
      return info ? { ...e, mediaInfo: info, isProbing: false } : e;
    }),
  })),

  setThumbnailPath: (id, path) => set((s) => ({
    entries: s.entries.map((e) => e.id === id ? { ...e, thumbnailPath: path, isLoadingThumbnail: false } : e),
  })),

  batchSetThumbnailPath: (updates) => set((s) => ({
    entries: s.entries.map((e) => {
      const path = updates.get(e.id);
      if (path === undefined) return e;
      return { ...e, thumbnailPath: path, isLoadingThumbnail: false };
    }),
  })),

  setLoadingThumbnail: (id, value) => set((s) => ({
    entries: s.entries.map((e) => e.id === id ? { ...e, isLoadingThumbnail: value } : e),
  })),

  clearAllThumbnailPaths: () => set((s) => ({
    entries: s.entries.map((e) => e.thumbnailPath ? { ...e, thumbnailPath: null } : e),
  })),

  setHealth: (id, health) => set((s) => ({
    entries: s.entries.map((e) => e.id === id ? { ...e, health } : e),
  })),

  selectEntry: (id, mode) => {
    set((state) => {
      if (mode === 'single') return { selectedIds: new Set([id]), focusedId: id };
      if (mode === 'toggle') {
        const next = new Set(state.selectedIds);
        if (next.has(id)) next.delete(id); else next.add(id);
        return { selectedIds: next, focusedId: id };
      }
      if (mode === 'range') {
        const focusedIdx = state.entries.findIndex((e) => e.id === state.focusedId);
        const clickedIdx = state.entries.findIndex((e) => e.id === id);
        if (focusedIdx === -1) return { selectedIds: new Set([id]), focusedId: id };
        const [lo, hi] = [Math.min(focusedIdx, clickedIdx), Math.max(focusedIdx, clickedIdx)];
        const next = new Set(state.selectedIds);
        for (let i = lo; i <= hi; i++) next.add(state.entries[i].id);
        return { selectedIds: next };
      }
      return state;
    });
  },

  selectAll: () => set((s) => ({ selectedIds: new Set(s.entries.map((e) => e.id)) })),
  clearSelection: () => set({ selectedIds: new Set() }),

  setSort: (field, direction) => {
    set((state) => {
      const dir = direction ?? (state.sort.field === field && state.sort.direction === 'asc' ? 'desc' : 'asc');
      return { sort: { field, direction: dir } };
    });
    get().applySortToEntries();
  },

  applySortToEntries: () => {
    set((state) => {
      if (state.sort.field === 'manual') return state;
      const folderOrderMap = new Map(state.folders.map((f, i) => [f.path, i]));

      const getDisplayFolder = (e: PlaylistEntry): string => {
        if (e.relativePath) {
          const normalized = e.relativePath.replace(/\\/g, '/');
          const parts = normalized.split('/');
          if (parts.length > 1) {
            parts.pop();
            return parts.join('/');
          }
        }
        return e.parentFolder || '';
      };

      const sorted = [...state.entries].sort((a, b) => {
        const folderA = getDisplayFolder(a);
        const folderB = getDisplayFolder(b);

        let cmp = 0;
        if (state.sort.field === 'folder') {
          const orderA = folderOrderMap.get(folderA) ?? Infinity;
          const orderB = folderOrderMap.get(folderB) ?? Infinity;
          cmp = orderA - orderB;
          if (cmp === 0) {
            cmp = a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: 'base' });
          }
        } else {
          const getFolderPath = (e: PlaylistEntry) => {
            if (e.relativePath) {
              const normalized = e.relativePath.replace(/\\/g, '/');
              const parts = normalized.split('/');
              if (parts.length > 1) {
                parts.pop();
                return parts.join('/');
              }
            }
            return e.parentFolder || '';
          };

          const fA = getFolderPath(a);
          const fB = getFolderPath(b);
          cmp = fA.localeCompare(fB, undefined, { numeric: true, sensitivity: 'base' });
          if (cmp === 0) {
            switch (state.sort.field) {
              case 'name':
                cmp = a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: 'base' });
                break;
              case 'duration':
                cmp = (a.mediaInfo?.duration ?? 0) - (b.mediaInfo?.duration ?? 0);
                break;
              case 'size':
                cmp = a.size - b.size;
                break;
              case 'resolution': {
                const ar = (a.mediaInfo?.videoStreams[0]?.width ?? 0) * (a.mediaInfo?.videoStreams[0]?.height ?? 0);
                const br = (b.mediaInfo?.videoStreams[0]?.width ?? 0) * (b.mediaInfo?.videoStreams[0]?.height ?? 0);
                cmp = ar - br;
                break;
              }
              case 'fps':
                cmp = (a.mediaInfo?.videoStreams[0]?.fps ?? 0) - (b.mediaInfo?.videoStreams[0]?.fps ?? 0);
                break;
              case 'modified':
                cmp = (a.modified ?? 0) - (b.modified ?? 0);
                break;
              case 'created':
                cmp = (a.created ?? 0) - (b.created ?? 0);
                break;
              default:
                cmp = a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: 'base' });
                break;
            }
          }
        }
        return state.sort.direction === 'desc' ? -cmp : cmp;
      });
      return { entries: sorted };
    });
  },

  setSearchQuery: (query) => {
    set((state) => {
      const q = query.trim().toLowerCase();
      const filteredIds = q
        ? state.entries.filter((e) =>
            e.name.toLowerCase().includes(q) || e.path.toLowerCase().includes(q)
          ).map((e) => e.id)
        : null;
      return { searchQuery: query, filteredIds };
    });
  },

  setPlaylistName: (name) => set({ playlistName: name, isDirty: true }),

  newPlaylist: () => set({
    entries: [], folders: [], selectedIds: new Set(), searchQuery: '', filteredIds: null,
    sort: DEFAULT_SORT, playlistId: generateId(), playlistName: 'New Playlist',
    isDirty: false, focusedId: null,
  }),

  setDirty: (dirty) => set({ isDirty: dirty }),
  setFocused: (id) => set({ focusedId: id }),
  setScanningFiles: (scanning) => set({ scanningFiles: scanning }),

  getVisibleEntries: () => {
    const { entries, filteredIds, folders } = get();
    const collapsedPaths = new Set(folders.filter((f) => f.isCollapsed).map((f) => f.path));

    let result = entries;
    if (filteredIds) {
      const idSet = new Set(filteredIds);
      result = result.filter((e) => idSet.has(e.id));
    }

    if (collapsedPaths.size > 0) {
      result = result.filter((e) => {
        const displayFolder = e.relativePath
          ? e.relativePath.replace(/\\/g, '/').split('/').slice(0, -1).join('/')
          : e.parentFolder;
        return !collapsedPaths.has(displayFolder || '');
      });
    }

    return result;
  },

  getTotalDuration: () => totalDuration(get().entries),

  syncFolders: () => {
    const state = get();
    if (state.entries.length === 0) {
      set({ folders: [] });
      return;
    }

    const getDisplayFolder = (entry: PlaylistEntry): string | null => {
      if (entry.relativePath) {
        const normalized = entry.relativePath.replace(/\\/g, '/');
        const parts = normalized.split('/');
        if (parts.length > 1) {
          parts.pop();
          return parts.join('/');
        }
      }
      return entry.parentFolder || null;
    };

    const existingFolders = new Map(state.folders.map((f) => [f.path, f]));
    const folderPaths = new Set<string>();

    for (const entry of state.entries) {
      const folder = getDisplayFolder(entry);
      if (folder) folderPaths.add(folder);
    }

    const sortedPaths = [...folderPaths].sort();
    const newFolders: Folder[] = sortedPaths.map((path, index) => {
      const existing = existingFolders.get(path);
      return {
        id: path,
        path,
        name: existing?.name || path.split('/').pop() || path,
        order: existing?.order ?? index,
        isCollapsed: existing?.isCollapsed ?? false,
      };
    });

    newFolders.sort((a, b) => a.order - b.order);
    newFolders.forEach((f, i) => { f.order = i; });

    set({ folders: newFolders });
  },

  deleteFolder: (folderId: string) => {
    const state = get();
    const folder = state.folders.find((f) => f.id === folderId);
    if (!folder) return;

    const entryIds = state.entries
      .filter((e) => {
        const displayFolder = e.relativePath
          ? e.relativePath.replace(/\\/g, '/').split('/').slice(0, -1).join('/')
          : e.parentFolder;
        return displayFolder === folder.path;
      })
      .map((e) => e.id);

    get().removeEntries(entryIds);
  },

  renameFolder: (folderId: string, name: string) => {
    set((state) => ({
      folders: state.folders.map((f) =>
        f.id === folderId ? { ...f, name } : f
      ),
      isDirty: true,
    }));
  },

  moveFolderUp: (folderId: string) => {
    set((state) => {
      const index = state.folders.findIndex((f) => f.id === folderId);
      if (index <= 0) return state;
      const newFolders = [...state.folders];
      [newFolders[index - 1], newFolders[index]] = [newFolders[index], newFolders[index - 1]];
      newFolders.forEach((f, i) => { f.order = i; });
      return { folders: newFolders, isDirty: true };
    });
    get().applySortToEntries();
  },

  moveFolderDown: (folderId: string) => {
    set((state) => {
      const index = state.folders.findIndex((f) => f.id === folderId);
      if (index === -1 || index >= state.folders.length - 1) return state;
      const newFolders = [...state.folders];
      [newFolders[index], newFolders[index + 1]] = [newFolders[index + 1], newFolders[index]];
      newFolders.forEach((f, i) => { f.order = i; });
      return { folders: newFolders, isDirty: true };
    });
    get().applySortToEntries();
  },

  toggleFolderCollapse: (folderId: string) => {
    set((state) => ({
      folders: state.folders.map((f) =>
        f.id === folderId ? { ...f, isCollapsed: !f.isCollapsed } : f
      ),
    }));
  },

  selectOnlyFolder: (folderId: string) => {
    const state = get();
    const folder = state.folders.find((f) => f.id === folderId);
    if (!folder) return;

    const entryIds = state.entries
      .filter((e) => {
        const displayFolder = e.relativePath
          ? e.relativePath.replace(/\\/g, '/').split('/').slice(0, -1).join('/')
          : e.parentFolder;
        return displayFolder === folder.path;
      })
      .map((e) => e.id);

    set({ selectedIds: new Set(entryIds), focusedId: entryIds[0] || null });
  },

  clearFolders: () => set({ folders: [] }),
}));
