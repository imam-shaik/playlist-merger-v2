// ────────────────────────────────────────────────
// MergeActionBar — Sticky bottom bar with merge
// button, warnings, and FFmpeg command preview.
// ────────────────────────────────────────────────

import React from 'react';
import { Zap, Sliders, AlertTriangle, ChevronRight } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { useMergeStore } from '@/store/mergeStore';
import { usePlaylistStore } from '@/store/playlistStore';
import type { CompatibilityReport } from '@/types';

interface MergeActionBarProps {
  report: CompatibilityReport | null;
  onMerge: () => void;
}

export function MergeActionBar({ report, onMerge }: MergeActionBarProps) {
  const mergeMode = useMergeStore((s) => s.mergeMode);
  const audioRepairMode = useMergeStore((s) => s.audioRepairMode);
  const videoCodec = useMergeStore((s) => s.videoCodec);
  const videoCrf = useMergeStore((s) => s.videoCrf);
  const audioCodec = useMergeStore((s) => s.audioCodec);
  const entries = usePlaylistStore((s) => s.entries);
  const isCheckingCompat = useMergeStore((s) => s.isCheckingCompat);

  const subtitleMode = useMergeStore((s) => s.subtitleMode);
  const splitConfig = useMergeStore((s) => s.splitConfig);
  const isSrtOnly = subtitleMode === 'srtMergeOnly';

  // Resolved execution mode: when compatibility report exists, use its recommendedMode
  // which reflects what the backend will actually do after auto-upgrade detection.
  // Falls back to user's selected mergeMode if no report yet.
  const resolvedMode = (() => {
    if (isSrtOnly) return 'lossless';
    if (report) return report.recommendedMode;
    return mergeMode;
  })();

  // Block lossless merge if:
  // 1. Report exists and says it's incompatible (auto-upgrade needed)
  // 2. No report yet (compatibility check hasn't completed) — prevent silent auto-upgrade
  const shouldBlockLossless = !isSrtOnly && mergeMode === 'lossless' && (
    (report && !report.isCompatible) || !report
  );
  const canMerge = entries.length >= 2 && !shouldBlockLossless;

  // Determine button label based on resolved execution mode
  const buttonLabel = (() => {
    if (isSrtOnly) return 'Merge SRT Only';
    if (mergeMode === 'fastMkv') return 'Fast MKV Merge';
    if (mergeMode === 'smartMkv') return 'Smart MKV Merge';
    if (resolvedMode === 'lossless') {
      if (report?.recommendedMode === 'lossless') return 'Lossless Merge';
      // No report yet, user selected lossless
      return 'Lossless Merge';
    }
    if (mergeMode === 'lossless' && resolvedMode === 'custom') {
      return 'Lossless → Custom (auto-upgraded)';
    }
    return 'Custom Merge';
  })();

  return (
    <div className="shrink-0 p-4 bg-bg-surface/80 backdrop-blur-sm border-t border-border">
      {/* Warn if about to lossless-merge incompatible files */}
      {mergeMode === 'lossless' && report && !report.isCompatible && (
        <div className="mb-3 flex items-center gap-2 px-3 py-2 rounded-lg bg-error-500/10 border border-error-500/20">
          <AlertTriangle className="w-3.5 h-3.5 text-error-400 shrink-0" />
          <p className="text-xs text-error-400 font-medium">Lossless merge is unavailable due to file incompatibilities. Switch to Custom Quality mode.</p>
        </div>
      )}

      {/* Warn if no compatibility check has been run for lossless mode */}
      {mergeMode === 'lossless' && !report && !isCheckingCompat && (
        <div className="mb-3 flex items-center gap-2 px-3 py-2 rounded-lg bg-warning/10 border border-warning/20">
          <AlertTriangle className="w-3.5 h-3.5 text-warning shrink-0" />
          <p className="text-xs text-warning font-medium">Compatibility check not yet run. Run it first, or enable auto-check in settings.</p>
        </div>
      )}

      {/* Warn if check is in progress */}
      {mergeMode === 'lossless' && !report && isCheckingCompat && (
        <div className="mb-3 flex items-center gap-2 px-3 py-2 rounded-lg bg-accent-500/10 border border-accent-500/20">
          <AlertTriangle className="w-3.5 h-3.5 text-accent-400 shrink-0 animate-pulse" />
          <p className="text-xs text-accent-400 font-medium">Checking file compatibility…</p>
        </div>
      )}

      {/* Warn about auto-upgrade */}
      {mergeMode === 'lossless' && resolvedMode === 'custom' && report && (
        <div className="mb-3 flex items-center gap-2 px-3 py-2 rounded-lg bg-warning/10 border border-warning/20">
          <AlertTriangle className="w-3.5 h-3.5 text-warning shrink-0" />
          <p className="text-xs text-warning font-medium">
            Files are incompatible with stream copy — merge will re-encode to Custom Quality.
          </p>
        </div>
      )}

      {/* Warn about srtMergeOnly + split */}
      {isSrtOnly && splitConfig?.mode !== 'none' && (
        <div className="mb-3 flex items-center gap-2 px-3 py-2 rounded-lg bg-warning/10 border border-warning/20">
          <AlertTriangle className="w-3.5 h-3.5 text-warning shrink-0" />
          <p className="text-xs text-warning font-medium">
            SRT Merge Only + Split will produce multiple .srt files (one per part).
          </p>
        </div>
      )}

      {/* Warn about subtitle mode conflict */}
      {!isSrtOnly && subtitleMode !== 'none' && splitConfig?.mode !== 'none' && splitConfig?.subtitleMode && (
        <div className="mb-3 flex items-center gap-2 px-3 py-2 rounded-lg bg-info/10 border border-info/20">
          <AlertTriangle className="w-3.5 h-3.5 text-info shrink-0" />
          <p className="text-xs text-info font-medium">
            Per-part subtitle mode ({splitConfig.subtitleMode}) overrides global subtitle mode ({subtitleMode}).
          </p>
        </div>
      )}

      <Button
        variant="primary"
        size="lg"
        className="w-full"
        leftIcon={isSrtOnly ? <Zap className="w-4 h-4" /> : resolvedMode === 'lossless' ? <Zap className="w-4 h-4" /> : <Sliders className="w-4 h-4" />}
        rightIcon={<ChevronRight className="w-4 h-4" />}
        onClick={onMerge}
        disabled={!canMerge}
        aria-label={canMerge ? (isSrtOnly ? `Merge subtitles for ${entries.length} files` : `Merge ${entries.length} files (${resolvedMode} mode)`) : shouldBlockLossless ? 'Run compatibility check or switch to Custom mode' : 'Need at least 2 files to merge'}
      >
        {buttonLabel}{' '}
        <span className="opacity-60 text-xs ml-1">{entries.length} files</span>
      </Button>
      <p className="text-2xs text-text-muted text-center mt-2">
        {isSrtOnly
          ? 'ffmpeg -f concat -safe 0 -i sub_list.txt -c:s srt output.srt'
          : resolvedMode === 'lossless'
          ? audioRepairMode === 'fast'
            ? 'ffmpeg -f concat -safe 0 -i list.txt -c copy output.mp4'
            : audioRepairMode === 'smart'
            ? 'smart check -> repair bad audio -> ffmpeg -c copy output.mp4'
            : 'repair all audio -> ffmpeg -c copy output.mp4'
          : `ffmpeg -c:v ${videoCodec} -crf ${videoCrf} -c:a ${audioCodec}`}
      </p>
    </div>
  );
}
