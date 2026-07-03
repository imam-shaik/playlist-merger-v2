// ────────────────────────────────────────────────
// ScreenshotPanel — Right-side inspector panel
// for displaying captured screenshots with notes.
// ────────────────────────────────────────────────

import React, { useState, useCallback, useRef, useEffect } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import {
  Camera, Trash2, X, StickyNote, Copy, ChevronDown,
} from 'lucide-react';
import { cn } from '@/utils/cn';
import { useScreenshotStore } from '@/store/screenshotStore';
import { tauriCommands } from '@/tauri/commands';
import { useAppStore } from '@/store/appStore';
import { pathToAssetUrl, formatDuration } from '@/utils';
import type { Screenshot } from '@/types';

export function ScreenshotPanel() {
  const screenshots = useScreenshotStore((s) => s.screenshots);
  const selectedId = useScreenshotStore((s) => s.selectedId);
  const setSelectedId = useScreenshotStore((s) => s.setSelectedId);
  const removeScreenshot = useScreenshotStore((s) => s.removeScreenshot);
  const clearAll = useScreenshotStore((s) => s.clearAll);
  const togglePanel = useScreenshotStore((s) => s.togglePanel);

  if (screenshots.length === 0) {
    return (
      <div className="flex flex-col h-full items-center justify-center px-4 py-8">
        <div className="w-10 h-10 rounded-xl bg-bg-overlay border border-border/60 flex items-center justify-center mb-3">
          <Camera className="w-5 h-5 text-text-disabled" />
        </div>
        <p className="text-xs font-medium text-text-muted text-center mb-1">No Screenshots</p>
        <p className="text-[10px] text-text-disabled text-center leading-relaxed">
          Select a video in the playlist and press <kbd className="px-1 py-0.5 bg-bg-overlay border border-border rounded text-[9px] font-mono">S</kbd> to capture a frame
        </p>
      </div>
    );
  }

  const selected = screenshots.find((s) => s.id === selectedId);

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-border/40">
        <div className="flex items-center gap-1.5">
          <Camera className="w-3.5 h-3.5 text-accent-400" />
          <span className="text-xs font-semibold text-text-secondary">
            Screenshots
          </span>
          <span className="text-[9px] text-text-disabled font-mono">
            {screenshots.length}
          </span>
        </div>
        <div className="flex items-center gap-1">
          <button
            onClick={clearAll}
            className="p-1 rounded text-text-disabled hover:text-danger hover:bg-danger/10 transition-colors"
            title="Clear all screenshots"
          >
            <Trash2 className="w-3 h-3" />
          </button>
          <button
            onClick={togglePanel}
            className="p-1 rounded text-text-disabled hover:text-text-secondary hover:bg-bg-overlay transition-colors"
            title="Close panel"
          >
            <X className="w-3 h-3" />
          </button>
        </div>
      </div>

      {/* Screenshot list + detail */}
      <div className="flex-1 min-h-0 overflow-hidden flex flex-col">
        {selected ? (
          <ScreenshotDetail
            screenshot={selected}
            onBack={() => setSelectedId(null)}
            onRemove={() => {
              removeScreenshot(selected.id);
              setSelectedId(null);
            }}
          />
        ) : (
          <ScreenshotList
            screenshots={screenshots}
            selectedId={selectedId}
            onSelect={setSelectedId}
            onRemove={removeScreenshot}
          />
        )}
      </div>
    </div>
  );
}

// ─── Screenshot List ────────────────────────────

