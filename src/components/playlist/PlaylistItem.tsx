// ────────────────────────────────────────────────
// PlaylistItem — Optimized with React.memo
// Uses direct store access via getState() to avoid
// unnecessary re-renders on unrelated store changes.
// ────────────────────────────────────────────────

import React, { useState, useRef, useCallback, memo } from 'react';
import { useSortable } from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import {
  GripVertical, Film, Pencil, Check, X, Copy, Trash2, Loader2,
} from 'lucide-react';
import { cn } from '@/utils/cn';
import { formatDuration, formatBytes, formatFps, formatResolution, pathToAssetUrl } from '@/utils';
import { Tag, Badge, Spinner } from '@/components/ui/Badge';
import { usePlaylistStore } from '@/store/playlistStore';
import type { PlaylistEntry } from '@/types';
import { PLAYLIST_ITEM } from '@/constants';
import { FileHealthBadge } from '@/features/playlist/FileHealthBadge';

interface PlaylistItemProps {
  entry: PlaylistEntry;
  index: number;
  isSelected: boolean;
  isFocused: boolean;
  onSelect: (id: string, mode: 'single' | 'toggle' | 'range') => void;
  onContextMenu?: (e: React.MouseEvent, id: string) => void;
  isDragOverlay?: boolean;
}

function PlaylistItemInner({
  entry, index, isSelected, isFocused, onSelect, onContextMenu, isDragOverlay = false,
}: PlaylistItemProps) {
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState('');
  const renameRef = useRef<HTMLInputElement>(null);

  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: entry.id,
    disabled: isDragOverlay,
  });

  const style: React.CSSProperties = {
    transform: CSS.Transform.toString(transform),
    transition: isDragging ? undefined : transition,
    height: PLAYLIST_ITEM.HEIGHT,
  };

  const handleClick = useCallback((e: React.MouseEvent) => {
    if (isRenaming) return;
    if (e.ctrlKey || e.metaKey) onSelect(entry.id, 'toggle');
    else if (e.shiftKey) onSelect(entry.id, 'range');
    else onSelect(entry.id, 'single');
  }, [entry.id, isRenaming, onSelect]);

  const startRename = useCallback(() => {
    setRenameValue(entry.name);
    setIsRenaming(true);
    setTimeout(() => { renameRef.current?.select(); }, 0);
  }, [entry.name]);

  const commitRename = useCallback(() => {
    const trimmed = renameValue.trim();
    if (trimmed && trimmed !== entry.name) {
      usePlaylistStore.getState().renameEntry(entry.id, trimmed);
    }
    setIsRenaming(false);
  }, [renameValue, entry.id, entry.name]);

  const cancelRename = useCallback(() => setIsRenaming(false), []);

  const handleRenameKey = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Enter') commitRename();
    else if (e.key === 'Escape') cancelRename();
  }, [commitRename, cancelRename]);

  const video = entry.mediaInfo?.videoStreams?.[0];
  const audio = entry.mediaInfo?.audioStreams?.[0];

  const thumbnailSrc = entry.thumbnailPath ? pathToAssetUrl(entry.thumbnailPath) : null;

  return (
    <div
      ref={setNodeRef}
      style={style}
      className={cn(
        'group relative flex items-center gap-2 px-3',
        'border-b border-border/40 cursor-pointer select-none',
        'transition-colors duration-100',
        isSelected
          ? 'bg-accent-muted border-l-2 border-l-accent-500'
          : 'hover:bg-bg-elevated border-l-2 border-l-transparent',
        isFocused && !isSelected && 'bg-bg-elevated/60',
        isDragging && 'opacity-30',
        isDragOverlay && 'shadow-modal rounded-lg border border-accent-500/30 bg-bg-elevated/95',
      )}
      onClick={handleClick}
      onDoubleClick={startRename}
      onContextMenu={(e) => { e.preventDefault(); onContextMenu?.(e, entry.id); }}
    >
      {/* Row number */}
      <span className="text-2xs text-text-disabled w-5 text-right shrink-0 font-mono tabular-nums">
        {index + 1}
      </span>

      {/* Drag handle */}
      <button
        {...attributes}
        {...listeners}
        className={cn(
          'p-0.5 rounded shrink-0 text-text-disabled',
          'opacity-0 group-hover:opacity-100 transition-opacity',
          'hover:text-text-muted cursor-grab active:cursor-grabbing focus:outline-none',
        )}
        onClick={(e) => e.stopPropagation()}
        tabIndex={-1}
        aria-label="Drag to reorder"
      >
        <GripVertical className="w-3.5 h-3.5" />
      </button>

      {/* Thumbnail */}
      <div className="w-[71px] h-10 rounded shrink-0 overflow-hidden bg-bg-overlay border border-border/50 flex items-center justify-center">
        {entry.isLoadingThumbnail ? (
          <Spinner size="xs" className="text-text-muted" />
        ) : thumbnailSrc ? (
          <img
            src={thumbnailSrc}
            alt=""
            loading="lazy"
            className="w-full h-full object-cover"
            draggable={false}
            onError={() => { usePlaylistStore.getState().setThumbnailPath(entry.id, null); }}
          />
        ) : (
          <Film className="w-4 h-4 text-text-disabled" />
        )}
      </div>

      {/* Main info */}
      <div className="flex-1 min-w-0 flex flex-col gap-0.5">
        {isRenaming ? (
          <div className="flex items-center gap-1" onClick={(e) => e.stopPropagation()}>
            <input
              ref={renameRef}
              value={renameValue}
              onChange={(e) => setRenameValue(e.target.value)}
              onKeyDown={handleRenameKey}
              onBlur={commitRename}
              autoFocus
              className="flex-1 min-w-0 h-6 px-1.5 text-sm bg-bg-overlay border border-accent-500/60 rounded-xs text-text-primary focus:outline-none"
              aria-label="Rename file"
            />
            <button onClick={commitRename} className="p-0.5 rounded text-success hover:bg-success/10" aria-label="Confirm rename">
              <Check className="w-3.5 h-3.5" />
            </button>
            <button onClick={cancelRename} className="p-0.5 rounded text-text-muted hover:bg-bg-overlay" aria-label="Cancel rename">
              <X className="w-3.5 h-3.5" />
            </button>
          </div>
        ) : (
          <span className="text-sm text-text-primary truncate leading-tight" title={entry.path}>
            {entry.name}
          </span>
        )}

        <div className="flex items-center gap-2 flex-wrap">
          {entry.isProbing ? (
            <span className="flex items-center gap-1 text-2xs text-text-muted">
              <Loader2 className="w-3 h-3 animate-spin" aria-hidden="true" />
              <span>probing…</span>
            </span>
          ) : entry.mediaInfo ? (
            <>
              <span className="text-2xs text-text-muted font-mono">{formatDuration(entry.mediaInfo.duration)}</span>
              {video && <Tag>{video.codecName}</Tag>}
              {video && <span className="text-2xs text-text-disabled">{formatResolution(entry.mediaInfo)}</span>}
              {video?.fps !== undefined && <span className="text-2xs text-text-disabled">{formatFps(video.fps)} fps</span>}
              {audio && <Tag>{audio.codecName}</Tag>}
              {entry.mediaInfo?.subtitleStreams?.map((stream, idx) => {
                let label = stream.codecName.toUpperCase();
                if (label === 'SUBRIP') label = 'SRT';
                if (label === 'WEBVTT') label = 'VTT';
                const lang = stream.language || stream.title;
                const displayLabel = lang && lang.toLowerCase() !== 'external' ? `${label} (${lang.toUpperCase()})` : label;
                return (
                  <Badge
                    key={idx}
                    variant="info"
                    className="font-mono uppercase tracking-wider text-[9px] py-0 px-1 font-semibold border-highlight/40"
                    title={`${stream.isExternal ? 'External' : 'Embedded'} ${stream.codecName} subtitle stream${lang ? ` (${lang})` : ''}`}
                  >
                    {displayLabel}
                  </Badge>
                );
              })}
            </>
          ) : (
            <span className="text-2xs text-text-disabled truncate">{entry.path}</span>
          )}
        </div>
      </div>

      {/* Health indicator */}
      <div className="shrink-0">
        <FileHealthBadge
          status={entry.health?.status}
          confidence={entry.health?.confidence}
          message={entry.health?.message}
          technicalDetails={entry.health?.technicalDetails}
          compact
        />
      </div>

      {/* File size */}
      <span className="text-2xs text-text-muted font-mono shrink-0 w-14 text-right tabular-nums">
        {entry.size > 0 ? formatBytes(entry.size) : ''}
      </span>

      {/* Row actions */}
      <div
        className="flex items-center gap-0.5 shrink-0 opacity-0 group-hover:opacity-100 transition-opacity"
        onClick={(e) => e.stopPropagation()}
      >
        <button onClick={startRename} title="Rename (F2)"
          className="p-1 rounded text-text-muted hover:text-text-secondary hover:bg-bg-overlay transition-colors"
          aria-label="Rename file">
          <Pencil className="w-3 h-3" />
        </button>
        <button onClick={() => usePlaylistStore.getState().duplicateEntry(entry.id)} title="Duplicate"
          className="p-1 rounded text-text-muted hover:text-text-secondary hover:bg-bg-overlay transition-colors"
          aria-label="Duplicate file">
          <Copy className="w-3 h-3" />
        </button>
        <button onClick={() => usePlaylistStore.getState().removeEntries([entry.id])} title="Remove"
          className="p-1 rounded text-text-muted hover:text-danger hover:bg-danger/10 transition-colors"
          aria-label="Remove file">
          <Trash2 className="w-3 h-3" />
        </button>
      </div>
    </div>
  );
}

