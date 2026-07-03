// ────────────────────────────────────────────────
// MergeTaskQueue — Shows all merge jobs with
// progress bars, status indicators, and actions.
// ────────────────────────────────────────────────

import React from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import {
  CheckCircle2, XCircle, StopCircle, FolderOpen, Copy, Trash2,
  List, Layers, ChevronDown,
  FileText, Play, ChevronRight, Clock, Terminal, ScrollText, AlertTriangle
} from 'lucide-react';
import { cn } from '@/utils/cn';
import { Badge, Spinner } from '@/components/ui/Badge';
import { useMergeStore } from '@/store/mergeStore';
import { useAppStore } from '@/store/appStore';
import { useMerge } from '@/hooks/useMerge';
import { tauriCommands } from '@/tauri/commands';
import { formatDuration, formatEta, formatBytes, pathToAssetUrl } from '@/utils';
import type { MergeJob, DiskSpaceInfo, NormalizationPlan } from '@/types';
import { FileProgressCard } from '@/components/ui/FileProgressCard';
import { computePhaseEta } from '@/hooks/usePhaseEta';
import { SmartMkvDashboard } from './SmartMkvDashboard';

// ────────────────────────────────────────────────
// NormalizationProgressList — Shows per-file normalization status
// during the normalizing phase.
// ────────────────────────────────────────────────

interface NormalizationProgressListProps {
  plan: NormalizationPlan;
  currentFileIndex: number | undefined;
  totalFilesInStage: number | undefined;
  normalizationType?: string;
  className?: string;
}

/** Badge color mapping for normalization types */
function getNormBadgeColor(badge: string): string {
  switch (badge) {
    case 'yellow': return 'bg-amber-500/10 text-amber-400 border-amber-500/20';
    case 'blue': return 'bg-blue-500/10 text-blue-400 border-blue-500/20';
    case 'purple': return 'bg-purple-500/10 text-purple-400 border-purple-500/20';
    case 'green': return 'bg-green-500/10 text-green-400 border-green-500/20';
    default: return 'bg-gray-500/10 text-gray-400 border-gray-500/20';
  }
}

/** Status icon for normalization item */
function NormStatusIcon({ status }: { status: 'pending' | 'current' | 'done' | 'error' }) {
  switch (status) {
    case 'done':
      return <CheckCircle2 size={10} className="text-success shrink-0" />;
    case 'current':
      return <div className="w-2.5 h-2.5 rounded-full bg-accent-500 animate-pulse shrink-0" />;
    case 'error':
      return <XCircle size={10} className="text-danger shrink-0" />;
    default:
      return <div className="w-2 h-2 rounded-full bg-text-disabled shrink-0" />;
  }
}

export function NormalizationProgressList({ plan, currentFileIndex, totalFilesInStage, normalizationType, className }: NormalizationProgressListProps) {
  // Only show files that need normalization (badge != 'green')
  const filesNeedingWork = plan.classifications.filter(c => c.badge !== 'green');

  // completedCount = backend's 0-based completed counter (par_done)
  const completedCount = currentFileIndex ?? 0;

  // Calculate iterations per file and cumulative thresholds
  // Files with both inProfile and inAudio need 2 iterations (video then audio)
  // Files with only one need 1 iteration
  const fileIterations = filesNeedingWork.map(c => {
    if (c.inProfile && c.inAudio) return 2; // A+V: 2 backend iterations
    return 1; // Audio-only or Video-only: 1 iteration
  });

  // Calculate cumulative iteration threshold for each file
  // A file is "done" when completedCount > threshold + iterations - 1
  // A file is "current" when threshold <= completedCount < threshold + iterations
  type FileStatus = 'pending' | 'current' | 'done';
  const fileStatuses: FileStatus[] = [];
  let cumulativeThreshold = 0;

  for (let i = 0; i < filesNeedingWork.length; i++) {
    const iterations = fileIterations[i];
    const fileThreshold = cumulativeThreshold;

    if (completedCount >= fileThreshold + iterations) {
      fileStatuses.push('done');
    } else if (completedCount >= fileThreshold && completedCount < fileThreshold + iterations) {
      fileStatuses.push('current');
    } else {
      fileStatuses.push('pending');
    }

    cumulativeThreshold += iterations;
  }

  // Total work items = sum of all iterations (not just file count)
  const totalWorkItems = totalFilesInStage ?? cumulativeThreshold;

  return (
    <div className={cn('rounded-lg border border-border/20 bg-bg-base/40 overflow-hidden', className)}>
      {/* Header */}
      <div className="px-3 py-2 bg-bg-overlay/40 border-b border-border/10">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <span className="text-[10px] font-semibold text-text-primary uppercase tracking-wider">
              Normalization
            </span>
            {totalWorkItems > 0 && (
              <span className="text-[9px] text-text-muted">
                {completedCount}/{totalWorkItems}
              </span>
            )}
          </div>
          {normalizationType && (
            <span className="text-[9px] text-accent-400 truncate max-w-[120px]" title={normalizationType}>
              {normalizationType}
            </span>
          )}
        </div>

        {/* Summary counts */}
        {plan.normalCount > 0 || plan.audioOnlyCount > 0 || plan.videoOnlyCount > 0 || plan.audioVideoCount > 0 ? (
          <div className="flex items-center gap-2 mt-1.5 text-[9px]">
            {plan.normalCount > 0 && (
              <span className="flex items-center gap-1 text-green-400">
                <span className="w-1.5 h-1.5 rounded-full bg-green-500" />
                {plan.normalCount}
              </span>
            )}
            {plan.audioOnlyCount > 0 && (
              <span className="flex items-center gap-1 text-amber-400">
                <span className="w-1.5 h-1.5 rounded-full bg-amber-500" />
                {plan.audioOnlyCount} Audio
              </span>
            )}
            {plan.videoOnlyCount > 0 && (
              <span className="flex items-center gap-1 text-blue-400">
                <span className="w-1.5 h-1.5 rounded-full bg-blue-500" />
                {plan.videoOnlyCount} Video
              </span>
            )}
            {plan.audioVideoCount > 0 && (
              <span className="flex items-center gap-1 text-purple-400">
                <span className="w-1.5 h-1.5 rounded-full bg-purple-500" />
                {plan.audioVideoCount} A+V
              </span>
            )}
          </div>
        ) : null}
      </div>

      {/* File list - only show files that need normalization work */}
      {filesNeedingWork.length > 0 && (
        <div className="max-h-32 overflow-y-auto divide-y divide-border/5">
          {filesNeedingWork.slice(0, 50).map((item, displayIdx) => {
            const status = fileStatuses[displayIdx] ?? 'pending';

            return (
              <div
                key={item.index}
                className={cn(
                  'flex items-center gap-2 px-3 py-1.5 text-[10px]',
                  status === 'current' && 'bg-accent-500/5',
                  status === 'done' && 'opacity-60',
                )}
              >
                <NormStatusIcon status={status} />
                <span className={cn(
                  'w-4 text-right shrink-0',
                  status === 'pending' && 'text-text-disabled',
                  status === 'current' && 'text-accent-400',
                  status === 'done' && 'text-success',
                )}>
                  {displayIdx + 1}
                </span>
                <span className={cn(
                  'flex-1 truncate',
                  status === 'current' && 'text-text-primary font-medium',
                  status === 'pending' && 'text-text-secondary',
                  status === 'done' && 'text-text-muted line-through',
                )}
                  title={item.filename}
                >
                  {item.filename}
                </span>
                <span className={cn(
                  'px-1.5 py-0.5 rounded text-[8px] border',
                  getNormBadgeColor(item.badge)
                )}>
                  {item.type}
                </span>
              </div>
            );
          })}
          {filesNeedingWork.length > 50 && (
            <div className="px-3 py-1 text-[9px] text-text-disabled text-center">
              ... and {filesNeedingWork.length - 50} more files
            </div>
          )}
        </div>
      )}

      {/* Empty state - all files normal */}
      {filesNeedingWork.length === 0 && plan.normalCount > 0 && (
        <div className="px-3 py-3 text-[10px] text-text-muted text-center">
          All {plan.normalCount} files are healthy — no normalization needed
        </div>
      )}
    </div>
  );
}

