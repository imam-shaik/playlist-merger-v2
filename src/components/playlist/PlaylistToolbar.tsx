import React, { useRef } from 'react';
import {
  Search, FolderOpen, FilePlus2, Trash2, SortAsc, SortDesc,
  ChevronDown, X, Layers, Zap, RefreshCw
} from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import { cn } from '@/utils/cn';
import { Button } from '@/components/ui/Button';
import { Input } from '@/components/ui/Input';
import { usePlaylistStore } from '@/store/playlistStore';
import { useAppStore } from '@/store/appStore';
import { useFileImport } from '@/hooks/useFileImport';
import { formatDuration, formatBytes, totalDuration, estimateOutputSize } from '@/utils';
import type { SortField } from '@/types';

const SORT_OPTIONS: Array<{ value: SortField; label: string }> = [
  { value: 'manual', label: 'Custom Order' },
  { value: 'name', label: 'Filename' },
  { value: 'duration', label: 'Duration' },
  { value: 'size', label: 'File Size' },
  { value: 'resolution', label: 'Resolution' },
  { value: 'fps', label: 'Frame Rate' },
  { value: 'modified', label: 'Date Modified' },
  { value: 'created', label: 'Date Created' },
  { value: 'folder', label: 'Folder' },
];

interface PlaylistToolbarProps {
  onMergeClick: () => void;
}

