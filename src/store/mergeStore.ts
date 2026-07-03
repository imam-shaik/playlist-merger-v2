import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type { MergeJob, MergeRequest, MergeProgress, MergeResult, MergeMode, AudioRepairMode, LargePlaylistStrategy, SplitConfig, SubtitleMode, NamingConfig, MergeLogEntry, MergeFileProgress, RecoveryCheckpoint, RepeatConfig, CardFrequency } from '@/types';
import { DEFAULT_REPEAT_CONFIG } from '@/types';
import { getContrastFontColor } from '@/utils';
import { MERGE_DEFAULTS, CANVAS_COLORS } from '@/constants';

interface MergeState {
  activeJob: MergeJob | null;
  jobs: MergeJob[];
  // Output config
  outputPath: string;
  outputFilename: string;
  // Mode
  mergeMode: MergeMode;
  audioRepairMode: AudioRepairMode;
  /** Strategy chosen for large playlist audio validation (Smart mode only). */
  largePlaylistStrategy: LargePlaylistStrategy | null;
  /** Fast MKV: convert merged MKV to MP4 after creation */
  convertToMp4: boolean;
  // Custom mode options
  videoCodec: string;
  audioCodec: string;
  videoCrf: number;
  videoPreset: string;
  audioBitrate: string;
  targetResolution: string;
  targetFps: string;
  hwAccel: string;
  // Repeat / Extend options
  repeatConfig: RepeatConfig;
  // Split options
  splitConfig: SplitConfig;
  namingConfig: NamingConfig;
  // Canvas overlay cards
  cardColor: string;
  cardFontColor: string;
  cardDuration: number;
  cardShowInReport: boolean;
  cardEnabled: boolean;
  cardFrequency: CardFrequency;

  // Multi-playlist merge
  selectedPlaylistIds: string[];
  // Compat check
  compatibilityReport: import('@/types').CompatibilityReport | null;
  isCheckingCompat: boolean;
  // Subtitle handling
  subtitleMode: SubtitleMode;
  exportMergedSrt: boolean;
  /** Per-playlist-entry selected subtitle stream index. Keyed by entry id. */
  selectedSubtitleStreamIndices: Record<string, number | null>;
  /** Current per-file progress during audio validation (keyed by jobId). */
  fileProgress: Record<string, MergeFileProgress | null>;

  // Recovery
  recoveryCheckpoints: RecoveryCheckpoint[];
  pendingResumeCheckpoint: RecoveryCheckpoint | null;

  // Actions
  setSubtitleMode: (mode: SubtitleMode) => void;
  setExportMergedSrt: (v: boolean) => void;
  setSelectedSubtitleStreamIndex: (entryId: string, streamIndex: number | null) => void;
  setCardColor: (color: string) => void;
  setCardDuration: (d: number) => void;
  setCardShowInReport: (v: boolean) => void;
  setCardEnabled: (v: boolean) => void;
  setCardFrequency: (v: CardFrequency) => void;
  setOutputPath: (path: string) => void;
  setOutputFilename: (name: string) => void;
  setMergeMode: (mode: MergeMode) => void;
  setAudioRepairMode: (mode: AudioRepairMode) => void;
  setLargePlaylistStrategy: (strategy: LargePlaylistStrategy | null) => void;
  setConvertToMp4: (v: boolean) => void;
  setVideoCodec: (v: string) => void;
  setAudioCodec: (v: string) => void;
  setVideoCrf: (v: number) => void;
  setVideoPreset: (v: string) => void;
  setAudioBitrate: (v: string) => void;
  setTargetResolution: (v: string) => void;
  setTargetFps: (v: string) => void;
  setHwAccel: (v: string) => void;
  setRepeatConfig: (config: RepeatConfig) => void;
  setSplitConfig: (config: SplitConfig) => void;
  setNamingConfig: (config: NamingConfig) => void;
  setSelectedPlaylistIds: (ids: string[]) => void;
  addSelectedPlaylistId: (id: string) => void;
  removeSelectedPlaylistId: (id: string) => void;
  clearSelectedPlaylistIds: () => void;
  setCompatibilityReport: (r: import('@/types').CompatibilityReport | null) => void;
  setCheckingCompat: (v: boolean) => void;
  setRecoveryCheckpoints: (cps: RecoveryCheckpoint[]) => void;
  setPendingResumeCheckpoint: (cp: RecoveryCheckpoint | null) => void;
  clearRecoveryCheckpoints: () => void;

