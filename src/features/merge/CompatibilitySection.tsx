// ────────────────────────────────────────────────
// CompatibilitySection — Displays compatibility
// report with issues, warnings, errors, and
// recommended mode switch.
// ────────────────────────────────────────────────

import React, { useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import {
  AlertTriangle, CheckCircle2, RefreshCw, Sliders,
  ChevronDown, ChevronUp,
} from 'lucide-react';
import { cn } from '@/utils/cn';
import { Badge, Spinner } from '@/components/ui/Badge';
import { useMergeStore } from '@/store/mergeStore';
import { usePlaylistStore } from '@/store/playlistStore';
import { useWorkspaceStore } from '@/store/workspaceStore';
import { issueKindLabel, computeCompatibilityBreakdown, totalDuration, formatDuration, formatFps } from '@/utils';
import type { CompatibilityReport, IssueSeverity } from '@/types';

const severityText: Record<IssueSeverity, string> = {
  error: 'text-danger',
  warning: 'text-warning',
  info: 'text-accent-400',
};
const severityBorder: Record<IssueSeverity, string> = {
  error: 'border-danger/20 bg-danger/5',
  warning: 'border-warning/20 bg-warning/5',
  info: 'border-accent-500/20 bg-accent-muted',
};

interface CompatibilitySectionProps {
  report: CompatibilityReport | null;
  isChecking: boolean;
  entryCount: number;
  onRecheck: () => void;
}

export function CompatibilitySection({ report, isChecking, entryCount, onRecheck }: CompatibilitySectionProps) {
  const compatExpanded = useWorkspaceStore((s) => s.sectionsCollapsed.compatibility);
  const toggleSection = useWorkspaceStore((s) => s.toggleSection);
  const setMergeMode = useMergeStore((s) => s.setMergeMode);
  const [tableExpanded, setTableExpanded] = useState(false);

  const errors = report?.issues.filter((i) => i.severity === 'error') ?? [];
  const warnings = report?.issues.filter((i) => i.severity === 'warning') ?? [];

  return (
    <div>
      <div className="flex items-center justify-between mb-2">
        <button
          onClick={() => toggleSection('compatibility')}
          className="flex items-center gap-1.5 text-xs font-medium text-text-secondary hover:text-text-primary transition-colors"
          aria-expanded={compatExpanded}
          aria-label="Toggle compatibility check"
        >
          Compatibility Check
          {errors.length > 0 && (
            <Badge variant="danger">{errors.length} error{errors.length > 1 ? 's' : ''}</Badge>
          )}
          {errors.length === 0 && warnings.length > 0 && (
            <Badge variant="warning">{warnings.length} warning{warnings.length > 1 ? 's' : ''}</Badge>
          )}
          {report?.isCompatible && report.issues.length === 0 && (
            <Badge variant="success">OK</Badge>
          )}
        </button>
        <button
          onClick={onRecheck}
          disabled={isChecking}
          className="flex items-center gap-1 text-2xs text-text-muted hover:text-text-secondary transition-colors disabled:opacity-50"
          aria-label={isChecking ? 'Checking compatibility...' : 'Re-check compatibility'}
        >
          <RefreshCw className={cn('w-3 h-3', isChecking && 'animate-spin')} />
          {isChecking ? 'Checking…' : 'Re-check'}
        </button>
      </div>

      <AnimatePresence>
        {compatExpanded && (
          <motion.div
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: 'auto' }}
            exit={{ opacity: 0, height: 0 }}
          >
            {isChecking ? (
              <div className="flex items-center gap-2 p-3 rounded-lg bg-bg-elevated border border-border">
                <Spinner size="xs" className="text-text-muted" />
                <span className="text-xs text-text-muted">Probing {entryCount} files…</span>
              </div>
            ) : report ? (
              <div className={cn(
                'rounded-xl border p-3',
                report.isCompatible ? 'bg-success/5 border-success/20' : 'bg-danger/5 border-danger/20',
              )}>
                <div className="flex items-center gap-2 mb-2">
                  {report.isCompatible
                    ? <CheckCircle2 className="w-4 h-4 text-success shrink-0" />
                    : <AlertTriangle className="w-4 h-4 text-danger shrink-0" />}
                  <span className={cn('text-sm font-semibold', report.isCompatible ? 'text-success' : 'text-danger')}>
                    {report.isCompatible
                      ? 'Files are compatible for lossless merge'
                      : errors.length > 0
                        ? `${errors.length} blocking incompatibilit${errors.length > 1 ? 'ies' : 'y'} found`
                        : `${warnings.length} warning${warnings.length > 1 ? 's' : ''} found`}
                  </span>
                </div>

                {(() => {
                  const entries = usePlaylistStore.getState().entries;
                  const issueCounts: Record<string, number> = {};
                  for (const issue of (report?.issues ?? [])) {
                    issueCounts[issue.kind] = (issueCounts[issue.kind] ?? 0) + 1;
                  }
                  const breakdown = computeCompatibilityBreakdown(entries, issueCounts);
                  if (breakdown.length === 0) return null;
                  const statusColor: Record<string, string> = {
                    ok: 'bg-success/10 border-success/30 text-success',
                    warning: 'bg-warning/10 border-warning/30 text-warning',
                    error: 'bg-danger/10 border-danger/30 text-danger',
                  };
                  return (
                    <div className="flex flex-wrap gap-1.5 mt-2">
                      {breakdown.map((item) => (
                        <div
                          key={item.label}
                          className={cn(
                            'flex items-center gap-1 px-2 py-1 rounded-md border text-[10px] font-medium',
                            statusColor[item.status] ?? 'bg-bg-overlay border-border text-text-muted',
                          )}
                          title={item.values.join(', ')}
                        >
                          <span className="text-text-muted">{item.label}:</span>
                          <span className="truncate max-w-[100px]">{item.values.slice(0, 3).join(', ')}{item.values.length > 3 ? ` +${item.values.length - 3}` : ''}</span>
                        </div>
                      ))}
                    </div>
                  );
                })()}

                {(() => {
                  const entries = usePlaylistStore.getState().entries;
                  const withInfo = entries.filter((e) => e.mediaInfo);
                  if (withInfo.length < 2) return null;

                  // Compute per-file comparison data
                  const computeRow = (e: typeof withInfo[0]) => {
                    const v = e.mediaInfo!.videoStreams?.[0];
                    const a = e.mediaInfo!.audioStreams?.[0];
                    return {
                      name: e.name,
                      videoCodec: v?.codecName ?? '—',
                      resolution: v?.width && v?.height ? `${v.width}×${v.height}` : '—',
                      fps: v?.fps ? formatFps(v.fps) : '—',
                      audioCodec: a?.codecName ?? '—',
                      sampleRate: a?.sampleRate ? String(a.sampleRate) : '—',
                      channels: a?.channels ? String(a.channels) : '—',
                    };
                  };

                  const rows = withInfo.map(computeRow);

                  // Find dominant (most common) value for each property
                  const dominant = <T,>(getVal: (r: typeof rows[0]) => T): T | null => {
                    const freq = new Map<T, number>();
                    for (const r of rows) {
                      const v = getVal(r);
                      freq.set(v, (freq.get(v) ?? 0) + 1);
                    }
                    let best: T | null = null;
                    let bestCount = 0;
                    for (const [v, c] of freq) {
                      if (c > bestCount) { best = v; bestCount = c; }
                    }
                    return best;
                  };

                  const dom = {
                    videoCodec: dominant(r => r.videoCodec),
                    resolution: dominant(r => r.resolution),
                    fps: dominant(r => r.fps),
                    audioCodec: dominant(r => r.audioCodec),
                    sampleRate: dominant(r => r.sampleRate),
                    channels: dominant(r => r.channels),
                  };

                  // Check which columns have any outliers
                  const hasMismatch = {
                    videoCodec: rows.some(r => r.videoCodec !== dom.videoCodec),
                    resolution: rows.some(r => r.resolution !== dom.resolution),
                    fps: rows.some(r => r.fps !== dom.fps),
                    audioCodec: rows.some(r => r.audioCodec !== dom.audioCodec),
                    sampleRate: rows.some(r => r.sampleRate !== dom.sampleRate),
                    channels: rows.some(r => r.channels !== dom.channels),
                  };

                  // Pre-compute mismatch counts to avoid repeated filter() calls
                  const mismatchCounts = {
                    videoCodec: rows.filter(r => r.videoCodec !== dom.videoCodec).length,
                    resolution: rows.filter(r => r.resolution !== dom.resolution).length,
                    fps: rows.filter(r => r.fps !== dom.fps).length,
                    audioCodec: rows.filter(r => r.audioCodec !== dom.audioCodec).length,
                    sampleRate: rows.filter(r => r.sampleRate !== dom.sampleRate).length,
                    channels: rows.filter(r => r.channels !== dom.channels).length,
                  };

                  const mismatchCount = Object.values(hasMismatch).filter(Boolean).length;
                  if (mismatchCount === 0) return null;

                  // Build mismatch summary strings
                  const summaries: string[] = [];
                  if (hasMismatch.videoCodec) summaries.push(`Codec mismatch (${mismatchCounts.videoCodec} of ${rows.length} files)`);
                  if (hasMismatch.resolution) summaries.push(`Resolution mismatch (${mismatchCounts.resolution} of ${rows.length} files)`);
                  if (hasMismatch.fps) summaries.push(`FPS mismatch (${mismatchCounts.fps} of ${rows.length} files)`);
                  if (hasMismatch.audioCodec) summaries.push(`Audio codec mismatch (${mismatchCounts.audioCodec} of ${rows.length} files)`);
                  if (hasMismatch.sampleRate) summaries.push(`Sample rate mismatch (${mismatchCounts.sampleRate} of ${rows.length} files)`);
                  if (hasMismatch.channels) summaries.push(`Channel mismatch (${mismatchCounts.channels} of ${rows.length} files)`);

                  // Table expanded state is managed at the component level

                  return (
                    <div className="mt-2">
                      {/* Mismatch summary chips */}
                      <div className="flex flex-wrap gap-1 mb-2">
                        {summaries.map((s, i) => (
                          <span key={i} className="inline-flex items-center px-1.5 py-0.5 rounded text-[9px] font-medium bg-warning/10 border border-warning/20 text-warning">
                            {s}
                          </span>
                        ))}
                      </div>

                      {/* Expandable table */}
                      <button
                        onClick={() => setTableExpanded(v => !v)}
                        className="flex items-center gap-1 text-[10px] text-text-muted hover:text-text-secondary transition-colors mb-1"
                      >
                        {tableExpanded ? <ChevronUp className="w-3 h-3" /> : <ChevronDown className="w-3 h-3" />}
                        {tableExpanded ? 'Hide file comparison' : 'Show file comparison'}
                      </button>

                      <AnimatePresence>
                        {tableExpanded && (
                          <motion.div
                            initial={{ opacity: 0, height: 0 }}
                            animate={{ opacity: 1, height: 'auto' }}
                            exit={{ opacity: 0, height: 0 }}
                            className="overflow-hidden"
                          >
                            <div className="overflow-x-auto">
                              <table className="w-full text-[9px] border-collapse">
                                <thead>
                                  <tr className="text-text-muted">
                                    <th className="text-left py-1 pr-2 font-semibold uppercase tracking-wider text-[8px]">File</th>
                                    <th className="text-center px-1 py-1 font-semibold uppercase tracking-wider text-[8px]">Video</th>
                                    <th className="text-center px-1 py-1 font-semibold uppercase tracking-wider text-[8px]">Resolution</th>
                                    <th className="text-center px-1 py-1 font-semibold uppercase tracking-wider text-[8px]">FPS</th>
                                    <th className="text-center px-1 py-1 font-semibold uppercase tracking-wider text-[8px]">Audio</th>
                                    <th className="text-center px-1 py-1 font-semibold uppercase tracking-wider text-[8px]">Sample Rate</th>
                                    <th className="text-center px-1 py-1 font-semibold uppercase tracking-wider text-[8px]">Ch</th>
                                  </tr>
                                </thead>
                                <tbody>
                                  {rows.map((r, i) => {
                                    const isOutlier = {
                                      videoCodec: r.videoCodec !== dom.videoCodec,
                                      resolution: r.resolution !== dom.resolution,
                                      fps: r.fps !== dom.fps,
                                      audioCodec: r.audioCodec !== dom.audioCodec,
                                      sampleRate: r.sampleRate !== dom.sampleRate,
                                      channels: r.channels !== dom.channels,
                                    };
                                    const hasAnyOutlier = Object.values(isOutlier).some(Boolean);
                                    return (
                                      <tr
                                        key={i}
                                        className={cn(
                                          'border-t border-border/20 transition-colors',
                                          hasAnyOutlier ? 'bg-warning/5' : 'bg-transparent',
                                          i % 2 === 0 ? '' : 'bg-bg-base/30'
                                        )}
                                      >
                                        <td className="py-1 pr-2 text-text-secondary truncate max-w-[120px]" title={r.name}>{r.name}</td>
                                        <Cell value={r.videoCodec} isOutlier={isOutlier.videoCodec} />
                                        <Cell value={r.resolution} isOutlier={isOutlier.resolution} />
                                        <Cell value={r.fps} isOutlier={isOutlier.fps} />
                                        <Cell value={r.audioCodec} isOutlier={isOutlier.audioCodec} />
                                        <Cell value={r.sampleRate} isOutlier={isOutlier.sampleRate} />
                                        <Cell value={r.channels} isOutlier={isOutlier.channels} />
                                      </tr>
                                    );
                                  })}
                                </tbody>
                              </table>
                            </div>
                          </motion.div>
                        )}
                      </AnimatePresence>
                    </div>
                  );
                })()}

                {(() => {
                  const entries = usePlaylistStore.getState().entries;
                  const count = entries.length;
                  const dur = totalDuration(entries);
                  if (count <= 300 && dur <= 86400) return null;
                  return (
                    <div className="mt-2 px-3 py-2 rounded-lg bg-warning/5 border border-warning/20 flex items-start gap-2">
                      <AlertTriangle className="w-3.5 h-3.5 text-warning shrink-0 mt-0.5" />
                      <div className="flex flex-col gap-0.5">
                        <p className="text-xs text-warning font-medium">Large playlist — deep validation skipped</p>
                        <p className="text-2xs text-warning/80">
                          {count > 300 && `${count} files`}{count > 300 && dur > 86400 ? ' · ' : ''}{dur > 86400 && `${formatDuration(dur)} total`}
                        </p>
                      </div>
                    </div>
                  );
                })()}

                {report.issues.length > 0 && (
                  <div className="flex flex-col gap-1.5 mt-2">
                    {report.issues.map((issue, i) => (
                      <div key={i} className={cn('flex items-start gap-2 p-2 rounded-lg border text-xs', severityBorder[issue.severity])}>
                        <span className={cn('font-bold shrink-0 mt-0.5', severityText[issue.severity])}>
                          {issue.severity === 'error' ? '✕' : '!'}
                        </span>
                        <div>
                          <p className={cn('font-semibold', severityText[issue.severity])}>
                            {issueKindLabel(issue.kind)}
                          </p>
                          <p className="text-text-muted mt-0.5">{issue.description}</p>
                        </div>
                      </div>
                    ))}
                  </div>
                )}

                {!report.isCompatible && (
                  <button
                    onClick={() => setMergeMode('custom')}
                    className="mt-2 w-full flex items-center justify-center gap-2 px-3 py-2 rounded-lg bg-warning/10 border border-warning/30 text-xs text-warning hover:bg-warning/15 transition-colors"
                  >
                    <Sliders className="w-3.5 h-3.5" />
                    Switch to Custom Quality mode to fix these issues
                  </button>
                )}
              </div>
            ) : (
              <div className="p-3 rounded-lg bg-bg-elevated border border-border">
                <p className="text-xs text-text-muted">
                  {entryCount < 2
                    ? 'Add at least 2 files to check compatibility'
                    : 'Press Re-check to verify file compatibility'}
                </p>
              </div>
            )}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

/** Small table cell with outlier highlighting */
function Cell({ value, isOutlier }: { value: string; isOutlier: boolean }) {
  return (
    <td className={cn(
      'text-center px-1 py-1 font-mono text-[9px]',
      isOutlier
        ? 'text-warning font-semibold bg-warning/10 rounded'
        : 'text-text-muted'
    )}>
      {isOutlier ? (
        <span className="inline-flex items-center gap-0.5">
          <span className="text-warning shrink-0">⚠</span>
          <span>{value}</span>
        </span>
      ) : (
        value
      )}
    </td>
  );
}
