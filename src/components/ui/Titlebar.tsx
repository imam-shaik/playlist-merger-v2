import React, { useEffect, useState } from 'react';
import { Minus, Square, Copy, X, Zap } from 'lucide-react';
import { useMergeStore } from '@/store/mergeStore';
import { useSplitStore } from '@/store/splitStore';
import { message } from '@tauri-apps/plugin-dialog';

const MERGE_ACTIVE_PHASES = ['writing', 'preparing', 'probing', 'validating', 'normalizing', 'finalizing'] as const;

export function Titlebar() {
  const [isMaximized, setIsMaximized] = useState(false);

  const activeMergeJob = useMergeStore((s) => s.activeJob);
  const activeSplitJob = useSplitStore((s) => s.activeJob);
  const cancelMerge = useMergeStore((s) => s.cancelJob);
  const cancelSplitJob = useSplitStore((s) => s.cancelJob);

  const hasActiveMerge = activeMergeJob !== null && activeMergeJob !== undefined &&
    (MERGE_ACTIVE_PHASES as readonly string[]).includes(activeMergeJob.progress.phase);
  const hasActiveSplit = activeSplitJob !== null && activeSplitJob.stage === 'splitting';
  const hasActiveJob = hasActiveMerge || hasActiveSplit;

  useEffect(() => {
    const updateMaximized = async () => {
      try {
        const { getCurrentWindow } = await import('@tauri-apps/api/window');
        setIsMaximized(await getCurrentWindow().isMaximized());
      } catch {
        // Fallback or ignore in browser
      }
    };

    window.addEventListener('resize', updateMaximized);
    updateMaximized();
    return () => window.removeEventListener('resize', updateMaximized);
  }, []);

  const handleMinimize = async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      await getCurrentWindow().minimize();
    } catch {
      console.warn('Minimize is only supported in Tauri desktop app.');
    }
  };

  const handleMaximize = async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      await getCurrentWindow().toggleMaximize();
    } catch {
      console.warn('Maximize is only supported in Tauri desktop app.');
    }
  };

  const handleClose = async () => {
    if (hasActiveJob) {
      const confirmed = await message(
        'An operation is in progress. Closing will cancel the current merge/split. Are you sure?',
        {
          title: 'Confirm Close',
          kind: 'warning',
        }
      );

      if (!confirmed) {
        return;
      }

      if (hasActiveMerge && activeMergeJob) {
        try {
          const { tauriCommands } = await import('@/tauri/commands');
          await tauriCommands.cancelMerge(activeMergeJob.id);
        } catch { /* non-critical */ }
        cancelMerge(activeMergeJob.id);
      }
      if (hasActiveSplit && activeSplitJob) {
        cancelSplitJob(activeSplitJob.id);
      }

      // Wait for backend to detect cancel flag, kill FFmpeg, and clean up.
      // FFmpeg process termination + temp file cleanup can take several seconds.
      // Startup cleanup (lib.rs) handles orphaned .merging markers >24h old.
      await new Promise((resolve) => setTimeout(resolve, 5000));
    }

    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      await getCurrentWindow().close();
    } catch {
      console.warn('Close is only supported in Tauri desktop app.');
    }
  };

  return (
    <div 
      className="flex items-center justify-between h-8 bg-bg-surface border-b border-border select-none shrink-0 z-50 text-text-secondary"
      style={{ WebkitAppRegion: 'drag' } as React.CSSProperties}
      data-tauri-drag-region
    >
      {/* Title & Logo */}
      <div 
        className="flex items-center gap-2 pl-3 pointer-events-none"
        data-tauri-drag-region
      >
        <div className="w-5 h-5 rounded bg-accent-600 flex items-center justify-center shrink-0 shadow-sm">
          <Zap className="w-3 h-3 text-white fill-white" />
        </div>
        <span className="text-xs font-semibold text-text-primary tracking-wide font-sans">
          PlaylistMerger
        </span>
      </div>

      {/* Draggable space */}
      <div 
        className="flex-1 h-full cursor-default"
        data-tauri-drag-region
      />

      {/* Window Controls */}
      <div className="flex items-center h-full no-drag" style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}>
        <button
          onClick={handleMinimize}
          className="flex items-center justify-center w-11 h-full hover:bg-bg-elevated hover:text-text-primary transition-colors duration-100"
          title="Minimize"
        >
          <Minus className="w-3.5 h-3.5" />
        </button>
        <button
          onClick={handleMaximize}
          className="flex items-center justify-center w-11 h-full hover:bg-bg-elevated hover:text-text-primary transition-colors duration-100"
          title={isMaximized ? 'Restore' : 'Maximize'}
        >
          {isMaximized ? (
            <Copy className="w-3 h-3 rotate-180" />
          ) : (
            <Square className="w-3 h-3" />
          )}
        </button>
        <button
          onClick={handleClose}
          className="flex items-center justify-center w-11 h-full hover:bg-danger hover:text-white transition-colors duration-100"
          title="Close"
        >
          <X className="w-3.5 h-3.5" />
        </button>
      </div>
    </div>
  );
}