  startJob: (request: MergeRequest) => string;
  updateProgress: (jobId: string, progress: MergeProgress) => void;
  completeJob: (jobId: string, result: MergeResult) => void;
  failJob: (jobId: string, error: string) => void;
  cancelJob: (jobId: string) => void;
  removeJob: (jobId: string) => void;
  updateJobRequest: (jobId: string, request: MergeRequest) => void;
  addLog: (jobId: string, entry: MergeLogEntry) => void;
  clearLogs: (jobId: string) => void;
  setFileProgress: (jobId: string, progress: MergeFileProgress | null) => void;
  /** Record phase start times for a job (used for ETA tracking). */
  setPhaseTimes: (jobId: string, phaseStartTimes: Record<string, number>) => void;
  /** Add a subtitle extraction warning for a job */
  addSubtitleWarning: (jobId: string, warning: import('@/types').SubtitleWarning) => void;
}

export const useMergeStore = create<MergeState>()(
  persist(
    (set, _get) => ({
      activeJob: null,
      jobs: [],
      outputPath: '',
      outputFilename: 'merged_output',
      mergeMode: 'lossless',
      audioRepairMode: 'smart',
      largePlaylistStrategy: null,
      convertToMp4: false,
      videoCodec: MERGE_DEFAULTS.VIDEO_CODEC,
      audioCodec: MERGE_DEFAULTS.AUDIO_CODEC,
      videoCrf: MERGE_DEFAULTS.VIDEO_CRF,
      videoPreset: MERGE_DEFAULTS.VIDEO_PRESET,
      audioBitrate: MERGE_DEFAULTS.AUDIO_BITRATE,
      targetResolution: '',
      targetFps: '',
      hwAccel: '',
      splitConfig: { mode: 'none' },
      repeatConfig: { ...DEFAULT_REPEAT_CONFIG },
      namingConfig: {
        mode: 'sequential',
        template: '{filename}_Merged_{num3}',
        prefix: undefined,
        suffix: undefined,
        zeroPadding: 3,
        separator: undefined,
      },
      selectedPlaylistIds: [],
      compatibilityReport: null,
      isCheckingCompat: false,
      subtitleMode: 'embed',
      exportMergedSrt: false,
      selectedSubtitleStreamIndices: {},
      fileProgress: {},
      cardColor: CANVAS_COLORS[0].color,
      cardFontColor: CANVAS_COLORS[0].fontColor,
      cardDuration: 2,
      cardShowInReport: true,
      cardEnabled: false,
      cardFrequency: 'perFolder',
      recoveryCheckpoints: [],
      pendingResumeCheckpoint: null,

  setOutputPath: (path) => set({ outputPath: path }),
  setOutputFilename: (name) => set({ outputFilename: name }),
  setMergeMode: (mode) => set({ mergeMode: mode }),
  setAudioRepairMode: (mode) => set({ audioRepairMode: mode }),
  setLargePlaylistStrategy: (strategy: LargePlaylistStrategy | null) => set({ largePlaylistStrategy: strategy }),
  setConvertToMp4: (v) => set({ convertToMp4: v }),
  setVideoCodec: (v) => set({ videoCodec: v }),
  setAudioCodec: (v) => set({ audioCodec: v }),
  setVideoCrf: (v) => set({ videoCrf: v }),
  setVideoPreset: (v) => set({ videoPreset: v }),
  setAudioBitrate: (v) => set({ audioBitrate: v }),
  setTargetResolution: (v) => set({ targetResolution: v }),
  setTargetFps: (v) => set({ targetFps: v }),
  setHwAccel: (v) => set({ hwAccel: v }),
  setRepeatConfig: (config) => set({ repeatConfig: config }),
  setSplitConfig: (config) => set({ splitConfig: config }),
  setNamingConfig: (config) => set({ namingConfig: config }),
  setSelectedPlaylistIds: (ids) => set({ selectedPlaylistIds: ids }),
  addSelectedPlaylistId: (id) => set((state) => ({
    selectedPlaylistIds: state.selectedPlaylistIds.includes(id)
      ? state.selectedPlaylistIds
      : [...state.selectedPlaylistIds, id]
  })),
  removeSelectedPlaylistId: (id) => set((state) => ({
    selectedPlaylistIds: state.selectedPlaylistIds.filter((i) => i !== id)
  })),
  clearSelectedPlaylistIds: () => set({ selectedPlaylistIds: [] }),
  setCompatibilityReport: (r) => set({ compatibilityReport: r }),
  setCheckingCompat: (v) => set({ isCheckingCompat: v }),
  setRecoveryCheckpoints: (cps) => set({ recoveryCheckpoints: cps }),
  setPendingResumeCheckpoint: (cp) => set({ pendingResumeCheckpoint: cp }),
  clearRecoveryCheckpoints: () => set({ recoveryCheckpoints: [], pendingResumeCheckpoint: null }),

  setSubtitleMode: (mode) => set({ subtitleMode: mode }),
  setExportMergedSrt: (v) => set({ exportMergedSrt: v }),
  setSelectedSubtitleStreamIndex: (entryId, streamIndex) => set((state) => ({
    selectedSubtitleStreamIndices: { ...state.selectedSubtitleStreamIndices, [entryId]: streamIndex },
  })),
  setCardColor: (color) => set({
    cardColor: color,
    cardFontColor: getContrastFontColor(color),
  }),
  setCardDuration: (d) => set({ cardDuration: d }),
  setCardShowInReport: (v) => set({ cardShowInReport: v }),
  setCardEnabled: (v) => set({ cardEnabled: v }),
  setCardFrequency: (v) => set({ cardFrequency: v }),

  startJob: (request) => {
    const job: MergeJob = {
      id: request.jobId,
      request,
      progress: { percent: 0, currentTime: 0, totalDuration: request.totalDuration, phase: 'probing', overallPercent: 0 },
      startedAt: Date.now(),
      logs: [],
      subtitleWarnings: [],
    };
    set((state) => {
      const ACTIVE_PHASES = ['writing', 'preparing', 'probing', 'validating', 'normalizing', 'finalizing'];
      const MAX_COMPLETED_JOBS = 5;
      // Prune old completed jobs to prevent unbounded memory growth
      const completedJobs = state.jobs.filter(j => !ACTIVE_PHASES.includes(j.progress.phase.toLowerCase()));
      const activeJobs = state.jobs.filter(j => ACTIVE_PHASES.includes(j.progress.phase.toLowerCase()));
      const prunedCompleted = completedJobs.slice(0, MAX_COMPLETED_JOBS);
      const nextJobs = [job, ...activeJobs, ...prunedCompleted];
      const nextActive = nextJobs.find(j => ACTIVE_PHASES.includes(j.progress.phase.toLowerCase())) || null;
      return { jobs: nextJobs, activeJob: nextActive };
    });
    return job.id;
  },

  updateProgress: (jobId, progress) => {
    set((state) => {
      const existingJob = state.jobs.find(j => j.id === jobId);

      // ── Merge progress instead of replacing ────────────────────────────
      // Start from the existing progress so fields not present in the new
      // event are preserved (not wiped). Then overlay the new values.
      const clampedProgress: MergeProgress = {
        ...(existingJob?.progress ?? {}),
        ...progress,
      };

      // Clamp values to prevent UI overshoot (e.g. 5 of 3)
      if (clampedProgress.percent !== undefined) clampedProgress.percent = Math.min(100, Math.max(0, clampedProgress.percent));
      if (clampedProgress.overallPercent !== undefined) clampedProgress.overallPercent = Math.min(100, Math.max(0, clampedProgress.overallPercent));
      if (clampedProgress.stagePercent !== undefined) clampedProgress.stagePercent = Math.min(100, Math.max(0, clampedProgress.stagePercent));
      if (clampedProgress.currentFileIndex !== undefined && clampedProgress.totalFilesInStage !== undefined) {
         clampedProgress.currentFileIndex = Math.min(clampedProgress.currentFileIndex, clampedProgress.totalFilesInStage);
      }

      // ── MONOTONICITY GUARD: Progress must never move backwards ──────────
      // Prevents: 31% → 15% → 20% regression caused by phase denominator resets.
      // Only enforce when existing progress exists and phase hasn't changed.
      if (existingJob?.progress && existingJob.progress.phase === clampedProgress.phase) {
        const prev = existingJob.progress;
        // overallPercent must not decrease
        if (clampedProgress.overallPercent !== undefined && prev.overallPercent !== undefined) {
          if (clampedProgress.overallPercent < prev.overallPercent) {
            clampedProgress.overallPercent = prev.overallPercent;
          }
        }
        // stagePercent must not decrease
        if (clampedProgress.stagePercent !== undefined && prev.stagePercent !== undefined) {
          if (clampedProgress.stagePercent < prev.stagePercent) {
            clampedProgress.stagePercent = prev.stagePercent;
          }
        }
        // currentFileIndex must not decrease
        if (clampedProgress.currentFileIndex !== undefined && prev.currentFileIndex !== undefined) {
          if (clampedProgress.currentFileIndex < prev.currentFileIndex) {
            clampedProgress.currentFileIndex = prev.currentFileIndex;
          }
        }
      }

      // ── STATE TRANSITION GUARD ──────────────────────────────────────────
      // Validate that phase transitions are legal
      const LEGAL_TRANSITIONS: Record<string, string[]> = {
        preparing: ['probing', 'validating', 'normalizing', 'writing', 'finalizing', 'complete', 'failed', 'cancelled'],
        probing: ['validating', 'normalizing', 'writing', 'finalizing', 'complete', 'failed', 'cancelled'],
        validating: ['normalizing', 'writing', 'finalizing', 'complete', 'failed', 'cancelled'],
        normalizing: ['writing', 'finalizing', 'complete', 'failed', 'cancelled'],
        writing: ['finalizing', 'complete', 'failed', 'cancelled'],
        finalizing: ['complete', 'failed', 'cancelled'],
        complete: [],  // Terminal
        failed: [],     // Terminal
        cancelled: [],  // Terminal
      };

      if (existingJob?.progress?.phase && clampedProgress.phase) {
        const fromPhase = existingJob.progress.phase.toLowerCase();
        const toPhase = clampedProgress.phase.toLowerCase();
        if (fromPhase !== toPhase) {
          const allowed = LEGAL_TRANSITIONS[fromPhase] || [];
          if (!allowed.includes(toPhase)) {
            console.warn(`[StateTransition] ILLEGAL: ${fromPhase} → ${toPhase} (job ${jobId})`);
          }
        }
      }

      const nextJobs = state.jobs.map((j) => j.id === jobId ? { ...j, progress: clampedProgress } : j);
      const ACTIVE_PHASES = ['writing', 'preparing', 'probing', 'validating', 'normalizing', 'finalizing'];
      const nextActive = nextJobs.find(j => ACTIVE_PHASES.includes(j.progress.phase.toLowerCase())) || null;
      return { jobs: nextJobs, activeJob: nextActive };
    });
  },

  completeJob: (jobId, result) => {
    set((state) => {
      const nextJobs = state.jobs.map((j) =>
        j.id === jobId
          ? {
              ...j,
              result,
              completedAt: Date.now(),
              progress: { ...j.progress, percent: 100, phase: 'complete' as const },
            }
          : j
      );
      const ACTIVE_PHASES = ['writing', 'preparing', 'probing', 'validating', 'normalizing', 'finalizing'];
      const nextActive = nextJobs.find(j => ACTIVE_PHASES.includes(j.progress.phase.toLowerCase())) || null;
      return { jobs: nextJobs, activeJob: nextActive };
    });
  },

  failJob: (jobId, error) => {
    set((state) => {
      const nextJobs = state.jobs.map((j) =>
        j.id === jobId
          ? {
              ...j,
              error,
              completedAt: Date.now(),
              progress: { ...j.progress, phase: 'failed' as const },
            }
          : j
      );
      const ACTIVE_PHASES = ['writing', 'preparing', 'probing', 'validating', 'normalizing', 'finalizing'];
      const nextActive = nextJobs.find(j => ACTIVE_PHASES.includes(j.progress.phase.toLowerCase())) || null;
      return { jobs: nextJobs, activeJob: nextActive };
    });
  },

  cancelJob: (jobId) => {
    set((state) => {
      const nextJobs = state.jobs.map((j) =>
        j.id === jobId
          ? {
              ...j,
              completedAt: Date.now(),
              progress: { ...j.progress, phase: 'cancelled' as const },
            }
          : j
      );
      const ACTIVE_PHASES = ['writing', 'preparing', 'probing', 'validating', 'normalizing', 'finalizing'];
      const nextActive = nextJobs.find(j => ACTIVE_PHASES.includes(j.progress.phase.toLowerCase())) || null;
      return { jobs: nextJobs, activeJob: nextActive };
    });
  },

  removeJob: (jobId) => {
    set((state) => {
      const ACTIVE_PHASES = ['writing', 'preparing', 'probing', 'validating', 'normalizing', 'finalizing'];
      const job = state.jobs.find(j => j.id === jobId);
      if (job && ACTIVE_PHASES.includes(job.progress.phase.toLowerCase())) {
        return state; // Don't remove active jobs
      }
      const nextJobs = state.jobs.filter((j) => j.id !== jobId);
      const nextActive = nextJobs.find(j => ACTIVE_PHASES.includes(j.progress.phase.toLowerCase())) || null;
      // Clean up fileProgress for the removed job
      const { [jobId]: _unusedField, ...restFileProgress } = state.fileProgress;
      void _unusedField;
      return { jobs: nextJobs, activeJob: nextActive, fileProgress: restFileProgress };
    });
  },

updateJobRequest: (jobId, request) => {
        set((state) => ({
          jobs: state.jobs.map((j) =>
            j.id === jobId
              ? { ...j, request, progress: { ...j.progress, totalDuration: request.totalDuration } }
              : j
          ),
        }));
      },

  addLog: (jobId, entry) => {
    set((state) => ({
      jobs: state.jobs.map((j) =>
        j.id === jobId
          ? { ...j, logs: [...j.logs, entry].slice(-500) }
          : j
      ),
    }));
  },

  clearLogs: (jobId) => {
    set((state) => ({
      jobs: state.jobs.map((j) =>
        j.id === jobId
          ? { ...j, logs: [] }
          : j
      ),
    }));
  },

  setFileProgress: (jobId, progress) => {
    set((state) => ({
      fileProgress: {
        ...state.fileProgress,
        [jobId]: progress,
      },
    }));
  },

  setPhaseTimes: (jobId, phaseStartTimes) => {
    set((state) => ({
      jobs: state.jobs.map((j) =>
        j.id === jobId
          ? { ...j, phaseTimes: phaseStartTimes as Partial<import('@/types').PhaseTimes> }
          : j
      ),
    }));
  },

  addSubtitleWarning: (jobId, warning) => {
    set((state) => ({
      jobs: state.jobs.map((j) =>
        j.id === jobId
          ? { ...j, subtitleWarnings: [...j.subtitleWarnings, warning] }
          : j
      ),
    }));
  },
    }),
    {
      name: 'playlist-merger-merge-settings',
      version: 1,
      partialize: (state) => ({
        mergeMode: state.mergeMode,
        audioRepairMode: state.audioRepairMode,
        largePlaylistStrategy: state.largePlaylistStrategy,
        videoCodec: state.videoCodec,
        audioCodec: state.audioCodec,
        videoCrf: state.videoCrf,
        videoPreset: state.videoPreset,
        audioBitrate: state.audioBitrate,
        targetResolution: state.targetResolution,
        targetFps: state.targetFps,
        hwAccel: state.hwAccel,
        splitConfig: state.splitConfig,
        repeatConfig: state.repeatConfig,
        namingConfig: state.namingConfig,
        subtitleMode: state.subtitleMode,
        exportMergedSrt: state.exportMergedSrt,
        cardColor: state.cardColor,
        cardFontColor: state.cardFontColor,
        cardDuration: state.cardDuration,
        cardShowInReport: state.cardShowInReport,
        cardEnabled: state.cardEnabled,
        cardFrequency: state.cardFrequency,
        convertToMp4: state.convertToMp4,
        selectedSubtitleStreamIndices: state.selectedSubtitleStreamIndices,
      }),
    }
  )
);


