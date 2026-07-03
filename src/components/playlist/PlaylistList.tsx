import React, { useState, useCallback, useRef, useMemo } from 'react';
import {
  DndContext,
  DragOverlay,
  closestCenter,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragStartEvent,
} from '@dnd-kit/core';
import {
  SortableContext,
  sortableKeyboardCoordinates,
  verticalListSortingStrategy,
} from '@dnd-kit/sortable';
import { restrictToVerticalAxis, restrictToWindowEdges } from '@dnd-kit/modifiers';
import { useVirtualizer } from '@tanstack/react-virtual';
import { Film, Folder as FolderIcon, ChevronRight } from 'lucide-react';
import { formatDuration } from '@/utils';
import { usePlaylistStore } from '@/store/playlistStore';
import { PlaylistItem, PlaylistItemDragOverlay } from './PlaylistItem';
import { PlaylistContextMenu } from './ContextMenu';
import { FolderContextMenu } from './FolderContextMenu';
import { EmptyState } from '@/components/ui/Badge';
import { Button } from '@/components/ui/Button';
import { PLAYLIST_ITEM } from '@/constants';
import type { PlaylistEntry, Folder } from '@/types';

interface ContextMenuState {
  x: number;
  y: number;
  targetId: string;
}

interface FolderContextMenuState {
  x: number;
  y: number;
  folderId: string;
}

interface PlaylistListProps {
  onAddFiles: () => void;
}