function getInterleavedTimeline(
  inputFiles: string[],
  inputDurations: number[],
  cardConfig?: { duration: number }
) {
  const timeline: {
    type: 'video' | 'card';
    originalIndex: number;
    duration: number;
    startTime: number;
    endTime: number;
  }[] = [];

  const hasCards = cardConfig && cardConfig.duration > 0 && inputFiles.length >= 2;
  let currentStart = 0;

  for (let i = 0; i < inputFiles.length; i++) {
    const videoDuration = inputDurations[i] ?? 0;
    timeline.push({
      type: 'video',
      originalIndex: i,
      duration: videoDuration,
      startTime: currentStart,
      endTime: currentStart + videoDuration,
    });
    currentStart += videoDuration;

    if (hasCards && i < inputFiles.length - 1) {
      const cardDuration = cardConfig.duration;
      timeline.push({
        type: 'card',
        originalIndex: i,
        duration: cardDuration,
        startTime: currentStart,
        endTime: currentStart + cardDuration,
      });
      currentStart += cardDuration;
    }
  }

  return timeline;
}

// eslint-disable-next-line @typescript-eslint/no-empty-object-type
interface MergeTaskQueueProps {}

export function MergeTaskQueue(_props: MergeTaskQueueProps) {
  const jobs = useMergeStore((s) => s.jobs);
  const { cancelMerge } = useMerge();
  const setScreen = useAppStore((s) => s.setScreen);

  const hasActiveJob = jobs.some(j =>
    ['writing', 'preparing', 'probing', 'validating', 'normalizing', 'finalizing'].includes(j.progress.phase)
  );

  return (
    <div className="bg-bg-elevated border border-border rounded-xl p-4 flex flex-col gap-3">
      {/* Header */}
      <div className="flex items-center justify-between w-full">
        <div className="flex items-center gap-2 text-left flex-1 min-w-0">
          <List className="w-4 h-4 text-text-muted shrink-0" />
          <p className="text-xs font-semibold text-text-secondary">Merge Tasks</p>
          {jobs.length > 0 && (
            <Badge variant={hasActiveJob ? 'info' : 'default'}>
              {jobs.length}
            </Badge>
          )}
        </div>
      </div>

      <div className="overflow-hidden">
        <div className="mt-3 space-y-3">
              {jobs.length === 0 ? (
                <div className="flex flex-col items-center gap-2 py-5 px-3">
                  <div className="w-9 h-9 rounded-xl bg-bg-overlay border border-border/60 flex items-center justify-center">
                    <Layers className="w-4 h-4 text-text-disabled" />
                  </div>
                  <div className="text-center">
                    <p className="text-xs font-medium text-text-muted">No parallel merge tasks</p>
                    <p className="text-[10px] text-text-disabled mt-0.5">
                      Use{' '}
                      <button
                        onClick={() => setScreen('playlist')}
                        className="font-semibold text-accent-400 hover:text-accent-300 hover:underline transition-colors focus:outline-none"
                      >
                        + New Merge
                      </button>{' '}
                      to queue a folder or playlist
                    </p>
                  </div>
                </div>
              ) : (
                jobs.map((j) => (
                  <MergeTaskCard
                    key={j.id}
                    job={j}
                    onCancel={() => cancelMerge(j.id)}
                    onRemove={() => useMergeStore.getState().removeJob(j.id)}
                    onReveal={() => {
                      tauriCommands.revealInExplorer(j.result?.outputPath || j.request.outputPath).catch(console.error);
                    }}
                    onCopyPath={() => {
                      navigator.clipboard.writeText(j.result?.outputPath || j.request.outputPath).then(() => {
                        useAppStore.getState().showToast({ type: 'success', title: 'Path copied to clipboard' });
                      }).catch(() => {});
                    }}
                  />
                ))
              )}
        </div>
      </div>
    </div>
  );
}

// ─── Individual Task Card ────────────────────────

interface MergeTaskCardProps {
  job: MergeJob;
  onCancel: () => void;
  onRemove: () => void;
  onReveal: () => void;
  onCopyPath: () => void;
}

/** Represents a unified output part for display — works for both single and split outputs */
interface UnifiedPart {
  partLabel: string;       // e.g. "Output" or "Part 1 / 3"
  outputPath: string;
  outputSizeBytes: number;
  totalDuration: number;
  reportPath: string | null;
  /** Segments that belong to this part, with times local to the part's output file */
  segments: Array<{
    name: string;
    duration: number;
    localStart: number;  // start time within the part video
    localEnd: number;    // end time within the part video
    remaining: number;   // remaining duration in the part video after this segment
    thumbnail: string | null;  // path to generated thumbnail for this segment's source file
    /** Whether this segment is a canvas overlay card */
    isCard?: boolean;
    /** Card background color (only set when isCard is true) */
    cardColor?: string | null;
    parentFolder?: string;
  }>;
}

function buildUnifiedParts(job: MergeJob): UnifiedPart[] {
  const result = job.result;
  if (!result) return [];

  const allSegs = result.segments ?? [];
  const reportPaths = result.reportPaths ?? [];
  const thumbnails = job.request.inputThumbnails ?? [];

  // ── Split merge ──────────────────────────────────────────────
  if (result.parts && result.parts.length > 0) {
    const parts: UnifiedPart[] = [];
    let segOffset = 0;
    const total = result.parts.length;

    for (let pi = 0; pi < result.parts.length; pi++) {
      const part = result.parts[pi];
      const count = part.fileCount;
      const partSegs = allSegs.slice(segOffset, segOffset + count);

      let localCursor = 0.0;
      let localRemaining = part.totalDuration;
      const mappedSegs = partSegs.map((s, si) => {
        localRemaining -= s.duration;
        const localStart = localCursor;
        const localEnd = localCursor + s.duration;
        localCursor = localEnd;
        return {
          name: s.name,
          duration: s.duration,
          localStart,
          localEnd,
          remaining: Math.max(0, localRemaining),
          thumbnail: thumbnails[segOffset + si] ?? null,
          isCard: s.isCard ?? false,
          cardColor: s.cardColor ?? null,
          parentFolder: s.parentFolder,
        };
      });

      parts.push({
        partLabel: `Part ${pi + 1} / ${total}`,
        outputPath: part.outputPath,
        outputSizeBytes: part.outputSizeBytes,
        totalDuration: part.totalDuration,
        reportPath: reportPaths[pi] ?? null,
        segments: mappedSegs,
      });

      segOffset += count;
    }
    return parts;
  }

  // ── Single merge ─────────────────────────────────────────────
  // Compute actual total duration from segments (includes card durations if any)
  const actualTotalDur = allSegs.reduce((s, seg) => s + seg.duration, 0);
  // Prefer segment sum; fall back to backend-probed output duration, then request total
  const totalDur = actualTotalDur > 0
    ? actualTotalDur
    : (result.outputDurationSecs ?? job.request.totalDuration);
  let localCursor = 0.0;
  let localRemaining = totalDur;
  const mappedSegs = allSegs.map((s, si) => {
    localRemaining -= s.duration;
    const localStart = localCursor;
    const localEnd = localCursor + s.duration;
    localCursor = localEnd;
    return {
      name: s.name,
      duration: s.duration,
      localStart,
      localEnd,
      remaining: Math.max(0, localRemaining),
      thumbnail: thumbnails[si] ?? null,
      isCard: s.isCard ?? false,
      cardColor: s.cardColor ?? null,
      parentFolder: s.parentFolder,
    };
  });

  return [{
    partLabel: 'Output',
    outputPath: result.outputPath,
    outputSizeBytes: result.outputSizeBytes,
    totalDuration: totalDur,
    reportPath: reportPaths[0] ?? null,
    segments: mappedSegs,
  }];
}

