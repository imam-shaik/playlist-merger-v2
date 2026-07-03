import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type {
  SplitMode,
  SplitParams,
  SplitPlan,
  SplitPlanRequest,
  SplitExecuteRequest,
  SplitJob,
  SplitStage,
  NamingConfig,
} from '@/types';
import { generateId } from '@/utils';


interface SplitStore {
  // Current file info
  inputFile: string;
  inputDuration: number;
  inputSizeBytes: number;
  inputSubtitleStreams: import('@/types').SubtitleStream[];
  outputDir: string;

  // Mode & params
  splitMode: SplitMode;
  splitParams: SplitParams;
  namingConfig: NamingConfig;

  // Preview
  currentPlan: SplitPlan | null;
  isGeneratingPlan: boolean;
  planError: string | null;

  // Jobs
  activeJob: SplitJob | null;
  jobs: SplitJob[];

  // Actions
  setInputFile: (path: string) => void;
  setInputDuration: (d: number) => void;
  setInputSizeBytes: (s: number) => void;
  setInputSubtitleStreams: (streams: import('@/types').SubtitleStream[]) => void;
  setOutputDir: (dir: string) => void;
  setSplitMode: (mode: SplitMode) => void;
  setSplitParams: (params: SplitParams) => void;
  setNamingConfig: (config: NamingConfig) => void;
  setCurrentPlan: (plan: SplitPlan | null) => void;
  setGeneratingPlan: (v: boolean) => void;
  setPlanError: (err: string | null) => void;

  generatePlan: () => Promise<void>;
  executePlan: () => Promise<void>;
  cancelJob: (jobId: string) => void;
  removeJob: (jobId: string) => void;
  reset: () => void;

  // Event listener initialization (call from App.tsx on startup)
  initSplitEvents: () => Promise<() => void>;
}

const defaultParams: SplitParams = {
  partCount: 2,
  partDuration: 3600,
  customRanges: undefined,
  itemsPerSegment: 10,
  maxSizeBytes: 4_000_000_000,
  courseMode: 'daily',
  hoursPerUnit: 2,
  outputFormat: 'mp4',
  labelPrefix: undefined,
  subtitleMode: 'copyAll',
  exportSrt: true,
};

const defaultNamingConfig: NamingConfig = {
  mode: 'sequential',
  template: '{filename}_Part_{num3}',
  prefix: undefined,
  suffix: undefined,
  zeroPadding: 3,
  separator: undefined,
};