export const PlaylistList = React.memo(function PlaylistList({ onAddFiles }: PlaylistListProps) {
  const entries = usePlaylistStore((s) => s.entries);
  const folders = usePlaylistStore((s) => s.folders);
  const selectedIds = usePlaylistStore((s) => s.selectedIds);
  const focusedId = usePlaylistStore((s) => s.focusedId);
  const searchQuery = usePlaylistStore((s) => s.searchQuery);
  const sort = usePlaylistStore((s) => s.sort);
  const toggleFolderCollapse = usePlaylistStore((s) => s.toggleFolderCollapse);
  const renameFolder = usePlaylistStore((s) => s.renameFolder);
  const [activeDragId, setActiveDragId] = useState<string | null>(null);
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
  const [folderContextMenu, setFolderContextMenu] = useState<FolderContextMenuState | null>(null);
  const [renamingFolderId, setRenamingFolderId] = useState<string | null>(null);
  const [renamingFolderName, setRenamingFolderName] = useState('');
  const [, setRenamingId] = useState<string | null>(null);
  const parentRef = useRef<HTMLDivElement>(null);

  const folderMap = useMemo(() => new Map(folders.map((f) => [f.path, f])), [folders]);

  const visibleEntries = React.useMemo(() => {
    return usePlaylistStore.getState().getVisibleEntries();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [entries, searchQuery, sort, folders]);
  const activeDragEntry = activeDragId
    ? entries.find(e => e.id === activeDragId)
    : null;

  const sensors = useSensors(
    useSensor(PointerSensor, {
      activationConstraint: { distance: 5 },
    }),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    })
  );

  // Grouped list items: headers for directories and entries for files
  type GroupedListItem =
    | { type: 'header'; id: string; name: string; fullName: string; duration: number; folderIndex: number; level: number; isCollapsed: boolean }
    | { type: 'entry'; id: string; entry: PlaylistEntry; flatIndex: number; level: number };

  const groupedItems = React.useMemo(() => {
    const items: GroupedListItem[] = [];
    let currentFolder: string | null = null;
    let folderAccumulator: { entry: PlaylistEntry; originalIndex: number }[] = [];

    const addFolderGroup = (folder: string | null, groupEntries: { entry: PlaylistEntry; originalIndex: number }[]) => {
      if (groupEntries.length === 0) return;
      const folderDuration = groupEntries.reduce((acc, item) => acc + (item.entry.mediaInfo?.duration ?? 0), 0);
      if (folder) {
        const folderData = folderMap.get(folder);
        const level = folder.split('/').length - 1;
        const displayName = folderData?.name || folder.split('/').pop() || folder;
        const folderIdx = folderData?.order ?? 0;

        items.push({
          type: 'header',
          id: folder,
          name: displayName,
          fullName: folder,
          duration: folderDuration,
          folderIndex: folderIdx + 1,
          level,
          isCollapsed: folderData?.isCollapsed ?? false,
        });
      }
      if (!folder || !(folderMap.get(folder)?.isCollapsed)) {
        for (const item of groupEntries) {
          const level = folder ? folder.split('/').length : 0;
          items.push({
            type: 'entry',
            id: item.entry.id,
            entry: item.entry,
            flatIndex: item.originalIndex,
            level,
          });
        }
      }
    };

    for (let i = 0; i < visibleEntries.length; i++) {
      const entry = visibleEntries[i];
      const displayFolder = (() => {
        if (entry.relativePath) {
          const normalized = entry.relativePath.replace(/\\/g, '/');
          const parts = normalized.split('/');
          if (parts.length > 1) {
            parts.pop();
            return parts.join('/');
          }
        }
        return entry.parentFolder || null;
      })();

      if (displayFolder !== currentFolder) {
        addFolderGroup(currentFolder, folderAccumulator);
        currentFolder = displayFolder;
        folderAccumulator = [{ entry, originalIndex: i }];
      } else {
        folderAccumulator.push({ entry, originalIndex: i });
      }
    }
    addFolderGroup(currentFolder, folderAccumulator);
    return items;
  }, [visibleEntries, folderMap]);

  const virtualizer = useVirtualizer({
    count: groupedItems.length,
    getScrollElement: () => parentRef.current,
    estimateSize: (index) => {
      const item = groupedItems[index];
      return item?.type === 'header' ? 36 : PLAYLIST_ITEM.HEIGHT;
    },
    overscan: PLAYLIST_ITEM.OVERSCAN_COUNT,
  });

  const handleDragStart = useCallback((event: DragStartEvent) => {
    setActiveDragId(String(event.active.id));
    // If dragging an unselected item, select it
    const id = String(event.active.id);
    const st = usePlaylistStore.getState();
    if (!st.selectedIds.has(id)) {
      st.selectEntry(id, 'single');
    }
  }, []);

  const handleDragEnd = useCallback((event: DragEndEvent) => {
    setActiveDragId(null);
    const { active, over } = event;
    if (!over || active.id === over.id) return;

    const st = usePlaylistStore.getState();
    const fromIndex = st.entries.findIndex(e => e.id === active.id);
    const toIndex = st.entries.findIndex(e => e.id === over.id);

    if (fromIndex === -1 || toIndex === -1) return;

    // If multiple selected, move all selected items
    if (st.selectedIds.size > 1 && st.selectedIds.has(String(active.id))) {
      st.moveSelectedTo(toIndex);
    } else {
      st.reorderEntry(fromIndex, toIndex);
    }
  }, []);

  const handleSelect = useCallback(
    (id: string, mode: 'single' | 'toggle' | 'range') => {
      usePlaylistStore.getState().selectEntry(id, mode);
      usePlaylistStore.getState().setFocused(id);
    },
    []
  );

  const handleContextMenu = useCallback((e: React.MouseEvent, id: string) => {
    e.preventDefault();
    const currentSelected = usePlaylistStore.getState().selectedIds;
    if (!currentSelected.has(id)) {
      usePlaylistStore.getState().selectEntry(id, 'single');
    }
    setContextMenu({ x: e.clientX, y: e.clientY, targetId: id });
  }, []);

  const handleFolderContextMenu = useCallback((e: React.MouseEvent, folderId: string) => {
    e.preventDefault();
    setFolderContextMenu({ x: e.clientX, y: e.clientY, folderId });
  }, []);

  const handleFolderDoubleClick = useCallback((folderId: string) => {
    toggleFolderCollapse(folderId);
  }, [toggleFolderCollapse]);

  const handleStartFolderRename = useCallback((folderId: string) => {
    const folder = folders.find((f) => f.id === folderId);
    if (folder) {
      setRenamingFolderId(folderId);
      setRenamingFolderName(folder.name);
    }
  }, [folders]);

  const handleFinishFolderRename = useCallback(() => {
    if (renamingFolderId && renamingFolderName.trim()) {
      renameFolder(renamingFolderId, renamingFolderName.trim());
    }
    setRenamingFolderId(null);
    setRenamingFolderName('');
  }, [renamingFolderId, renamingFolderName, renameFolder]);

  if (visibleEntries.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center">
        {searchQuery ? (
          <EmptyState
            title="No results"
            description={`No files match "${searchQuery}"`}
            action={
              <Button variant="ghost" size="sm" onClick={() => usePlaylistStore.getState().setSearchQuery('')}>
                Clear search
              </Button>
            }
          />
        ) : (
          <EmptyState
            icon={<Film className="w-5 h-5" />}
            title="Playlist is empty"
            description="Drop videos here, or click to add files and folders"
            action={
              <Button variant="primary" size="sm" onClick={onAddFiles}>
                Add Files
              </Button>
            }
          />
        )}
      </div>
    );
  }

  return (
    <>
      <DndContext
        sensors={sensors}
        collisionDetection={closestCenter}
        modifiers={[restrictToVerticalAxis, restrictToWindowEdges]}
        onDragStart={handleDragStart}
        onDragEnd={handleDragEnd}
      >
        <SortableContext
          items={visibleEntries.map(e => e.id)}
          strategy={verticalListSortingStrategy}
        >
          <div
            ref={parentRef}
            className="flex-1 overflow-y-auto overflow-x-hidden scrollbar-thin"
          >
            {/* Virtualizer total height */}
            <div
              style={{ height: virtualizer.getTotalSize(), position: 'relative' }}
            >
              {virtualizer.getVirtualItems().map((vItem) => {
                const item = groupedItems[vItem.index];
                if (!item) return null;

                const level = item.level || 0;
                const indentPx = level * 18;

                if (item.type === 'header') {
                  const isRoot = level === 0;
                  const isRenaming = renamingFolderId === item.id;
                  const isCollapsed = item.isCollapsed;

                  return (
                    <div
                      key={item.id}
                      style={{
                        position: 'absolute',
                        top: 0,
                        left: 0,
                        right: 0,
                        transform: `translateY(${vItem.start}px)`,
                        height: `${vItem.size}px`,
                        paddingLeft: `${indentPx}px`,
                      }}
                      className="px-3 flex items-center justify-between relative group"
                    >
                      {/* Tree guidelines */}
                      {Array.from({ length: level }).map((_, idx) => (
                        <div
                          key={idx}
                          style={{ left: `${idx * 18 + 20}px` }}
                          className="absolute top-0 bottom-0 w-px bg-border/20 pointer-events-none"
                        />
                      ))}
                      <div
                        className="absolute left-0 right-0 top-0 bottom-0 cursor-pointer"
                        onDoubleClick={() => handleFolderDoubleClick(item.id)}
                        onContextMenu={(e) => handleFolderContextMenu(e, item.id)}
                      />
                      {isRoot ? (
                        <div className="w-full flex items-center justify-between bg-accent-600/90 border border-accent-500/30 text-white select-none px-3 py-1.5 rounded-md text-xs font-semibold uppercase tracking-wider shadow-sm">
                          <div className="flex items-center gap-2 truncate">
                            <button
                              onClick={() => handleFolderDoubleClick(item.id)}
                              className="p-0.5 hover:bg-white/10 rounded transition-colors"
                            >
                              <ChevronRight className={`w-3.5 h-3.5 text-white/80 shrink-0 transition-transform duration-200 ${isCollapsed ? '' : 'rotate-90'}`} />
                            </button>
                            <FolderIcon className="w-3.5 h-3.5 text-white/80 shrink-0" />
                            {isRenaming ? (
                              <input
                                autoFocus
                                value={renamingFolderName}
                                onChange={(e) => setRenamingFolderName(e.target.value)}
                                onBlur={handleFinishFolderRename}
                                onKeyDown={(e) => {
                                  if (e.key === 'Enter') handleFinishFolderRename();
                                  if (e.key === 'Escape') {
                                    setRenamingFolderId(null);
                                    setRenamingFolderName('');
                                  }
                                }}
                                className="bg-white/20 border border-white/30 rounded px-1.5 py-0.5 text-xs text-white placeholder-white/50 outline-none"
                                onClick={(e) => e.stopPropagation()}
                              />
                            ) : (
                              <span className="truncate">{item.folderIndex}. {item.name}</span>
                            )}
                          </div>
                          <span className="text-2xs text-white/95 font-mono shrink-0 ml-4">
                            ({formatDuration(item.duration)})
                          </span>
                        </div>
                      ) : (
                        <div className="w-full flex items-center justify-between bg-bg-surface/50 border border-border/80 text-text-secondary select-none px-3 py-1.5 rounded-md text-[11px] font-semibold uppercase tracking-wider shadow-2xs backdrop-blur-md">
                          <div className="flex items-center gap-2 truncate">
                            <button
                              onClick={() => handleFolderDoubleClick(item.id)}
                              className="p-0.5 hover:bg-bg-overlay rounded transition-colors"
                            >
                              <ChevronRight className={`w-3 h-3 text-text-muted shrink-0 transition-transform duration-200 ${isCollapsed ? '' : 'rotate-90'}`} />
                            </button>
                            <FolderIcon className="w-3 h-3 text-accent-400 shrink-0" />
                            {isRenaming ? (
                              <input
                                autoFocus
                                value={renamingFolderName}
                                onChange={(e) => setRenamingFolderName(e.target.value)}
                                onBlur={handleFinishFolderRename}
                                onKeyDown={(e) => {
                                  if (e.key === 'Enter') handleFinishFolderRename();
                                  if (e.key === 'Escape') {
                                    setRenamingFolderId(null);
                                    setRenamingFolderName('');
                                  }
                                }}
                                className="bg-bg-elevated border border-accent-500 rounded px-1.5 py-0.5 text-[11px] text-text-primary outline-none"
                                onClick={(e) => e.stopPropagation()}
                              />
                            ) : (
                              <span className="truncate">{item.name}</span>
                            )}
                          </div>
                          <span className="text-3xs text-text-muted font-mono shrink-0 ml-4">
                            ({formatDuration(item.duration)})
                          </span>
                        </div>
                      )}
                    </div>
                  );
                }

                const entry = item.entry;
                return (
                  <div
                    key={entry.id}
                    style={{
                      position: 'absolute',
                      top: 0,
                      left: 0,
                      right: 0,
                      transform: `translateY(${vItem.start}px)`,
                      paddingLeft: `${indentPx}px`,
                    }}
                    className="relative"
                  >
                    {/* Tree guidelines */}
                    {Array.from({ length: level }).map((_, idx) => (
                      <div
                        key={idx}
                        style={{ left: `${idx * 18 + 20}px` }}
                        className="absolute top-0 bottom-0 w-px bg-border/20 pointer-events-none"
                      />
                    ))}
                    <PlaylistItem
                      entry={entry}
                      index={item.flatIndex}
                      isSelected={selectedIds.has(entry.id)}
                      isFocused={focusedId === entry.id}
                      onSelect={handleSelect}
                      onContextMenu={handleContextMenu}
                    />
                  </div>
                );
              })}
            </div>
          </div>
        </SortableContext>

        <DragOverlay>
          {activeDragEntry && (
            <PlaylistItemDragOverlay entry={activeDragEntry} />
          )}
        </DragOverlay>
      </DndContext>

      {contextMenu && (
        <PlaylistContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          targetId={contextMenu.targetId}
          onClose={() => setContextMenu(null)}
          onRename={(id) => setRenamingId(id)}
        />
      )}

      {folderContextMenu && (
        <FolderContextMenu
          x={folderContextMenu.x}
          y={folderContextMenu.y}
          folderId={folderContextMenu.folderId}
          onClose={() => setFolderContextMenu(null)}
          onRename={(folderId) => handleStartFolderRename(folderId)}
        />
      )}
    </>
  );
});