function ScreenshotList({
  screenshots,
  selectedId,
  onSelect,
  onRemove,
}: {
  screenshots: Screenshot[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onRemove: (id: string) => void;
}) {
  return (
    <div className="flex-1 overflow-y-auto scrollbar-thin">
      <div className="p-2 space-y-1.5">
        <AnimatePresence initial={false}>
          {screenshots.map((ss) => (
            <ScreenshotCard
              key={ss.id}
              screenshot={ss}
              isSelected={ss.id === selectedId}
              onSelect={() => onSelect(ss.id)}
              onRemove={() => onRemove(ss.id)}
            />
          ))}
        </AnimatePresence>
      </div>
    </div>
  );
}

// ─── Screenshot Card ────────────────────────────

function ScreenshotCard({
  screenshot,
  isSelected,
  onSelect,
  onRemove,
}: {
  screenshot: Screenshot;
  isSelected: boolean;
  onSelect: () => void;
  onRemove: () => void;
}) {
  const imgSrc = pathToAssetUrl(screenshot.imagePath);

  return (
    <motion.div
      layout
      initial={{ opacity: 0, y: -8 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, x: 20 }}
      transition={{ duration: 0.15 }}
      onClick={onSelect}
      className={cn(
        'group rounded-lg border cursor-pointer overflow-hidden transition-all',
        isSelected
          ? 'border-accent-500/50 bg-accent-500/5 shadow-glow-sm'
          : 'border-border/40 bg-bg-base/30 hover:border-border-strong hover:bg-bg-base/60'
      )}
    >
      {/* Thumbnail */}
      <div className="relative aspect-video bg-bg-overlay">
        <img
          src={imgSrc}
          alt=""
          className="w-full h-full object-cover"
          loading="lazy"
          onError={(e) => {
            e.currentTarget.style.display = 'none';
          }}
        />
        {/* Timestamp badge */}
        <div className="absolute bottom-1 left-1 px-1.5 py-0.5 rounded bg-black/70 backdrop-blur-sm">
          <span className="text-[9px] font-mono text-white/90">
            {formatDuration(screenshot.timestamp)}
          </span>
        </div>
        {/* Notes indicator */}
        {screenshot.notes && (
          <div className="absolute top-1 right-1 p-0.5 rounded bg-black/50">
            <StickyNote className="w-2.5 h-2.5 text-amber-400" />
          </div>
        )}
        {/* Remove button */}
        <button
          onClick={(e) => { e.stopPropagation(); onRemove(); }}
          className="absolute top-1 left-1 p-0.5 rounded bg-black/50 text-white/70 hover:text-white hover:bg-black/70 opacity-0 group-hover:opacity-100 transition-opacity"
        >
          <X className="w-2.5 h-2.5" />
        </button>
      </div>

      {/* Info */}
      <div className="px-2 py-1.5">
        <p className="text-[10px] text-text-secondary truncate font-medium" title={screenshot.sourceName}>
          {screenshot.sourceName}
        </p>
        {screenshot.notes && (
          <p className="text-[9px] text-text-muted truncate mt-0.5" title={screenshot.notes}>
            📝 {screenshot.notes}
          </p>
        )}
      </div>
    </motion.div>
  );
}

// ─── Screenshot Detail ──────────────────────────

function ScreenshotDetail({
  screenshot,
  onBack,
  onRemove,
}: {
  screenshot: Screenshot;
  onBack: () => void;
  onRemove: () => void;
}) {
  const updateNotes = useScreenshotStore((s) => s.updateNotes);
  const [notesValue, setNotesValue] = useState(screenshot.notes);
  const notesRef = useRef<HTMLTextAreaElement>(null);
  const toast = useAppStore((s) => s.showToast);

  // Sync notes if screenshot changes
  useEffect(() => {
    setNotesValue(screenshot.notes);
  }, [screenshot.id, screenshot.notes]);

  const handleSaveNotes = useCallback(() => {
    updateNotes(screenshot.id, notesValue);
  }, [screenshot.id, notesValue, updateNotes]);

  const handleCopyPath = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(screenshot.imagePath);
      toast({ type: 'success', title: 'Path copied to clipboard' });
    } catch {
      toast({ type: 'error', title: 'Failed to copy path' });
    }
  }, [screenshot.imagePath, toast]);

  const handleOpenFile = useCallback(async () => {
    try {
      await tauriCommands.openWithDefault(screenshot.imagePath);
    } catch {
      toast({ type: 'error', title: 'Failed to open screenshot' });
    }
  }, [screenshot.imagePath, toast]);

  const imgSrc = pathToAssetUrl(screenshot.imagePath);

  return (
    <div className="flex-1 overflow-y-auto scrollbar-thin flex flex-col">
      {/* Back button */}
      <div className="px-2 py-1.5 border-b border-border/30">
        <button
          onClick={onBack}
          className="flex items-center gap-1 text-[10px] text-text-muted hover:text-text-secondary transition-colors"
        >
          <ChevronDown className="w-3 h-3 rotate-90" />
          All screenshots
        </button>
      </div>

      {/* Full preview */}
      <div className="p-2">
        <div className="rounded-lg overflow-hidden border border-border/40 bg-bg-overlay">
          <img
            src={imgSrc}
            alt=""
            className="w-full object-contain max-h-48"
            onError={(e) => {
              e.currentTarget.style.display = 'none';
            }}
          />
        </div>
      </div>

      {/* Metadata */}
      <div className="px-3 pb-2 space-y-1">
        <div className="flex items-center justify-between">
          <span className="text-[10px] text-text-muted">Source</span>
          <span className="text-[10px] text-text-secondary truncate ml-2" title={screenshot.sourcePath}>
            {screenshot.sourceName}
          </span>
        </div>
        <div className="flex items-center justify-between">
          <span className="text-[10px] text-text-muted">Timestamp</span>
          <span className="text-[10px] text-accent-400 font-mono">
            {formatDuration(screenshot.timestamp)}
          </span>
        </div>
        <div className="flex items-center justify-between">
          <span className="text-[10px] text-text-muted">Captured</span>
          <span className="text-[10px] text-text-secondary">
            {new Date(screenshot.capturedAt).toLocaleTimeString()}
          </span>
        </div>
      </div>

      {/* Notes */}
      <div className="px-3 pb-3">
        <label className="text-[10px] text-text-muted font-semibold uppercase tracking-wider mb-1.5 block">
          Notes
        </label>
        <textarea
          ref={notesRef}
          value={notesValue}
          onChange={(e) => setNotesValue(e.target.value)}
          onBlur={handleSaveNotes}
          placeholder="Add notes about this frame..."
          className="w-full h-20 px-2 py-1.5 text-[11px] text-text-primary bg-bg-overlay border border-border/40 rounded-md resize-none focus:outline-none focus:border-accent-500/50 placeholder:text-text-disabled"
        />
      </div>

      {/* Actions */}
      <div className="px-3 pb-3 flex items-center gap-1.5">
        <button
          onClick={handleOpenFile}
          className="flex-1 flex items-center justify-center gap-1 px-2 py-1.5 text-[10px] font-medium text-text-secondary bg-bg-overlay border border-border/40 rounded-md hover:bg-bg-elevated transition-colors"
        >
          Open Image
        </button>
        <button
          onClick={handleCopyPath}
          className="flex items-center justify-center gap-1 px-2 py-1.5 text-[10px] font-medium text-text-secondary bg-bg-overlay border border-border/40 rounded-md hover:bg-bg-elevated transition-colors"
          title="Copy file path"
        >
          <Copy className="w-3 h-3" />
        </button>
        <button
          onClick={onRemove}
          className="flex items-center justify-center gap-1 px-2 py-1.5 text-[10px] font-medium text-danger bg-danger/5 border border-danger/20 rounded-md hover:bg-danger/10 transition-colors"
          title="Delete screenshot"
        >
          <Trash2 className="w-3 h-3" />
        </button>
      </div>
    </div>
  );
}
