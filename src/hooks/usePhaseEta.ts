import { useMemo } from 'react';
import { useMergeStore } from '@/store/mergeStore';
import { useAppStore } from '@/store/appStore';
import type { MergePhase, MergeStatsRecord } from '@/types';

export interface PhaseEtaResult {
  elapsedSeconds: number;
  estimatedRemainingSeconds: number;
  estimatedTotalSeconds: number;
  estimatedCompletionMs: number;
  confidence: 'low' | 'medium' | 'high';
  phaseLabel: string;
  /** How much of the current phase has been completed (0-1) */
  phaseProgress: number;
}

/** Phase weights represent typical time distribution across phases for a merge.
 * These are used to estimate remaining time when historical data is unavailable. */
const PHASE_WEIGHTS: Record<Exclude<MergePhase, 'complete' | 'failed' | 'cancelled'>, number> = {
  probing: 0.05,
  validating: 0.25,
  preparing: 0.05,
  normalizing: 0.35,
  writing: 0.25,
  finalizing: 0.05,
};

const PHASE_LABELS: Record<MergePhase, string> = {
  probing: 'Analysing',
  validating: 'Validating',
  preparing: 'Preparing',
  normalizing: 'Normalising',
  writing: 'Merging',
  finalizing: 'Finalising',
  complete: 'Complete',
  failed: 'Failed',
  cancelled: 'Cancelled',
};

/** Compute similarity score between a current job and a historical record.
 * Higher score = more similar = better predictor. */
function computeSimilarity(
  files: number,
  totalDuration: number,
  mode: string,
  audioRepairMode: string,
  record: MergeStatsRecord,
): number {
  let score = 0;
  // File count similarity (most important) — weight: 4
  const fileDiff = Math.abs(files - record.files);
  if (fileDiff === 0) score += 4;
  else if (fileDiff <= 5) score += 3;
  else if (fileDiff <= 20) score += 2;
  else if (fileDiff <= 50) score += 1;

  // Total media duration similarity — weight: 3
  const durDiff = Math.abs(totalDuration - record.totalMediaDurationSeconds);
  const durPct = durDiff / Math.max(record.totalMediaDurationSeconds, 1);
  if (durPct < 0.05) score += 3;
  else if (durPct < 0.15) score += 2;
  else if (durPct < 0.3) score += 1;

  // Mode match — weight: 2
  if (record.mode === mode) score += 2;

  // Audio repair mode match — weight: 1
  if (record.audioRepairMode === audioRepairMode) score += 1;

  return score;
}

function findSimilarRecords(
  files: number,
  totalDuration: number,
  mode: string,
  audioRepairMode: string,
  history: MergeStatsRecord[],
): MergeStatsRecord[] {
  if (history.length === 0) return [];
  const scored = history.map(r => ({
    r,
    score: computeSimilarity(files, totalDuration, mode, audioRepairMode, r),
  }));
  scored.sort((a, b) => b.score - a.score);
  return scored.slice(0, 5).filter(s => s.score > 0).map(s => s.r);
}

function estimateTotalFromHistory(
  files: number,
  totalDuration: number,
  mode: string,
  audioRepairMode: string,
  history: MergeStatsRecord[],
): number | null {
  const similar = findSimilarRecords(files, totalDuration, mode, audioRepairMode, history);
  if (similar.length === 0) return null;
  // Weighted average by similarity score
  let totalScore = 0;
  let weightedSum = 0;
  for (const r of similar) {
    const s = computeSimilarity(files, totalDuration, mode, audioRepairMode, r);
    if (s > 0) {
      totalScore += s;
      weightedSum += r.totalTimeSeconds * s;
    }
  }
  if (totalScore === 0) return null;
  return weightedSum / totalScore;
}

/** Phase-weighted ETA calculator — pure function, no React dependencies.
 * Can be called from any context (hooks, IIFEs, event handlers). */