export const useSplitStore = create<SplitStore>()(
  persist(
    (set, get) => ({
  inputFile: '',
  inputDuration: 0,
  inputSizeBytes: 0,
  inputSubtitleStreams: [],
  outputDir: '',
  splitMode: 'byParts',
  splitParams: { ...defaultParams },
  namingConfig: { ...defaultNamingConfig },
  currentPlan: null,
  isGeneratingPlan: false,
  planError: null,
  activeJob: null,
  jobs: [],

  setInputFile: (path) => set({ inputFile: path, currentPlan: null, planError: null, inputSubtitleStreams: [] }),
  setInputDuration: (d) => set({ inputDuration: d }),
  setInputSizeBytes: (s) => set({ inputSizeBytes: s }),
  setInputSubtitleStreams: (streams) => set({ inputSubtitleStreams: streams }),
  setOutputDir: (dir) => set({ outputDir: dir, currentPlan: null }),
  setSplitMode: (mode) => set({ splitMode: mode, currentPlan: null, planError: null }),
  setSplitParams: (params) => set({ splitParams: { ...get().splitParams, ...params }, currentPlan: null }),
  setNamingConfig: (config) => set({ namingConfig: config, currentPlan: null }),
  setCurrentPlan: (plan) => set({ currentPlan: plan }),
  setGeneratingPlan: (v) => set({ isGeneratingPlan: v }),
  setPlanError: (err) => set({ planError: err }),

  generatePlan: async () => {
    const state = get();
    if (!state.inputFile) {
      set({ planError: 'No input file selected' });
      return;
    }

    set({ isGeneratingPlan: true, planError: null, currentPlan: null });
    const jobId = generateId();

    const request: SplitPlanRequest = {
      jobId,
      inputFile: state.inputFile,
      inputDuration: state.inputDuration,
      inputSizeBytes: state.inputSizeBytes,
      mode: state.splitMode,
      params: state.splitParams,
      outputDir: state.outputDir,
    };

    try {
      const { tauriCommands } = await import('@/tauri/commands');

      let plan: SplitPlan;
      if (state.splitMode === 'byChapters' || state.splitMode === 'byPlaylistItems') {
        plan = await tauriCommands.generateChapterSplitPlan(request);
      } else {
        plan = await tauriCommands.generateSplitPlan(request);
      }

      set({ currentPlan: plan, isGeneratingPlan: false });
    } catch (err) {
      set({
        planError: err instanceof Error ? err.message : String(err),
        isGeneratingPlan: false,
      });
    }
  },

executePlan: async () => {
    const state = get();
    const plan = state.currentPlan;
    if (!plan) {
      set({ planError: 'No split plan to execute. Generate a plan first.' });
      return;
    }

    const jobId = plan.jobId;
    const job: SplitJob = {
      id: jobId,
      plan,
      progress: {
        jobId,
        segmentIndex: 0,
        segmentCount: plan.segments.length,
        progress: 0,
        stage: 'planning',
        message: 'Starting split...',
      },
      stage: 'splitting',
      startedAt: Date.now(),
    };

    set((s) => ({
      jobs: [job, ...s.jobs],
      activeJob: job,
      planError: null,
    }));

    try {
      const { tauriCommands } = await import('@/tauri/commands');

      const request: SplitExecuteRequest = {
        jobId,
        plan,
        subtitleMode: state.splitParams.subtitleMode,
        exportSrt: state.splitParams.exportSrt,
        namingConfig: state.namingConfig,
      };

      // Start listening for progress events
      const { tauriEvents } = await import('@/tauri/commands');
      const unlisten = await tauriEvents.onSplitProgress((e) => {
        set((s) => ({
          jobs: s.jobs.map((j) =>
            j.id === e.jobId
              ? { ...j, progress: e }
              : j
          ),
          activeJob: s.activeJob?.id === e.jobId
            ? { ...s.activeJob, progress: e }
            : s.activeJob,
        }));
      });

      try {
        const result = await tauriCommands.executeSplitPlan(request);

        set((s) => {
          const updatedJobs = s.jobs.map((j) =>
            j.id === jobId
              ? {
                  ...j,
                  result,
                  stage: 'complete' as SplitStage,
                  completedAt: Date.now(),
                  progress: {
                    ...j.progress,
                    progress: 100,
                    stage: 'done' as const,
                    message: 'Split complete!',
                    segmentIndex: j.progress.segmentCount,
                  },
                }
              : j
          );
          const nextActive = updatedJobs.find(
            (j) => j.stage === 'splitting'
          ) ?? null;
          return { jobs: updatedJobs, activeJob: nextActive };
        });
      } finally {
        unlisten();
      }
    } catch (err) {
      const errMsg = err instanceof Error ? err.message : String(err);
      set((s) => {
        const updatedJobs = s.jobs.map((j) =>
          j.id === jobId
            ? {
                ...j,
                error: errMsg,
                stage: 'failed' as SplitStage,
                completedAt: Date.now(),
                progress: {
                  ...j.progress,
                  stage: 'error' as const,
                  message: errMsg,
                },
              }
            : j
        );
        const nextActive = updatedJobs.find(
          (j) => j.stage === 'splitting'
        ) ?? null;
        return { jobs: updatedJobs, activeJob: nextActive };
      });
    }
  },

  cancelJob: async (jobId) => {
    try {
      const { tauriCommands } = await import('@/tauri/commands');
      await tauriCommands.cancelSplit(jobId);
    } catch { /* non-critical */ }

    set((s) => {
      const updatedJobs = s.jobs.map((j) =>
        j.id === jobId
          ? {
              ...j,
              stage: 'cancelled' as SplitStage,
              completedAt: Date.now(),
              progress: { ...j.progress, stage: 'cancelled' as const, message: 'Cancelled' },
            }
          : j
      );
      const nextActive = updatedJobs.find(
        (j) => j.stage === 'splitting'
      ) ?? null;
      return { jobs: updatedJobs, activeJob: nextActive };
    });
  },

  removeJob: (jobId) => {
    set((s) => {
      const updatedJobs = s.jobs.filter((j) => j.id !== jobId);
      const nextActive = s.activeJob?.id === jobId
        ? updatedJobs.find((j) => j.stage === 'splitting') ?? null
        : s.activeJob;
      return { jobs: updatedJobs, activeJob: nextActive };
    });
  },

  reset: () =>
    set({
      inputFile: '',
      inputDuration: 0,
      inputSizeBytes: 0,
      inputSubtitleStreams: [],
      outputDir: '',
      splitMode: 'byParts',
      splitParams: { ...defaultParams },
      namingConfig: { ...defaultNamingConfig },
      currentPlan: null,
      isGeneratingPlan: false,
      planError: null,
    }),

  initSplitEvents: async () => {
    const { tauriEvents } = await import('@/tauri/commands');
    const unlisteners: Array<() => void> = [];

    const u1 = await tauriEvents.onSplitComplete((e) => {
      console.log(`[EVENT_RECEIVED] event=split-complete jobId=${e.jobId}`);
      set((s) => {
        const job = s.jobs.find((j) => j.id === e.jobId);
        if (!job) return s;
        if (job.stage === 'complete') return s;
        const updatedJobs = s.jobs.map((j) =>
          j.id === e.jobId
            ? {
                ...j,
                stage: 'complete' as SplitStage,
                completedAt: Date.now(),
                progress: {
                  ...j.progress,
                  progress: 100,
                  stage: 'done' as const,
                  message: 'Split complete!',
                  segmentIndex: j.progress.segmentCount,
                },
              }
            : j
        );
        const nextActive = updatedJobs.find((j) => j.stage === 'splitting') ?? null;
        return { jobs: updatedJobs, activeJob: nextActive };
      });
    });
    unlisteners.push(u1);

    const u2 = await tauriEvents.onSplitError((e) => {
      console.log(`[EVENT_RECEIVED] event=split-error jobId=${e.jobId} error=${e.error}`);
      set((s) => {
        const job = s.jobs.find((j) => j.id === e.jobId);
        if (!job) return s;
        if (job.stage === 'failed') return s;
        const updatedJobs = s.jobs.map((j) =>
          j.id === e.jobId
            ? {
                ...j,
                error: e.error,
                stage: 'failed' as SplitStage,
                completedAt: Date.now(),
                progress: {
                  ...j.progress,
                  stage: 'error' as const,
                  message: e.error,
                },
              }
            : j
        );
        const nextActive = updatedJobs.find((j) => j.stage === 'splitting') ?? null;
        return { jobs: updatedJobs, activeJob: nextActive };
      });
    });
    unlisteners.push(u2);

    return () => {
      unlisteners.forEach((fn) => fn());
    };
  },
}),
{
  name: 'playlist-merger-split-settings',
  version: 1,
  partialize: (state) => ({
    inputFile: state.inputFile,
    outputDir: state.outputDir,
    splitMode: state.splitMode,
    splitParams: state.splitParams,
    namingConfig: state.namingConfig,
  }),
}
  )
);