const MergeTaskCard = React.memo(function MergeTaskCard({
  job, onCancel, onRemove, onReveal, onCopyPath,
}: MergeTaskCardProps) {
  const progress = job.progress;
  const phase = progress.phase;
  const isDone = phase === 'complete';
  const isFailed = phase === 'failed';
  const isCancelled = phase === 'cancelled';
  const isActive = ['writing', 'preparing', 'probing', 'validating', 'normalizing', 'finalizing'].includes(phase);

  const [cardExpanded, setCardExpanded] = React.useState(true);
  const [filesExpanded, setFilesExpanded] = React.useState(isActive);
  const [diskSpace, setDiskSpace] = React.useState<DiskSpaceInfo | null>(null);
  // Track which parts are expanded in the detail view
  const [expandedParts, setExpandedParts] = React.useState<Set<number>>(() => new Set([0]));
  const [logExpanded, setLogExpanded] = React.useState(false);
  const [reportExpanded, setReportExpanded] = React.useState(false);

  // Move clearLogs to component level (hooks must be called at top level, not inside IIFEs)
  const clearLogs = useMergeStore((s) => s.clearLogs);
  const fileProgress = useMergeStore((s) => s.fileProgress[job.id]);

  const recentLogs = React.useMemo(() => job.logs?.slice(-20) ?? [], [job.logs]);

  // ── Per-file normalization result type ──
  interface NormFileResult {
    status: 'normal' | 'unknown' | 'audio_repair' | 'video_normalize' | 'audio_video';
    label: string;
    normalizationType?: string;
    repairReason?: string;
  }

  // Status ranking: higher number = more impactful. Only escalate, never downgrade.
  // Defined outside effect via useMemo to avoid stale closure / missing dep issues.
  const statusRank = React.useMemo(() => ({
    normal: 0,
    unknown: 0,
    audio_repair: 1,
    video_normalize: 1,
    audio_video: 2,
  }), []);

  // Ref to accumulate per-file normalization results as progress events arrive
  const normResultsRef = React.useRef<Map<number, NormFileResult>>(new Map());
  const filledNormRef = React.useRef(false);
  const prevActiveRef = React.useRef(false);
  const [, setNormTick] = React.useState(0); // forces re-render after ref mutation

  // Track normalization results from progress events
  React.useEffect(() => {
    const fileCount = job.request.inputFiles.length;

    // Cleanup: when transitioning from active to inactive, clear ref for next merge
    if (prevActiveRef.current && !isActive && normResultsRef.current.size > 0) {
      normResultsRef.current = new Map();
      filledNormRef.current = false;
      setNormTick(t => t + 1);
    }
    prevActiveRef.current = isActive;

    if (!isActive) return;

    let didUpdate = false;

    // ── Helper: map Rust NormalizationType string to UI status ──
    const normTypeToResult = (nt: string, repairReason?: string): NormFileResult => {
      switch (nt) {
        case 'AudioReencode': return { status: 'audio_repair', label: 'Audio', normalizationType: nt, repairReason };
        case 'VideoReencode': return { status: 'video_normalize', label: 'Video', normalizationType: nt, repairReason };
        case 'RemuxOnly':     return { status: 'video_normalize', label: 'Video (Remux)', normalizationType: nt, repairReason };
        case 'FullReencode':  return { status: 'audio_video', label: 'Audio + Video', normalizationType: nt, repairReason };
        case 'None':          return { status: 'normal', label: 'Normal', normalizationType: nt };
        default: {
          // Fallback: infer from name (handles unknown future enum variants safely)
          const lower = nt.toLowerCase();
          const hasAudio = lower.includes('audio');
          const hasVideo = lower.includes('video');
          if (hasAudio && hasVideo) return { status: 'audio_video', label: 'Audio + Video', normalizationType: nt, repairReason };
          if (hasAudio) return { status: 'audio_repair', label: 'Audio', normalizationType: nt, repairReason };
          if (hasVideo) return { status: 'video_normalize', label: 'Video', normalizationType: nt, repairReason };
          return { status: 'unknown', label: 'Unknown', normalizationType: nt };
        }
      }
    };

    // Capture normalization type during normalizing phase
    // NOTE: `currentFileIndex` is the outlier processing counter (1..N), NOT the original
    // file index in the playlist. Cross-reference by `currentFile` (filename) against
    // `inputNames` to get the correct original file index for accurate badge placement.
    if (progress.phase === 'normalizing' && progress.normalizationType && progress.currentFile) {
      const actualIdx = job.request.inputNames.indexOf(progress.currentFile);
      if (actualIdx >= 0) {
        const newResult = normTypeToResult(progress.normalizationType, progress.repairReason);
        const existing = normResultsRef.current.get(actualIdx);
        const newRank = statusRank[newResult.status] ?? 0;
        const oldRank = existing ? (statusRank[existing.status] ?? 0) : -1;
        // Only escalate: never overwrite with a lower-priority status
        if (newRank > oldRank) {
          normResultsRef.current.set(actualIdx, newResult);
          didUpdate = true;
        }
      }
    }

    // When transitioning to writing/finalizing/complete, fill unmarked files as Normal
    if ((progress.phase === 'writing' || progress.phase === 'finalizing' || progress.phase === 'complete') && !filledNormRef.current) {
      filledNormRef.current = true;
      for (let i = 0; i < fileCount; i++) {
        if (!normResultsRef.current.has(i)) {
          normResultsRef.current.set(i, { status: 'normal', label: 'Normal' });
          didUpdate = true;
        }
      }
    }

    // Force re-render so the UI picks up ref changes immediately
    if (didUpdate) setNormTick(t => t + 1);
  }, [progress, isActive, job.request.inputFiles.length, job.request.inputNames, job.id, statusRank]);

  // Local ticker to force re-render every second to update elapsed time ticker
  const [, setTick] = React.useState(0);
  React.useEffect(() => {
    if (!isActive) return;
    const interval = setInterval(() => {
      setTick(t => t + 1);
    }, 1000);
    return () => clearInterval(interval);
  }, [isActive]);

  // EMA rate tracking for stable ETA (component-level, persists across renders)
  const rateRef = React.useRef(0);
  const prevIdxRef = React.useRef<number>(0);
  const prevTimeRef = React.useRef<number>(Date.now());
  // Log auto-scroll: track if user has manually scrolled away from bottom
  const logScrollRef = React.useRef<HTMLDivElement>(null);
  const autoScrollRef = React.useRef(true);

  const interleavedTimeline = React.useMemo(() => {
    return getInterleavedTimeline(
      job.request.inputFiles,
      job.request.inputDurations,
      job.request.cardConfig
    );
  }, [job.request.inputFiles, job.request.inputDurations, job.request.cardConfig]);

  // Pre-compute Map from originalIndex → video segment index
  // Eliminates O(n) findIndex inside O(n) map, reducing complexity from O(n²) to O(n)
  const videoSegmentIndexMap = React.useMemo(() => {
    const map = new Map<number, number>();
    interleavedTimeline.forEach((item, idx) => {
      if (item.type === 'video') {
        map.set(item.originalIndex, idx);
      }
    });
    return map;
  }, [interleavedTimeline]);

  React.useEffect(() => {
    const fetchSpace = async () => {
      try {
        const path = job.request.outputPath;
        if (path) {
          const space = await tauriCommands.getDiskSpace(path);
          setDiskSpace(space);
        }
      } catch (err) {
        console.error('Failed to get disk space:', err);
      }
    };
    fetchSpace();
  }, [job.request.outputPath]);

  // Keep expanded when task becomes active
  React.useEffect(() => {
    if (isActive) {
      setCardExpanded(true);
      setFilesExpanded(true);
    }
    if (isDone) {
      setFilesExpanded(true);
      setExpandedParts(new Set([0]));
    }
  }, [isActive, isDone]);

  const togglePart = (idx: number) => {
    setExpandedParts(prev => {
      const next = new Set(prev);
      if (next.has(idx)) { next.delete(idx); } else { next.add(idx); }
      return next;
    });
  };

  const unifiedParts = React.useMemo(() => isDone ? buildUnifiedParts(job) : [], [isDone, job]);

  return (
    <div className="bg-bg-overlay border border-border rounded-lg p-3 flex flex-col gap-2 relative overflow-hidden transition-all hover:border-border-strong">
      {/* Task Header */}
      <div className="flex items-start justify-between gap-3 min-w-0">
        <div className="flex items-start gap-2.5 min-w-0 flex-1">
          {/* Collapse Toggle */}
          <button
            onClick={() => setCardExpanded(v => !v)}
            className="mt-0.5 p-0.5 rounded text-text-disabled hover:text-text-secondary transition-colors shrink-0"
            title={cardExpanded ? 'Collapse' : 'Expand'}
            aria-label={cardExpanded ? 'Collapse task details' : 'Expand task details'}
          >
            {cardExpanded ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />}
          </button>
          {/* Status Indicator */}
          <div className="mt-0.5 shrink-0">
            {isActive && <Spinner size="xs" className="text-accent-400" />}
            {isDone && <CheckCircle2 className="w-4 h-4 text-success" />}
            {isFailed && <XCircle className="w-4 h-4 text-danger" />}
            {isCancelled && <StopCircle className="w-4 h-4 text-text-disabled" />}
          </div>
          <div className="min-w-0">
            <p className="text-xs font-semibold text-text-primary truncate" title={job.request.outputPath}>
              {job.request.outputPath.replace(/\\/g, '/').split('/').pop() || 'merged_output'}
            </p>
            <p className="text-[10px] text-text-muted mt-0.5">
              {job.request.mode === 'lossless' ? 'Lossless' 
                : job.request.mode === 'custom' ? 'Custom' 
                : job.request.mode === 'fastMkv' ? 'Fast MKV' 
                : job.request.mode === 'smartMkv' ? 'Smart MKV' 
                : job.request.mode} · {job.request.inputFiles.length} files · {formatDuration(job.request.totalDuration)}
            </p>
          </div>
        </div>

        {/* Actions */}
        <div className="flex items-center gap-1 shrink-0">
            {isActive && (
            <button
              onClick={onCancel}
              className="p-1 rounded text-text-muted hover:text-danger hover:bg-danger/10 transition-colors"
              title="Cancel Merge"
              aria-label="Cancel merge"
            >
              <StopCircle className="w-3.5 h-3.5" />
            </button>
          )}
          {isDone && (
            <>
              <button
                onClick={() => {
                  const outDir = (job.result?.outputPath || job.request.outputPath).replace(/\\/g, '/').split('/').slice(0, -1).join('/');
                  tauriCommands.openLogsFolder(outDir).catch(console.error);
                }}
                className="p-1 rounded text-text-muted hover:text-accent-400 hover:bg-accent-muted transition-colors"
                title="Open Logs Folder"
                aria-label="Open logs folder"
              >
                <ScrollText className="w-3.5 h-3.5" />
              </button>
              <button
                onClick={async () => {
                  try {
                    const outDir = (job.result?.outputPath || job.request.outputPath).replace(/\\/g, '/').split('/').slice(0, -1).join('/');
                    const logFiles = await tauriCommands.getJobLogFiles(outDir);
                    if (logFiles.length > 0) {
                      const content = await tauriCommands.readJobLog(logFiles[0].path);
                      await navigator.clipboard.writeText(content);
                      useAppStore.getState().showToast({ type: 'success', title: 'Job log copied to clipboard' });
                    } else {
                      useAppStore.getState().showToast({ type: 'warning', title: 'No log files found' });
                    }
                  } catch (err) {
                    console.error('Failed to export job log:', err);
                    useAppStore.getState().showToast({ type: 'error', title: 'Failed to export log' });
                  }
                }}
                className="p-1 rounded text-text-muted hover:text-accent-400 hover:bg-accent-muted transition-colors"
                title="Export Job Log (copy to clipboard)"
                aria-label="Export job log"
              >
                <Copy className="w-3.5 h-3.5" />
              </button>
              <button
                onClick={onReveal}
                className="p-1 rounded text-text-muted hover:text-text-primary hover:bg-bg-elevated transition-colors"
                title="Reveal in Explorer"
                aria-label="Reveal in file explorer"
              >
                <FolderOpen className="w-3.5 h-3.5" />
              </button>
              <button
                onClick={onCopyPath}
                className="p-1 rounded text-text-muted hover:text-text-primary hover:bg-bg-elevated transition-colors"
                title="Copy Path"
                aria-label="Copy output path"
              >
                <Copy className="w-3.5 h-3.5" />
              </button>
            </>
          )}
          {!isActive && (
            <button
              onClick={onRemove}
              className="p-1 rounded text-text-disabled hover:text-danger hover:bg-danger/10 transition-colors"
              title="Clear Task"
              aria-label="Remove task"
            >
              <Trash2 className="w-3.5 h-3.5" />
            </button>
          )}
        </div>
      </div>

      {/* Body (collapsible) */}
      {cardExpanded && (
        <>
      {/* Progress Details */}
      {isActive && (() => {
        const currentFileIdx = progress.currentFileIndex ?? 0;
        const elapsedSecs = job.startedAt ? Math.floor((Date.now() - job.startedAt) / 1000) : 0;
        // Prefer backend-emitted values; fall back to computed
        const currentFileName = progress.currentFile
          || (job.request.inputNames?.[currentFileIdx])
          || (job.request.inputFiles?.[currentFileIdx]?.replace(/\\/g, '/').split('/').pop())
          || '';
        // Use backend ETA if available; otherwise compute stable EMA-based ETA
        (() => {
          const now = Date.now();
          const dt = now - prevTimeRef.current;
          const idx = progress.currentFileIndex ?? 0;
          const dIdx = idx - prevIdxRef.current;
          if (dt > 100 && dIdx >= 0) {
            const instantRate = dIdx / (dt / 1000);
            // EMA: 25% weight on new sample for responsiveness
            rateRef.current = rateRef.current * 0.75 + instantRate * 0.25;
          }
          prevIdxRef.current = idx;
          prevTimeRef.current = now;
        })();

        // Phase-weighted ETA with historical learning
        const history = useAppStore.getState().settings?.mergeStatsHistory ?? [];
        const phaseEta = computePhaseEta({
          jobStartedAt: job.startedAt,
          phase: phase,
          phaseTimes: (job.phaseTimes ?? {}) as Partial<Record<string, number>>,
          inputFiles: job.request.inputFiles,
          totalDuration: job.request.totalDuration,
          mode: job.request.mode,
          audioRepairMode: job.request.audioRepairMode ?? 'smart',
          backendEtaSeconds: progress.etaSeconds,
          history,
        });

        // Handle copy logs
        const handleCopyLogs = () => {
          const header = `Job: ${job.id}\nMerge Mode: ${job.request.mode}\n\n`;
          const lines = recentLogs.map(l =>
            `${new Date(l.timestamp).toLocaleTimeString('en-US', { hour12: false })}  ${l.level === 'error' ? '✗' : l.level === 'warn' ? '⚠' : '✓'}  ${l.message}`
          );
          navigator.clipboard.writeText(header + lines.join('\n')).then(() => {
            useAppStore.getState().showToast({ type: 'success', title: 'Logs copied to clipboard' });
          }).catch(() => {});
        };

        return (
        <div className="mt-2.5 rounded-lg border border-border/40 bg-bg-base/40 overflow-hidden">
          {/* Current Operation Header */}
          <div className="px-3 py-2.5 bg-bg-overlay/60 border-b border-border/20">
            <div className="flex items-center justify-between mb-1.5">
              <div className="flex items-center gap-1.5">
                <span className="inline-flex w-2 h-2 rounded-full bg-accent-500 animate-pulse animate-duration-1000" />
                <span className="text-[11px] font-semibold text-text-primary uppercase tracking-wider">
                  {phase === 'probing' && 'Analysing Files'}
                  {phase === 'validating' && 'Validating Files'}
                  {phase === 'preparing' && 'Preparing Files'}
                  {phase === 'normalizing' && 'Normalising Files'}
                  {phase === 'writing' && 'Merging Timeline'}
                  {phase === 'finalizing' && 'Finalising Output'}
                </span>
              </div>
              <div className="flex items-center gap-2 text-[10px] font-mono">
                <span className="text-text-muted flex items-center gap-1">
                  <Clock size={9} />
                  {formatDuration(elapsedSecs)}
                </span>
                <span className="font-bold text-text-primary">
                  {Math.round(progress.overallPercent ?? progress.percent ?? 0)}%
                </span>
              </div>
            </div>

            {/* Current File Display */}
            {phase === 'validating' && fileProgress ? (
              <FileProgressCard progress={fileProgress} className="mb-2" />
            ) : currentFileName ? (
              <div className="flex items-center gap-2 mb-2">
                <div className="flex-1 min-w-0">
                  <p className="text-[10px] text-text-muted mb-0.5">Currently Processing</p>
                  <p className="text-xs font-semibold text-accent-300 truncate" title={currentFileName}>
                    {currentFileName}
                  </p>
                </div>
              </div>
            ) : null}

            {/* Phase 2: Normalization Dashboard */}
            {phase === 'normalizing' && progress.normalizationPlan && (
              <NormalizationProgressList
                plan={progress.normalizationPlan}
                currentFileIndex={progress.currentFileIndex}
                totalFilesInStage={progress.totalFilesInStage}
                normalizationType={progress.normalizationType}
                className="mb-2"
              />
            )}

            {/* Phase 5: Smart MKV Analysis Dashboard */}
            {phase === 'normalizing' && progress.smartMkvBreakdown && (
              <SmartMkvDashboard
                breakdown={progress.smartMkvBreakdown}
                className="mb-2"
              />
            )}

            {/* Stats Row — Phase ETA with historical learning */}
            <div className="grid grid-cols-2 gap-2 text-[10px]">
              {/* Elapsed + Phase Progress */}
              <div className="bg-bg-base/60 rounded px-2 py-1.5 border border-border/10">
                <p className="text-text-muted mb-0.5">Elapsed</p>
                <p className="font-mono font-semibold text-text-primary">
                  {formatDuration(elapsedSecs)}
                </p>
                <div className="mt-1 h-1 bg-bg-elevated rounded-full overflow-hidden">
                  <div
                    className="h-full bg-accent-500/60 rounded-full transition-all duration-500"
                    style={{ width: `${Math.min(100, phaseEta.phaseProgress * 100)}%` }}
                  />
                </div>
                <p className="text-[9px] text-text-muted mt-0.5">
                  {phaseEta.phaseLabel}{phaseEta.phaseProgress > 0 && phaseEta.phaseProgress < 1 ? ` · ${Math.round(phaseEta.phaseProgress * 100)}%` : ''}
                </p>
              </div>
              {/* Estimated Remaining + Completion */}
              <div className="bg-bg-base/60 rounded px-2 py-1.5 border border-border/10">
                <p className="text-text-muted mb-0.5">ETA</p>
                <p className="font-mono font-semibold text-accent-400">
                  {phaseEta.estimatedRemainingSeconds > 0 ? formatEta(phaseEta.estimatedRemainingSeconds) : '—'}
                </p>
                {phaseEta.estimatedRemainingSeconds > 0 && (
                  <p className="text-[9px] text-text-muted mt-0.5">
                    {new Date(phaseEta.estimatedCompletionMs).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                  </p>
                )}
                {phaseEta.confidence === 'low' && phaseEta.estimatedRemainingSeconds > 0 && (
                  <p className="text-[8px] text-text-disabled mt-0.5">≈ estimate</p>
                )}
              </div>
            </div>
          </div>

          {/* Progress Bar */}
          <div className="px-3 py-2">
            <div className="h-2 bg-bg-elevated rounded-full overflow-hidden border border-border/10 shadow-inner" role="progressbar" aria-valuenow={Math.round(progress.overallPercent ?? progress.percent ?? 0)} aria-valuemin={0} aria-valuemax={100}>
              <div
                className="h-full bg-gradient-to-r from-accent-500 via-indigo-500 to-highlight rounded-full transition-all duration-300 shadow-glow-sm"
                style={{ width: `${progress.overallPercent ?? progress.percent ?? 0}%` }}
              />
            </div>
            {/* Stage progress bar */}
            {progress.stagePercent !== undefined && progress.stagePercent > 0 && (
              <div className="mt-1.5">
                <div className="flex justify-between text-[9px] text-text-muted font-mono mb-0.5">
                  <span>Stage</span>
                  <span>{Math.round(progress.stagePercent)}%</span>
                </div>
                <div className="h-1 bg-bg-base rounded-full overflow-hidden border border-border/10">
                  <div
                    className="h-full bg-warning/80 rounded-full transition-all duration-300"
                    style={{ width: `${progress.stagePercent ?? 0}%` }}
                  />
                </div>
              </div>
            )}
          </div>

          {/* Warning/Large Playlist Banner */}
          {progress.warning && (
            <div className="mx-3 mb-2 flex items-start gap-2 px-2.5 py-1.5 rounded-lg border border-amber-500/20 bg-amber-500/5 text-[10px]">
              <span className="text-amber-400 shrink-0 mt-0.5">⚠️</span>
              <span className="text-amber-300 font-semibold">{progress.warning}</span>
            </div>
          )}
          {progress.isLargePlaylist && !progress.warning && (
            <div className="mx-3 mb-2 flex items-center gap-1.5 px-2.5 py-1 rounded-lg border border-accent-500/15 bg-accent-500/5 text-[10px]">
              <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[9px] font-bold uppercase tracking-wider bg-accent-500/10 text-accent-400 shrink-0">
                Large Playlist
              </span>
              <span className="text-accent-300/80">Fast Validation ✓</span>
            </div>
          )}

          {/* Live Log Section */}
          {recentLogs.length > 0 && (
            <div className="border-t border-border/20">
              <div className="flex items-center px-3 py-1.5">
                <button
                  onClick={() => setLogExpanded(v => !v)}
                  className="flex-1 flex items-center gap-1.5 text-[10px] text-text-muted hover:text-text-primary transition-colors"
                >
                  <Terminal size={10} className="text-accent-500" />
                  Live Log
                  <span className="text-text-disabled">({recentLogs.length})</span>
                  {logExpanded ? <ChevronDown size={10} /> : <ChevronRight size={10} />}
                </button>
                <div className="flex items-center gap-1">
                  <button
                    onClick={handleCopyLogs}
                    className="p-1 rounded text-text-muted hover:text-text-primary hover:bg-bg-overlay/40 transition-colors"
                    title="Copy logs"
                  >
                    <Copy size={10} />
                  </button>
                  <button
                    onClick={() => clearLogs(job.id)}
                    className="p-1 rounded text-text-muted hover:text-danger hover:bg-danger/10 transition-colors"
                    title="Clear logs"
                  >
                    <Trash2 size={10} />
                  </button>
                </div>
              </div>
              {logExpanded && (
                <div
                  ref={logScrollRef}
                  onScroll={(e) => {
                    const el = e.currentTarget;
                    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 10;
                    autoScrollRef.current = atBottom;
                  }}
                  className="max-h-32 overflow-y-auto px-3 pb-2 space-y-0.5"
                >
                  {recentLogs.map((log, i) => (
                    <div key={i} className="flex items-start gap-2 text-[9px] py-0.5">
                      <span className="text-text-disabled shrink-0 font-mono">
                        {new Date(log.timestamp).toLocaleTimeString('en-US', { hour12: false })}
                      </span>
                      <span className={cn(
                        'shrink-0',
                        log.level === 'error' && 'text-danger',
                        log.level === 'warn' && 'text-amber-400',
                        log.level === 'info' && 'text-success',
                        log.level === 'debug' && 'text-text-muted',
                      )}>
                        {log.level === 'error' && '✗'}
                        {log.level === 'warn' && '⚠'}
                        {log.level === 'info' && '✓'}
                        {log.level === 'debug' && '◦'}
                      </span>
                      <span className={cn(
                        'flex-1 truncate',
                        log.level === 'error' && 'text-danger/80',
                        log.level === 'warn' && 'text-amber-300/80',
                        log.level === 'info' && 'text-text-secondary',
                        log.level === 'debug' && 'text-text-disabled',
                      )}>
                        {log.message}
                      </span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
        );
      })()}

      {/* Fail/Error details */}
      {isFailed && job.error && (
        <p className="text-[10px] text-danger/80 border border-danger/10 bg-danger/5 rounded px-2 py-1 leading-normal max-h-16 overflow-y-auto mt-1">
          {job.error}
        </p>
      )}

      {/* ── Phase 3: Normalization Completion Summary ── */}
      {isDone && job.result?.audioRepairSummary && (
        <div className="mt-2 px-3 py-2 rounded-lg border border-success/20 bg-success/5">
          <p className="text-[9px] font-semibold text-success uppercase tracking-wider mb-1.5">
            Normalization Complete
          </p>
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-[10px]">
            {job.result.audioRepairSummary.filesRepaired > 0 ? (
              <>
                <span className="text-text-secondary">
                  <span className="text-success font-semibold">{job.result.audioRepairSummary.filesRepaired}</span> of{' '}
                  {job.result.audioRepairSummary.totalFiles} files repaired
                </span>
                {job.result.audioRepairSummary.dueToCorruption > 0 && (
                  <span className="text-amber-400">🔧 {job.result.audioRepairSummary.dueToCorruption} corruption</span>
                )}
                {job.result.audioRepairSummary.dueToProfileMismatch > 0 && (
                  <span className="text-blue-400">🔧 {job.result.audioRepairSummary.dueToProfileMismatch} profile</span>
                )}
                {job.result.audioRepairSummary.dueToSafeMode > 0 && (
                  <span className="text-purple-400">🔧 {job.result.audioRepairSummary.dueToSafeMode} safe-mode</span>
                )}
              </>
            ) : (
              <span className="text-text-muted">
                {job.result.audioRepairSummary.totalFiles} files — no repairs needed
              </span>
            )}
          </div>
        </div>
      )}

      {/* ── Phase 3b: Subtitle Warning Summary ── */}
      {isDone && job.subtitleWarnings && job.subtitleWarnings.length > 0 && (
        <div className="mt-2 px-3 py-2 rounded-lg border border-amber-500/20 bg-amber-500/5">
          <p className="text-[9px] font-semibold text-amber-400 uppercase tracking-wider mb-1.5 flex items-center gap-1">
            <AlertTriangle className="w-3 h-3" />
            Subtitles Skipped
          </p>
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-[10px]">
            <span className="text-text-secondary">
              <span className="text-amber-400 font-semibold">{job.subtitleWarnings.length}</span> of{' '}
              {job.request.inputFiles.length} files — subtitles could not be extracted
            </span>
          </div>
          <div className="mt-1.5 text-[9px] text-text-muted max-h-16 overflow-y-auto space-y-0.5">
            {job.subtitleWarnings.slice(0, 5).map((w, i) => (
              <div key={i} className="truncate" title={w.reason}>
                <span className="text-amber-300">•</span>{' '}
                <span className="text-text-secondary/80">{w.filePath.split(/[/\\]/).pop() ?? w.filePath}</span>
                <span className="text-text-muted/60 ml-1">— {w.reason}</span>
              </div>
            ))}
            {job.subtitleWarnings.length > 5 && (
              <div className="text-text-muted/60 italic">
                +{job.subtitleWarnings.length - 5} more warnings
              </div>
            )}
          </div>
        </div>
      )}

      {/* ── Completed: Detailed Merge Report ── */}
      {isDone && unifiedParts.length > 0 && (
        <div className="mt-2 border-t border-border/40 pt-2 flex flex-col gap-1.5">
          <button
            onClick={() => setFilesExpanded(v => !v)}
            className="flex items-center justify-between w-full text-left"
          >
            <span className="text-[10px] font-semibold text-text-muted uppercase tracking-wider flex items-center gap-1.5">
              <FileText className="w-3 h-3 text-success" />
              Merge Report · {unifiedParts.length > 1 ? `${unifiedParts.length} parts` : `${unifiedParts[0]?.segments.length ?? 0} files`}
            </span>
            <span className="text-[9px] text-accent-400 hover:text-accent-300">
              {filesExpanded ? 'Hide' : 'Show'}
            </span>
          </button>

          <AnimatePresence>
            {filesExpanded && (
              <motion.div
                initial={{ opacity: 0, height: 0 }}
                animate={{ opacity: 1, height: 'auto' }}
                exit={{ opacity: 0, height: 0 }}
                className="overflow-hidden"
              >
                <div className="space-y-2 mt-1">
                  {unifiedParts.map((part, pi) => (
                    <div key={pi} className="rounded-lg border border-border/60 bg-bg-base/40 overflow-hidden">
                      {/* Part header row */}
                      <div
                        className="flex items-center justify-between px-2.5 py-2 cursor-pointer hover:bg-bg-elevated/50 transition-colors group"
                        onClick={() => togglePart(pi)}
                      >
                        <div className="flex items-center gap-2 min-w-0">
                          <ChevronRight
                            className={cn(
                              'w-3 h-3 text-text-muted shrink-0 transition-transform duration-150',
                              expandedParts.has(pi) && 'rotate-90'
                            )}
                          />
                          <div className="min-w-0">
                            <p className="text-[10px] font-semibold text-text-secondary truncate" title={part.outputPath}>
                              {part.partLabel !== 'Output'
                                ? <span className="text-accent-400 mr-1">[{part.partLabel}]</span>
                                : null}
                              {part.outputPath.replace(/\\/g, '/').split('/').pop()}
                            </p>
                            <p className="text-[9px] text-text-muted mt-0.5">
                              {formatDuration(part.totalDuration)} · {formatBytes(part.outputSizeBytes)} · {part.segments.length} files
                            </p>
                          </div>
                        </div>
                        {/* Part actions */}
                        <div className="flex items-center gap-1 shrink-0 opacity-0 group-hover:opacity-100 transition-opacity">
                          <button
                            onClick={e => { e.stopPropagation(); tauriCommands.openWithDefault(part.outputPath).catch(console.error); }}
                            className="p-1 rounded text-text-muted hover:text-accent-400 hover:bg-accent-muted transition-colors"
                            title="Play Video"
                            aria-label="Play merged video"
                          >
                            <Play className="w-3 h-3" />
                          </button>
                          <button
                            onClick={e => { e.stopPropagation(); tauriCommands.revealInExplorer(part.outputPath).catch(console.error); }}
                            className="p-1 rounded text-text-muted hover:text-text-primary hover:bg-bg-elevated transition-colors"
                            title="Reveal in Explorer"
                            aria-label="Reveal in explorer"
                          >
                            <FolderOpen className="w-3 h-3" />
                          </button>
                          {part.reportPath && (
                            <button
                              onClick={e => { e.stopPropagation(); tauriCommands.openWithDefault(part.reportPath!).catch(console.error); }}
                              className="p-1 rounded text-text-muted hover:text-success hover:bg-success/10 transition-colors"
                              title="Open Merge Report"
                              aria-label="Open merge report file"
                            >
                              <FileText className="w-3 h-3" />
                            </button>
                          )}
                          <button
                            onClick={e => { e.stopPropagation(); navigator.clipboard.writeText(part.outputPath).then(() => { useAppStore.getState().showToast({ type: 'success', title: 'Path copied' }); }).catch(() => {}); }}
                            className="p-1 rounded text-text-muted hover:text-text-primary hover:bg-bg-elevated transition-colors"
                            title="Copy Path"
                            aria-label="Copy output path"
                          >
                            <Copy className="w-3 h-3" />
                          </button>
                        </div>
                      </div>

                      {/* Segment timeline */}
                      <AnimatePresence>
                        {expandedParts.has(pi) && part.segments.length > 0 && (
                          <motion.div
                            initial={{ opacity: 0, height: 0 }}
                            animate={{ opacity: 1, height: 'auto' }}
                            exit={{ opacity: 0, height: 0 }}
                            className="overflow-hidden"
                          >
                            {/* Column headers */}
                            <div className="grid text-[8px] font-semibold uppercase tracking-wider text-text-disabled border-t border-border/30 px-2.5 py-1 bg-bg-overlay/60"
                              style={{ gridTemplateColumns: '1.5rem 2rem 1fr 5.5rem 6.5rem 5rem' }}>
                              <span>#</span>
                              <span />
                              <span>Source File</span>
                              <span className="text-right">Duration</span>
                              <span className="text-center">Start → End</span>
                              <span className="text-right">Remaining</span>
                            </div>
                            {/* Segment rows */}
                            <div className="max-h-52 overflow-y-auto scrollbar-thin">
                              {(() => {
                                let currentFolder: string | undefined = undefined;
                                return part.segments.map((seg, si) => {
                                  const isCard = seg.isCard ?? false;
                                  const cardColor = seg.cardColor ?? null;
                                  const thumbnailSrc = seg.thumbnail ? pathToAssetUrl(seg.thumbnail) : null;

                                  let folderHeader = null;
                                  if (seg.parentFolder) {
                                    if (seg.parentFolder !== currentFolder) {
                                      currentFolder = seg.parentFolder;
                                      folderHeader = (
                                        <div key={`folder-${currentFolder}`} className="px-3 py-1 bg-bg-surface border-t border-b border-border/20 text-[9px] font-bold text-text-secondary flex items-center gap-1.5 select-none">
                                          <span>📁</span>
                                          <span>{currentFolder}</span>
                                        </div>
                                      );
                                    }
                                  } else if (currentFolder !== undefined) {
                                    currentFolder = undefined;
                                    folderHeader = (
                                      <div key="folder-root" className="px-3 py-1 bg-bg-surface border-t border-b border-border/20 text-[9px] font-bold text-text-secondary flex items-center gap-1.5 select-none">
                                        <span>📁</span>
                                        <span>Root / Other</span>
                                      </div>
                                    );
                                  }

                                  return (
                                    <React.Fragment key={si}>
                                      {folderHeader}
                                      <div
                                        className={cn(
                                          'grid items-center px-2.5 py-1.5 border-t border-border/20 transition-colors',
                                          si % 2 === 0 ? 'bg-transparent' : 'bg-bg-base/20',
                                          isCard && 'bg-accent-500/5'
                                        )}
                                        style={{ gridTemplateColumns: '1.5rem 2rem 1fr 5.5rem 6.5rem 5rem' }}
                                      >
                                  {/* Index */}
                                  <span className="text-[9px] font-mono text-text-disabled">
                                    {String(si + 1).padStart(2, '0')}
                                  </span>
                                  {/* Thumbnail: show card swatch or video thumbnail */}
                                  <div className="flex items-center justify-center">
                                    {isCard && cardColor ? (
                                      <div
                                        className="w-8 h-[18px] rounded border border-border/40 flex items-center justify-center text-white/80"
                                        style={{ backgroundColor: cardColor }}
                                      >
                                        <span className="text-[7px] font-bold">
                                          ◼
                                        </span>
                                      </div>
                                    ) : thumbnailSrc ? (
                                      <img
                                        src={thumbnailSrc}
                                        alt=""
                                        className="w-8 h-[18px] rounded object-cover border border-border/40 bg-bg-base"
                                        loading="lazy"
                                        onError={(e) => {
                                          // If thumbnail fails to load, show fallback icon
                                          e.currentTarget.style.display = 'none';
                                          const fallback = e.currentTarget.nextElementSibling;
                                          if (fallback) fallback.classList.remove('hidden');
                                        }}
                                      />
                                    ) : null}
                                    {!thumbnailSrc && (
                                      <div className="w-8 h-[18px] rounded bg-bg-base border border-border/20 flex items-center justify-center">
                                        <span className="text-[8px] text-text-disabled">🎬</span>
                                      </div>
                                    )}
                                  </div>
                                  {/* Name */}
                                  <span className="text-[10px] text-text-secondary truncate pr-2 flex items-center gap-1" title={seg.name}>
                                    {isCard ? (
                                      <span className="inline-flex items-center gap-1 px-1 py-0.5 rounded text-[7px] font-bold uppercase tracking-wider bg-accent-500/10 text-accent-400">
                                        Canvas
                                      </span>
                                    ) : (
                                      <span className="text-success mr-1 shrink-0">✓</span>
                                    )}
                                    <span className={isCard ? 'text-accent-400 font-medium' : ''}>
                                      {seg.name}
                                    </span>
                                  </span>
                                  {/* Duration */}
                                  <span className={cn(
                                    'text-[9px] font-mono text-right',
                                    isCard ? 'text-accent-400/70' : 'text-text-muted'
                                  )}>
                                    {formatDuration(seg.duration)}
                                  </span>
                                  {/* Start → End */}
                                  <span className="text-[9px] font-mono text-accent-400/80 text-center">
                                    {formatDuration(seg.localStart)}&nbsp;<span className="text-text-disabled">→</span>&nbsp;{formatDuration(seg.localEnd)}
                                  </span>
                                  {/* Remaining */}
                                  <span className={cn(
                                    'text-[9px] font-mono text-right',
                                    seg.remaining === 0 ? 'text-success' : 'text-text-muted'
                                  )}>
                                    {seg.remaining === 0 ? '—' : formatDuration(seg.remaining)}
                                  </span>
                                  </div>
                                </React.Fragment>
                              );
                            })})()}
                            </div>
                          </motion.div>
                        )}
                      </AnimatePresence>
                    </div>
                  ))}
                </div>
              </motion.div>
            )}
          </AnimatePresence>
        </div>
      )}

      {/* ── Normalization Report ── */}
      {(() => {
        if (normResultsRef.current.size === 0) return null;

        const counts = { normal: 0, unknown: 0, audio_repair: 0, video_normalize: 0, audio_video: 0 };
        const perType: Record<string, NormFileResult[]> = { normal: [], unknown: [], audio_repair: [], video_normalize: [], audio_video: [] };
        for (const [, res] of normResultsRef.current.entries()) {
          counts[res.status]++;
          perType[res.status].push(res);
        }
        const totalNorm = counts.normal + counts.unknown + counts.audio_repair + counts.video_normalize + counts.audio_video;

        const statusConfig: Record<string, { icon: string; color: string; label: string }> = {
          normal: { icon: '🟢', color: 'text-success', label: 'Normal' },
          unknown: { icon: '⚪', color: 'text-text-muted', label: 'Unknown' },
          audio_repair: { icon: '🟡', color: 'text-amber-400', label: 'Audio Repair' },
          video_normalize: { icon: '🔵', color: 'text-blue-400', label: 'Video Normalize' },
          audio_video: { icon: '🟣', color: 'text-purple-400', label: 'Audio + Video' },
        };

        return (
          <div className="mt-2 border-t border-border/40 pt-2 flex flex-col gap-1.5">
            <button
              onClick={() => setReportExpanded(v => !v)}
              className="flex items-center justify-between w-full text-left"
            >
              <span className="text-[10px] font-semibold text-text-muted uppercase tracking-wider flex items-center gap-1.5">
                <List className="w-3 h-3 text-accent-400" />
                Normalization Report
              </span>
              <span className="text-[9px] text-accent-400 hover:text-accent-300">
                {reportExpanded ? 'Hide' : `Show (${totalNorm} files)`}
              </span>
            </button>

            {/* Summary pills */}
            <div className="flex flex-wrap gap-1.5">
              {Object.entries(counts).filter(([, c]) => c > 0).map(([key, count]) => {
                const cfg = statusConfig[key];
                if (!cfg) return null;
                return (
                  <span
                    key={key}
                    className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[9px] font-semibold bg-bg-base border border-border/40"
                  >
                    <span>{cfg.icon}</span>
                    <span className={cfg.color}>{count}</span>
                    <span className="text-text-muted">{cfg.label}</span>
                  </span>
                );
              })}
            </div>

            {/* Expandable per-file breakdown */}
            <AnimatePresence>
              {reportExpanded && (
                <motion.div
                  initial={{ opacity: 0, height: 0 }}
                  animate={{ opacity: 1, height: 'auto' }}
                  exit={{ opacity: 0, height: 0 }}
                  className="overflow-hidden"
                >
                  <div className="max-h-48 overflow-y-auto space-y-1 scrollbar-thin">
                    {Object.entries(perType).filter(([, items]) => items.length > 0).map(([statusKey, items]) => {
                      const cfg = statusConfig[statusKey];
                      if (!cfg) return null;
                      return (
                        <div key={statusKey} className="flex flex-col gap-0.5">
                          <p className="text-[9px] font-semibold text-text-muted uppercase tracking-wider px-1 pt-1 pb-0.5">
                            {cfg.icon} {cfg.label} ({items.length})
                          </p>
                          {items.map((item, i) => (
                            <div key={i} className="flex items-center gap-2 px-2 py-0.5 rounded text-[10px]">
                              <span className="text-text-muted font-mono w-6 text-right shrink-0">{String(i + 1).padStart(2, '0')}</span>
                              <span className="truncate text-text-secondary">
                                {job.request.inputNames?.[i] || job.request.inputFiles?.[i]?.replace(/\\/g, '/').split('/').pop() || `File ${i + 1}`}
                              </span>
                              {item.normalizationType && (
                                <span className="text-[8px] text-text-disabled shrink-0 ml-auto font-mono">{item.normalizationType}</span>
                              )}
                            </div>
                          ))}
                        </div>
                      );
                    })}
                  </div>
                </motion.div>
              )}
            </AnimatePresence>
          </div>
        );
      })()}

      {/* ── In-Progress: File Checklist Procession ── */}
      {!isDone && job.request.inputFiles && job.request.inputFiles.length > 0 && (
        <div className="mt-2 border-t border-border/40 pt-2 flex flex-col gap-1.5">
          <button
            onClick={() => setFilesExpanded(v => !v)}
            className="flex items-center justify-between w-full text-left"
          >
            <span className="text-[10px] font-semibold text-text-muted uppercase tracking-wider flex items-center gap-1.5">
              <List className="w-3 h-3 text-accent-400" />
              Merge Procession
            </span>
            <div className="flex items-center gap-1.5">
              {diskSpace && (
                <span className="text-[9px] text-text-muted">
                  Free: {formatBytes(diskSpace.availableBytes)}
                </span>
              )}
              <span className="text-[9px] text-accent-400 hover:text-accent-300">
                {filesExpanded ? 'Hide' : `Show (${job.request.inputFiles.length})`}
              </span>
            </div>
          </button>

          <AnimatePresence>
            {filesExpanded && (
              <motion.div
                initial={{ opacity: 0, height: 0 }}
                animate={{ opacity: 1, height: 'auto' }}
                exit={{ opacity: 0, height: 0 }}
                className="overflow-hidden"
              >
                <div className="max-h-56 overflow-y-auto space-y-2.5 scrollbar-thin pr-1 mt-1">
                  {job.request.inputFiles.map((file, idx) => {
                    const name = job.request.inputNames?.[idx] || file.replace(/\\/g, '/').split('/').pop() || '';
                    const duration = job.request.inputDurations?.[idx];

                    // Determine individual progress and status
                    let filePercent = 0;
                    let status: 'done' | 'active' | 'pending' = 'pending';
                    let detailText = '';

                    if (isDone) {
                      filePercent = 100;
                      status = 'done';
                    } else if (isFailed || isCancelled) {
                      filePercent = 0;
                      status = 'pending';
                    } else if (isActive) {
                      // currentIdx is used by 'preparing' and 'writing' phases
                      const currentIdx = progress.currentSegmentIndex ?? 0;
                      if (phase === 'validating') {
                        // Validate phase emits currentFileIndex (0..N) — use it directly
                        const validateIdx = progress.currentFileIndex ?? 0;
                        if (idx < validateIdx) {
                          filePercent = 100;
                          status = 'done';
                        } else if (idx === validateIdx) {
                          filePercent = progress.stagePercent ?? 50;
                          status = 'active';
                          detailText = progress.stageName || 'Validating...';
                        } else {
                          filePercent = 0;
                          status = 'pending';
                        }
                      } else if (phase === 'preparing') {
                        if (idx < currentIdx) {
                          filePercent = 100;
                          status = 'done';
                        } else if (idx === currentIdx) {
                          filePercent = progress.stagePercent ?? 50;
                          status = 'active';
                          detailText = progress.stageName || 'Preparing...';
                        } else {
                          filePercent = 0;
                          status = 'pending';
                        }
                      } else if (phase === 'normalizing') {
                        // Normalizing emits outlier index (1..N), not original file index.
                        // Show stage progress count but keep all files as pending.
                        // Stage-level info displayed in the progress bar above handles this.
                        filePercent = 0;
                        status = 'pending';
                      } else if (phase === 'probing') {
                        filePercent = 0;
                        if (idx === 0) {
                          status = 'active';
                          detailText = 'Analysing...';
                        } else {
                          status = 'pending';
                        }
                      } else if (phase === 'writing') {
                        const targetSegIdx = videoSegmentIndexMap.get(idx) ?? -1;
                        if (targetSegIdx !== -1) {
                          if (currentIdx > targetSegIdx) {
                            filePercent = 100;
                            status = 'done';
                          } else if (currentIdx < targetSegIdx) {
                            filePercent = 0;
                            status = 'pending';
                          } else {
                            status = 'active';
                            const item = interleavedTimeline[targetSegIdx];
                            const fileElapsed = progress.currentTime - item.startTime;
                            filePercent = Math.min(100, Math.max(0, (fileElapsed / item.duration) * 100));
                            detailText = `Merging · ${Math.round(filePercent)}%`;
                          }
                        }
                      } else if (phase === 'finalizing') {
                        filePercent = 100;
                        status = 'done';
                        detailText = progress.stageName || 'Finalising Output';
                      }
                    }

                    return (
                      <div
                        key={idx}
                        className={cn(
                          'flex flex-col gap-1.5 text-[11px] px-3 py-2 rounded-lg transition-all border border-transparent bg-bg-base/30 hover:bg-bg-base/50',
                          status === 'active' && 'bg-accent-500/5 border-accent-500/20 text-accent-300 shadow-glow-sm',
                          status === 'done' && 'text-text-secondary bg-bg-base/15',
                          status === 'pending' && 'text-text-muted opacity-60'
                        )}
                      >
                        {/* Top Row: Meta and Title */}
                        <div className="flex items-center justify-between gap-3 min-w-0">
                          <div className="flex items-center gap-2 min-w-0">
                            <span className="text-[9px] font-mono text-text-muted w-3.5 shrink-0">
                              {String(idx + 1).padStart(2, '0')}.
                            </span>
                            {status === 'done' && (
                              <span className="text-success font-semibold shrink-0">✓</span>
                            )}
                            {status === 'active' && (
                              <Spinner size="xs" className={cn(
                                'shrink-0',
                                phase === 'preparing' ? 'text-warning' : 'text-accent-400'
                              )} />
                            )}
                            {status === 'pending' && (
                              <span className="inline-block w-1.5 h-1.5 rounded-full bg-text-disabled shrink-0" />
                            )}
                            {/* Normalization color badge */}
                            {(() => {
                              const normRes = normResultsRef.current.get(idx);
                              if (!normRes || normRes.status === 'normal') return null;
                              const cfg: Record<string, { color: string; text: string; dot: string }> = {
                                unknown: { color: 'bg-text-muted/15 text-text-muted border-text-muted/25', text: 'UNKNOWN', dot: '⚪' },
                                audio_repair: { color: 'bg-amber-400/15 text-amber-400 border-amber-500/25', text: 'AUDIO', dot: '🟡' },
                                video_normalize: { color: 'bg-blue-400/15 text-blue-400 border-blue-500/25', text: 'VIDEO', dot: '🔵' },
                                audio_video: { color: 'bg-purple-400/15 text-purple-400 border-purple-500/25', text: 'AUDIO+VIDEO', dot: '🟣' },
                              };
                              const c = cfg[normRes.status];
                              if (!c) return null;
                              return (
                                <span className={cn(
                                  'inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[7px] font-bold uppercase tracking-wider shrink-0 border',
                                  c.color
                                )}>
                                  <span className="text-[8px]">{c.dot}</span>
                                  {c.text}
                                </span>
                              );
                            })()}
                            <span className="truncate font-medium text-text-primary" title={file}>
                              {name}
                            </span>
                          </div>

                          <div className="flex items-center gap-2 shrink-0 text-[10px] font-mono">
                            {detailText && (
                              <span className={cn(
                                'text-[9px] uppercase tracking-wider font-bold',
                                status === 'active' && phase === 'preparing' ? 'text-warning' : 'text-accent-400'
                              )}>
                                {detailText}
                              </span>
                            )}
                            {duration > 0 && (
                              <span className="text-text-muted">
                                {formatDuration(duration)}
                              </span>
                            )}
                          </div>
                        </div>

                        {/* Bottom Row: Sleek Progress Bar */}
                        <div className="h-1 bg-bg-elevated rounded-full overflow-hidden relative w-full border border-border/5">
                          {status === 'active' && phase === 'preparing' ? (
                            <div className="h-full bg-gradient-to-r from-warning via-amber-400 to-warning rounded-full animate-progress-indeterminate w-[30%]" />
                          ) : (
                            <div
                              className={cn(
                                'h-full rounded-full transition-all duration-300',
                                status === 'done' && 'bg-success',
                                status === 'active' && 'bg-gradient-to-r from-accent-500 to-indigo-400 shadow-glow-sm',
                                status === 'pending' && 'bg-transparent'
                              )}
                              style={{ width: `${filePercent}%` }}
                            />
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              </motion.div>
            )}
          </AnimatePresence>
        </div>
      )}
      </>)}
    </div>
  );
});