export function PlaylistToolbar({ onMergeClick }: PlaylistToolbarProps) {
  // ── Selective store subscriptions ─────────────────────────────────────
  // Subscribe to individual slices instead of the full store to prevent
  // unnecessary re-renders when unrelated properties change.
  const entries = usePlaylistStore((s) => s.entries);
  const selectedIds = usePlaylistStore((s) => s.selectedIds);
  const searchQuery = usePlaylistStore((s) => s.searchQuery);
  const sort = usePlaylistStore((s) => s.sort);
  const scanningFiles = usePlaylistStore((s) => s.scanningFiles);
  const removeEntries = usePlaylistStore((s) => s.removeEntries);
  const clearSelection = usePlaylistStore((s) => s.clearSelection);
  const setSearchQuery = usePlaylistStore((s) => s.setSearchQuery);
  const setSortAction = usePlaylistStore((s) => s.setSort);

  const refreshMediaMetadataStore = useAppStore((s) => s.refreshMediaMetadata);
  const [sortOpen, setSortOpen] = React.useState(false);
  const [isPickingFiles, setIsPickingFiles] = React.useState(false);
  const [isPickingFolder, setIsPickingFolder] = React.useState(false);
  const { pickAndAddFiles, pickAndAddFolder } = useFileImport();
  const sortRef = useRef<HTMLDivElement>(null);

  // Close sort dropdown on outside click or Escape
  React.useEffect(() => {
    if (!sortOpen) return;
    const handler = (e: MouseEvent) => {
      if (sortRef.current && !sortRef.current.contains(e.target as Node)) {
        setSortOpen(false);
      }
    };
    const keyHandler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setSortOpen(false);
    };
    document.addEventListener('mousedown', handler);
    document.addEventListener('keydown', keyHandler);
    return () => {
      document.removeEventListener('mousedown', handler);
      document.removeEventListener('keydown', keyHandler);
    };
  }, [sortOpen]);

  const handleAddFiles = async () => {
    if (isPickingFiles) return;
    setIsPickingFiles(true);
    try {
      await pickAndAddFiles();
    } finally {
      setIsPickingFiles(false);
    }
  };

  const handleAddFolder = async () => {
    if (isPickingFolder) return;
    setIsPickingFolder(true);
    try {
      await pickAndAddFolder();
    } finally {
      setIsPickingFolder(false);
    }
  };

  const removeSelected = () => {
    removeEntries([...selectedIds]);
  };

  const selectedCount = selectedIds.size;
  const totalDur = totalDuration(entries);
  const totalSize = estimateOutputSize(entries);

  return (
    <div className="flex flex-col gap-2 px-3 py-2.5 border-b border-border">
      {/* Top row */}
      <div className="flex items-center gap-2">
        {/* Search */}
        <div className="flex-1 min-w-0">
          <Input
            placeholder="Search files…"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            leftIcon={<Search className="w-3.5 h-3.5" />}
            rightElement={
              searchQuery ? (
                <button
                  onClick={() => setSearchQuery('')}
                  className="p-0.5 rounded text-text-muted hover:text-text-secondary transition-colors"
                >
                  <X className="w-3 h-3" />
                </button>
              ) : null
            }
            className="h-7 text-xs"
          />
        </div>

        {/* Sort */}
        <div ref={sortRef} className="relative">
          <Button
            variant="outline"
            size="sm"
            onClick={() => setSortOpen(v => !v)}
            rightIcon={<ChevronDown className={cn('w-3 h-3 transition-transform', sortOpen && 'rotate-180')} />}
            leftIcon={sort.direction === 'asc' ? <SortAsc className="w-3.5 h-3.5" /> : <SortDesc className="w-3.5 h-3.5" />}
          >
            {SORT_OPTIONS.find(o => o.value === sort.field)?.label ?? 'Sort'}
          </Button>

          <AnimatePresence>
            {sortOpen && (
              <motion.div
                initial={{ opacity: 0, y: -4, scale: 0.97 }}
                animate={{ opacity: 1, y: 0, scale: 1 }}
                exit={{ opacity: 0, y: -4, scale: 0.97 }}
                transition={{ duration: 0.12 }}
                className="absolute right-0 top-full mt-1 z-30 w-44 bg-bg-elevated/95 backdrop-blur-md border border-border rounded-lg shadow-modal py-1 overflow-hidden"
              >
                {SORT_OPTIONS.map(opt => (
                  <button
                    key={opt.value}
                    className={cn(
                      'w-full flex items-center justify-between px-3 py-1.5 text-xs text-left transition-colors',
                      'hover:bg-bg-overlay',
                      sort.field === opt.value
                        ? 'text-accent-400 font-medium'
                        : 'text-text-secondary hover:text-text-primary'
                    )}
                    onClick={() => {
                      setSortAction(opt.value);
                      setSortOpen(false);
                    }}
                  >
                    <span>{opt.label}</span>
                    {sort.field === opt.value && (
                      <span className="text-2xs text-text-muted">
                        {sort.direction === 'asc' ? '↑' : '↓'}
                      </span>
                    )}
                  </button>
                ))}
              </motion.div>
            )}
          </AnimatePresence>
        </div>

        {/* Refresh Metadata */}
        <Button
          variant="ghost"
          size="sm"
          onClick={() => {
            refreshMediaMetadataStore();
          }}
          leftIcon={<RefreshCw className="w-3.5 h-3.5" />}
          title="Re-scan all entries for subtitle files and refresh metadata"
        >
          Refresh
        </Button>

        {/* Add buttons */}
        <Button variant="ghost" size="sm" onClick={handleAddFiles} loading={isPickingFiles} leftIcon={<FilePlus2 className="w-3.5 h-3.5" />}>
          Files
        </Button>
        <Button variant="ghost" size="sm" onClick={handleAddFolder} loading={isPickingFolder || scanningFiles} leftIcon={<FolderOpen className="w-3.5 h-3.5" />}>
          Folder
        </Button>
        <Button
          variant="primary"
          size="sm"
          onClick={onMergeClick}
          disabled={entries.length < 2}
          leftIcon={<Zap className="w-3.5 h-3.5" />}
        >
          Merge
        </Button>
      </div>

      {/* Bottom status row */}
      <div className="flex items-center justify-between min-h-[18px]">
        <div className="flex items-center gap-3 text-2xs text-text-muted">
          <span className="flex items-center gap-1">
            <Layers className="w-3 h-3" />
            <span className="font-medium text-text-secondary">{entries.length}</span> files
          </span>
          {totalDur > 0 && (
            <span>{formatDuration(totalDur)} total</span>
          )}
          {totalSize > 0 && (
            <span>~{formatBytes(totalSize)}</span>
          )}
        </div>

        {selectedCount > 0 && (
          <div className="flex items-center gap-2">
            <span className="text-2xs text-accent-400 font-medium">
              {selectedCount} selected
            </span>
            <button
              onClick={removeSelected}
              className="flex items-center gap-1 text-2xs text-danger hover:text-danger/80 transition-colors"
            >
              <Trash2 className="w-3 h-3" />
              Remove
            </button>
            <button
              onClick={clearSelection}
              className="text-2xs text-text-muted hover:text-text-secondary transition-colors"
            >
              Deselect
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