// ─── Memoized Export ─────────────────────────────
// Custom comparison function to avoid re-renders
// when unrelated store properties change.
export const PlaylistItem = memo(PlaylistItemInner, (prev, next) => {
  return (
    prev.entry.id === next.entry.id &&
    prev.entry.name === next.entry.name &&
    prev.entry.path === next.entry.path &&
    prev.entry.size === next.entry.size &&
    prev.entry.thumbnailPath === next.entry.thumbnailPath &&
    prev.entry.isProbing === next.entry.isProbing &&
    prev.entry.isLoadingThumbnail === next.entry.isLoadingThumbnail &&
    prev.entry.mediaInfo === next.entry.mediaInfo &&
    prev.entry.health === next.entry.health &&
    prev.isSelected === next.isSelected &&
    prev.isFocused === next.isFocused &&
    prev.index === next.index
  );
});

// ─── Drag Overlay ─────────────────────────────────

export function PlaylistItemDragOverlay({ entry }: { entry: PlaylistEntry }) {
  return (
    <div
      className="flex items-center gap-2 px-3 bg-bg-elevated/95 backdrop-blur-md rounded-lg border border-accent-500/40 shadow-modal"
      style={{ height: PLAYLIST_ITEM.HEIGHT }}
    >
      <GripVertical className="w-3.5 h-3.5 text-text-muted shrink-0" />
      <div className="w-[71px] h-10 rounded bg-bg-overlay border border-border/50 flex items-center justify-center shrink-0">
        <Film className="w-4 h-4 text-text-disabled" />
      </div>
      <span className="text-sm text-text-primary truncate flex-1">{entry.name}</span>
    </div>
  );
}
