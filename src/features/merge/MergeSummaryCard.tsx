// ────────────────────────────────────────────────
// MergeSummaryCard — Displays a detailed per-video
// merge report preview with thumbnails, durations,
// resolution, and cumulative timestamps.
// ────────────────────────────────────────────────

import React, { useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { ChevronDown, ChevronUp, FileText, Film } from 'lucide-react';
import { usePlaylistStore } from '@/store/playlistStore';
import { useMergeStore } from '@/store/mergeStore';
import { useProbe, useThumbnails } from '@/hooks/useProbe';
import { formatDuration, formatBytes, totalDuration, estimateOutputSize, pathToAssetUrl } from '@/utils';
import { cn } from '@/utils/cn';

export function MergeSummaryCard() {
  const entries = usePlaylistStore((s) => s.entries);
  const totalDur = totalDuration(entries);
  const estimatedSize = estimateOutputSize(entries);
  const [expanded, setExpanded] = useState(false);
  const { probeEntries } = useProbe();
  const { generateThumbnails } = useThumbnails();

  // Access store for enriched stats
  const compatibilityReport = useMergeStore((s) => s.compatibilityReport);
  const mergeMode = useMergeStore((s) => s.mergeMode);
  const videoCrf = useMergeStore((s) => s.videoCrf);

  // Resolved execution mode: use compatibility report's recommendedMode when available
  // (reflects what the backend will actually do after auto-upgrade checks)
  const resolvedMode = compatibilityReport?.recommendedMode ?? mergeMode;
  const modeMismatch = mergeMode !== resolvedMode; // user selected one mode, backend will run another
  const splitConfig = useMergeStore((s) => s.splitConfig);
  const subtitleMode = useMergeStore((s) => s.subtitleMode);
  const exportMergedSrt = useMergeStore((s) => s.exportMergedSrt);
  const cardEnabled = useMergeStore((s) => s.cardEnabled);
  const cardDuration = useMergeStore((s) => s.cardDuration);
  const cardFrequency = useMergeStore((s) => s.cardFrequency);
  const repeatConfig = useMergeStore((s) => s.repeatConfig);

  // Compute cumulative start/end times for each entry
  const entriesWithTimes = React.useMemo(() => {
    let cursor = 0;
    return entries.map((e) => {
      const start = cursor;
      const dur = e.mediaInfo?.duration ?? 0;
      cursor += dur;
      return { entry: e, startTime: start, endTime: cursor };
    });
  }, [entries]);

  // Entries still needing probe / thumbnails
  const needsProbe = entries.filter((e) => !e.mediaInfo && !e.isProbing);
  const needsThumb = entries.filter((e) => !e.thumbnailPath && !e.isLoadingThumbnail);

  // Compute effective repeat count
  // FIX P2: Use deduped duration approximation for untilDuration calculation
  // The deduped duration is the total duration minus duplicates.
  // For approximate frontend calculation, we use totalDur as a conservative estimate.
  // If there are many duplicates, the backend may produce different results.
  const effectiveRepeatCount = React.useMemo(() => {
    if (!repeatConfig.enabled) return 0;
    if (totalDur <= 0) return 0;

    let countFromCount = 0;
    let countFromDuration = 0;

    if (repeatConfig.byCount) {
      countFromCount = repeatConfig.repeatCount ?? 0;
    }

    // P2 FIX: For untilDuration, we use totalDur as the basis.
    // The backend uses working_total_duration which is the deduped duration.
    // Without backend info, we approximate using totalDur.
    // NOTE: This may differ from backend if playlist has duplicate files.
    if (repeatConfig.untilDuration) {
      const target = repeatConfig.targetDurationSeconds ?? 0;
      if (target > 0) {
        // Use totalDur as conservative estimate for deduped duration
        // NOTE: If files are unique, this equals the backend calculation
        countFromDuration = Math.ceil(target / totalDur);
      }
    }

    if (repeatConfig.byCount && repeatConfig.untilDuration) return Math.max(countFromCount, countFromDuration);
    if (repeatConfig.byCount) return countFromCount;
    if (repeatConfig.untilDuration) return countFromDuration;
    return 0;
  }, [repeatConfig, totalDur]);

  const finalDuration = totalDur * (effectiveRepeatCount || 1);

  const handleProbeMissing = () => {
    if (needsProbe.length > 0) {
      probeEntries(needsProbe.map((e) => e.id));
    }
    if (needsThumb.length > 0) {
      generateThumbnails(needsThumb.map((e) => e.id));
    }
  };

  return (
    <div className="bg-bg-elevated border border-border rounded-xl overflow-hidden">
      {/* ── Aggregate Stats ── */}
      <div className="p-4">
        <div className="flex items-center justify-between mb-3">
          <p className="text-xs font-semibold text-text-muted uppercase tracking-wider">
            Merge Preview
          </p>
          {(needsProbe.length > 0 || needsThumb.length > 0) && (
            <button
              type="button"
              onClick={handleProbeMissing}
              className="text-[9px] font-medium text-accent-400 hover:text-accent-300 transition-colors px-2 py-0.5 rounded bg-accent-500/10 border border-accent-500/20"
            >
              Load missing info
            </button>
          )}
        </div>
        <div className={cn('grid gap-2', effectiveRepeatCount > 0 ? 'grid-cols-6' : 'grid-cols-5')}>
          {[
            {
              label: 'Mode',
              value: resolvedMode === 'lossless' ? 'Lossless' : resolvedMode === 'fastMkv' ? 'Fast MKV' : resolvedMode === 'smartMkv' ? 'Smart MKV' : 'Custom',
              variant: modeMismatch
                ? 'warning'
                : resolvedMode === 'lossless' ? 'success' : 'warning',
            },
            // Original stage metrics (pre-pipeline)
            { label: 'Original', value: `${entries.length} files` },
            { label: 'Duration', value: formatDuration(totalDur) },
            ...(effectiveRepeatCount > 0 ? [{
              label: 'Repeat',
              value: `×${effectiveRepeatCount}`,
              variant: 'info' as const,
            }] : []),
            // Output stage metrics (post-pipeline)
            ...(effectiveRepeatCount > 0 ? [{
              label: 'Effective',
              value: `${entries.length * effectiveRepeatCount} videos`,
            }] : []),
            { label: 'Est. Size', value: formatBytes((() => {
              // For Custom mode, estimate based on CRF (higher CRF = smaller output)
              // CRF 18 ≈ 0.5× original, CRF 23 ≈ 0.3× original, CRF 28 ≈ 0.15× original
              if (resolvedMode === 'custom' && videoCrf != null) {
                const crfFactor = Math.max(0.05, Math.min(2, Math.pow(2, (18 - videoCrf) / 6)));
                return estimatedSize * crfFactor * (effectiveRepeatCount || 1);
              }
              return estimatedSize * (effectiveRepeatCount || 1);
            })()) },
            { label: 'Avg / file', value: entries.length > 0 ? formatDuration(totalDur / entries.length) : '—' },
          ].map((s) => (
            <div key={s.label}>
              <p className="text-2xs text-text-muted">{s.label}</p>
              {s.variant ? (
                <p className={cn(
                  'text-sm font-semibold mt-0.5',
                  s.variant === 'success' ? 'text-success' : s.variant === 'warning' ? 'text-warning' : s.variant === 'info' ? 'text-accent-400' : 'text-text-primary'
                )}>
                  {s.value}
                </p>
              ) : (
                <p className="text-sm font-semibold text-text-primary mt-0.5">{s.value}</p>
              )}
            </div>
          ))}
        </div>
        {splitConfig.mode !== 'none' && (
          <p className="text-2xs text-text-muted -mt-1">
            {splitConfig.mode === 'count' && splitConfig.partCount
              ? `Split: ${splitConfig.partCount} parts · ${entries.length} files`
              : splitConfig.mode === 'duration'
              ? `Split: ${formatDuration(totalDur / (splitConfig.maxDurationPerPart ?? 3600))} per part`
              : splitConfig.mode === 'folder'
              ? 'Split by folder groups'
              : `Split: ${splitConfig.partCount ?? 0} parts`}
          </p>
        )}
        {effectiveRepeatCount > 0 && (
          <p className="text-2xs text-accent-400/80 -mt-1">
            Repeat: {entries.length} files × {effectiveRepeatCount} → {formatDuration(finalDuration)} final
          </p>
        )}
        <div className="flex items-center gap-3 mt-1.5">
          {cardEnabled && (() => {
            const expandedFileCount = entries.length * (effectiveRepeatCount || 1);
            // Card count depends on frequency mode:
            // PerVideo: card before each video except the first → N-1
            // PerFolder: card before the first video of each folder group → count unique parent folders
            let cardCount: number;
            if (cardFrequency === 'perFolder') {
              // Count folder transitions in the (expanded) file list
              const folders = new Set<string>();
              const expandedEntries = effectiveRepeatCount > 1
                ? Array.from({ length: expandedFileCount }, (_, i) => entries[i % entries.length])
                : entries;
              for (const e of expandedEntries) {
                const parts = e.path.replace(/\\/g, '/').split('/');
                // Use second-to-last segment as folder indicator (same as backend's get_top_level_component)
                const folder = parts.length >= 2 ? parts[parts.length - 2] : 'Root';
                folders.add(folder);
              }
              cardCount = folders.size;
            } else {
              // PerVideo: card before each video except the first
              cardCount = Math.max(0, expandedFileCount - 1);
            }
            const canvasAdded = cardDuration * cardCount;
            const expandedDuration = totalDur * (effectiveRepeatCount || 1);
            const finalDur = expandedDuration + canvasAdded;
            return (
              <span className="text-2xs text-accent-400/80">
                Canvas: {cardCount} card{cardCount !== 1 ? 's' : ''} · {cardDuration}s each · +{formatDuration(canvasAdded)}<span className="text-accent-400/60"> → </span>{formatDuration(finalDur)}
              </span>
            );
          })()}
          {subtitleMode !== 'none' && (() => {
            // P1 FIX: Use expanded count for subtitle tracks
            // After repeat expansion, each expanded file contributes its subtitle tracks
            const originalSubtitleCount = entries.reduce((t, e) => t + (e.mediaInfo?.subtitleStreams?.length ?? 0), 0);
            const total = originalSubtitleCount * (effectiveRepeatCount || 1);
            const modeLabel = subtitleMode === 'embed' ? 'embed' : subtitleMode === 'burn' ? 'burn' : subtitleMode === 'exportSrt' ? 'export as srt' : 'srt merge only';
            const outputHint = subtitleMode === 'embed'
              ? '✓ 1 video'
              : subtitleMode === 'burn'
              ? '⚠ 1 video (re-encode)'
              : subtitleMode === 'exportSrt'
              ? '✓ 1 video · ✓ 1 .srt'
              : '✗ no video · ✓ 1 merged.srt';
            return (
              <span className="text-2xs text-accent-400/80">
                Subtitles: {modeLabel}{total > 0 ? ` · ${total} track${total !== 1 ? 's' : ''}` : ''} · {outputHint}
              </span>
            );
          })()}
          {exportMergedSrt && subtitleMode !== 'exportSrt' && subtitleMode !== 'srtMergeOnly' && (
            <span className="text-2xs text-success/80">+ ✓ 1 .srt</span>
          )}
        </div>
      </div>

      {/* ── Divider ── */}
      <div className="border-t border-border/40" />

      {/* ── Per-File Details Toggle ── */}
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        className="w-full flex items-center justify-between px-4 py-2.5 hover:bg-bg-surface/40 transition-colors text-left"
      >
        <div className="flex items-center gap-2">
          <FileText className="w-3.5 h-3.5 text-accent-400" />
          <span className="text-[10px] font-semibold text-text-muted uppercase tracking-wider">
            File Details ({entries.length} files)
          </span>
        </div>
        <div className="flex items-center gap-2">
          <span className="text-[9px] text-accent-400">{expanded ? 'Hide' : 'Show'}</span>
          {expanded ? (
            <ChevronUp className="w-3.5 h-3.5 text-text-muted" />
          ) : (
            <ChevronDown className="w-3.5 h-3.5 text-text-muted" />
          )}
        </div>
      </button>

      {/* ── Per-File Report ── */}
      <AnimatePresence>
        {expanded && (
          <motion.div
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: 'auto' }}
            exit={{ opacity: 0, height: 0 }}
            className="overflow-hidden border-t border-border/30"
          >
            {/* Column headers */}
            <div
              className="grid text-[8px] font-semibold uppercase tracking-wider text-text-disabled px-3 py-1.5 bg-bg-base/60"
              style={{ gridTemplateColumns: '1.5rem 2.5rem 1fr 5rem 4.5rem 5.5rem 4.5rem' }}
            >
              <span>#</span>
              <span />
              <span>Source File</span>
              <span className="text-right">Duration</span>
              <span className="text-right">Resolution</span>
              <span className="text-center">Start → End</span>
              <span className="text-right">Size</span>
            </div>

            {/* File rows */}
            <div className="max-h-64 overflow-y-auto scrollbar-thin">
              {entriesWithTimes.map(({ entry, startTime, endTime }, idx) => {
                const mi = entry.mediaInfo;
                const vs = mi?.videoStreams?.[0];
                const thumbnailSrc = entry.thumbnailPath ? pathToAssetUrl(entry.thumbnailPath) : null;
                return (
                  <div
                    key={entry.id}
                    className={cn(
                      'grid items-center px-3 py-2 border-t border-border/15 transition-colors hover:bg-bg-surface/30',
                      idx % 2 === 0 ? 'bg-transparent' : 'bg-bg-base/20'
                    )}
                    style={{ gridTemplateColumns: '1.5rem 2.5rem 1fr 5rem 4.5rem 5.5rem 4.5rem' }}
                  >
                    {/* Index */}
                    <span className="text-[9px] font-mono text-text-disabled">
                      {String(idx + 1).padStart(2, '0')}
                    </span>

                    {/* Thumbnail */}
                    <div className="flex items-center justify-center">
                      {entry.isLoadingThumbnail ? (
                        <div className="w-10 h-[22px] rounded bg-bg-base border border-border/20 skeleton" />
                      ) : thumbnailSrc ? (
                        <img
                          src={thumbnailSrc}
                          alt=""
                          className="w-10 h-[22px] rounded object-cover border border-border/40 bg-bg-base"
                          loading="lazy"
                          onError={() => { usePlaylistStore.getState().setThumbnailPath(entry.id, null); }}
                        />
                      ) : (
                        <div className="w-10 h-[22px] rounded bg-bg-base border border-border/20 flex items-center justify-center">
                          <Film className="w-3 h-3 text-text-disabled" />
                        </div>
                      )}
                    </div>

                    {/* File name */}
                    <div className="min-w-0 pr-2">
                      <p className="text-[10px] text-text-secondary truncate" title={entry.name}>
                        {entry.name}
                      </p>
                      <div className="flex items-center gap-1.5 mt-0.5">
                        {vs?.codecName && (
                          <span className="text-[8px] text-text-muted bg-bg-overlay px-1 py-0.5 rounded">
                            {vs.codecName}
                          </span>
                        )}
                        {mi?.audioStreams?.[0]?.codecName && (
                          <span className="text-[8px] text-text-muted bg-bg-overlay px-1 py-0.5 rounded">
                            {mi.audioStreams[0].codecName}
                          </span>
                        )}
                      </div>
                    </div>

                    {/* Duration */}
                    <span className="text-[9px] font-mono text-text-muted text-right">
                      {entry.isProbing ? (
                        <span className="text-text-disabled">…</span>
                      ) : mi ? (
                        formatDuration(mi.duration)
                      ) : (
                        <span className="text-text-disabled">—</span>
                      )}
                    </span>

                    {/* Resolution */}
                    <span className="text-[9px] font-mono text-text-muted text-right">
                      {vs?.width && vs?.height ? (
                        `${vs.width}×${vs.height}`
                      ) : (
                        <span className="text-text-disabled">—</span>
                      )}
                    </span>

                    {/* Start → End */}
                    <span className="text-[9px] font-mono text-accent-400/80 text-center">
                      {mi ? (
                        <>{formatDuration(startTime)}&nbsp;<span className="text-text-disabled">→</span>&nbsp;{formatDuration(endTime)}</>
                      ) : (
                        <span className="text-text-disabled">—</span>
                      )}
                    </span>

                    {/* Size */}
                    <span className="text-[9px] font-mono text-text-muted text-right">
                      {formatBytes(entry.size)}
                    </span>
                  </div>
                );
              })}
            </div>

            {/* Footer with remaining */}
            <div className="border-t border-border/20 px-3 py-1.5 flex items-center justify-between bg-bg-base/40">
              <span className="text-[8px] text-text-muted">
                {entriesWithTimes.filter((e) => !e.entry.mediaInfo).length > 0
                  ? 'Some files still being probed — duration/start times are estimates'
                  : 'Timestamps reflect the order shown in the playlist'}
              </span>
              <span className="text-[9px] font-mono text-success">
                Total: {formatDuration(totalDur)}
              </span>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