export function computePhaseEta(params: {
  jobStartedAt: number;
  phase: MergePhase;
  phaseTimes: Partial<Record<string, number>>;
  inputFiles: { length: number } | undefined;
  totalDuration: number;
  mode: string;
  audioRepairMode: string;
  backendEtaSeconds: number | undefined;
  history: MergeStatsRecord[];
}): PhaseEtaResult {
  const {
    jobStartedAt, phase, phaseTimes,
    inputFiles, totalDuration, mode, audioRepairMode,
    backendEtaSeconds, history,
  } = params;

  const now = Date.now();
  const elapsedMs = now - jobStartedAt;
  const elapsedSeconds = Math.floor(elapsedMs / 1000);

  let estimatedTotalSeconds: number;
  let confidence: 'low' | 'medium' | 'high' = 'low';

  if (backendEtaSeconds !== undefined && backendEtaSeconds > 0) {
    estimatedTotalSeconds = elapsedSeconds + backendEtaSeconds;
    confidence = 'medium';
  } else {
    const historicalEstimate = estimateTotalFromHistory(
      inputFiles?.length ?? 0,
      totalDuration,
      mode,
      audioRepairMode,
      history,
    );

    if (historicalEstimate !== null && historicalEstimate > 0) {
      estimatedTotalSeconds = historicalEstimate;
      confidence = history.length >= 5 ? 'high' : 'medium';
    } else {
      const modeMultiplier = mode === 'custom' ? 5 : 1;
      estimatedTotalSeconds = Math.max(
        (totalDuration / 3600) * 180 * modeMultiplier,
        30,
      );
    }
  }

  let phaseProgress = 0;
  let estimatedRemainingSeconds = 0;

  if (phase === 'complete' || phase === 'failed' || phase === 'cancelled') {
    estimatedRemainingSeconds = 0;
    phaseProgress = 1;
  } else {
    const phaseStartTime = phaseTimes[phase] ?? jobStartedAt;
    const phaseElapsedSec = Math.max(0, (now - phaseStartTime) / 1000);
    const phaseWeight = PHASE_WEIGHTS[phase as Exclude<MergePhase, 'complete' | 'failed' | 'cancelled'>] ?? 0.1;
    const phaseEstimated = phaseWeight * estimatedTotalSeconds;
    estimatedRemainingSeconds = Math.max(0, phaseEstimated - phaseElapsedSec);
    if (phaseEstimated > 0) {
      phaseProgress = Math.min(1, phaseElapsedSec / phaseEstimated);
    }
  }

  const estimatedCompletionMs = now + estimatedRemainingSeconds * 1000;

  return {
    elapsedSeconds,
    estimatedRemainingSeconds,
    estimatedTotalSeconds,
    estimatedCompletionMs,
    confidence,
    phaseLabel: PHASE_LABELS[phase] ?? phase,
    phaseProgress,
  };
}

/** usePhaseEta — computes elapsed time, estimated remaining, and estimated completion
 * for a merge job using phase-weighted estimation with historical learning.
 *
 * Uses backend ETA when available, otherwise falls back to:
 * 1. Historical similarity (best) — finds similar completed merges
 * 2. Phase-weighted estimation (fallback) — uses phase progress to estimate */
export function usePhaseEta(
  jobId: string,
  phase: MergePhase,
  startedAt: number,
  backendEtaSeconds?: number,
): PhaseEtaResult {
  const job = useMergeStore((s) => s.jobs.find(j => j.id === jobId));
  const history = useAppStore((s) => s.settings?.mergeStatsHistory ?? []);

  return useMemo(() => {
    const now = Date.now();
    const elapsedMs = now - startedAt;
    const elapsedSeconds = Math.floor(elapsedMs / 1000);
    const phaseTimes = job?.phaseTimes ?? {};

    // Determine estimated total
    let estimatedTotalSeconds: number;
    let confidence: 'low' | 'medium' | 'high' = 'low';

    if (backendEtaSeconds !== undefined && backendEtaSeconds > 0) {
      // Backend ETA is available — use it but scale by phase progress
      estimatedTotalSeconds = elapsedSeconds + backendEtaSeconds;
      confidence = 'medium';
    } else {
      // Try historical similarity
      const historicalEstimate = estimateTotalFromHistory(
        job?.request.inputFiles.length ?? 0,
        job?.request.totalDuration ?? 0,
        job?.request.mode ?? 'lossless',
        job?.request.audioRepairMode ?? 'smart',
        history,
      );

      if (historicalEstimate !== null && historicalEstimate > 0) {
        estimatedTotalSeconds = historicalEstimate;
        confidence = history.length >= 5 ? 'high' : 'medium';
      } else {
        // Fallback: use phase-weight-based estimation
        const totalMediaDuration = job?.request.totalDuration ?? 0;
        // Custom mode is ~3-10x slower than lossless
        const modeMultiplier = job?.request.mode === 'custom' ? 5 : 1;
        // Estimate total = media duration * multiplier * overall speed factor
        // A 1-hour lossless merge typically takes ~2-5 minutes for validation/normalization/concat
        estimatedTotalSeconds = Math.max(
          (totalMediaDuration / 3600) * 180 * modeMultiplier,
          30,
        );
      }
    }

    // Compute phase progress and remaining
    let phaseProgress = 0;
    let estimatedRemainingSeconds = 0;

    if (phase === 'complete' || phase === 'failed' || phase === 'cancelled') {
      estimatedRemainingSeconds = 0;
      phaseProgress = 1;
    } else {
      const phaseStartTime = phaseTimes[phase] ?? startedAt;
      const phaseElapsedSec = Math.max(0, (now - phaseStartTime) / 1000);
      const phaseWeight = PHASE_WEIGHTS[phase] ?? 0.1;

      // Estimate remaining using phase progress
      // If this phase typically takes X seconds (weight * estimatedTotal) and
      // we've been in it for Y seconds, remaining ≈ X - Y
      const phaseEstimated = phaseWeight * estimatedTotalSeconds;
      estimatedRemainingSeconds = Math.max(0, phaseEstimated - phaseElapsedSec);

      // Compute phase progress (0-1) for display
      if (phaseEstimated > 0) {
        phaseProgress = Math.min(1, phaseElapsedSec / phaseEstimated);
      }
    }

    const estimatedCompletionMs = now + estimatedRemainingSeconds * 1000;

    return {
      elapsedSeconds,
      estimatedRemainingSeconds,
      estimatedTotalSeconds,
      estimatedCompletionMs,
      confidence,
      phaseLabel: PHASE_LABELS[phase] ?? phase,
      phaseProgress,
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [job, phase, startedAt, backendEtaSeconds, history.length]);
}