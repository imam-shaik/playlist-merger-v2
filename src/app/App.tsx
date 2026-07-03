import React, { useEffect, useRef } from 'react';
import { AnimatePresence, motion } from 'framer-motion';
import { Sidebar } from '@/components/ui/Sidebar';
import { Titlebar } from '@/components/ui/Titlebar';
import { ToastContainer } from '@/components/ui/Toast';
import { ErrorBoundary } from '@/components/ui/ErrorBoundary';
import { WorkspaceLayout } from '@/components/layout/WorkspaceLayout';
import { PlaylistScreen } from '@/features/playlist/PlaylistScreen';
import { SettingsScreen } from '@/features/settings/SettingsScreen';
import { MergePanel } from '@/features/merge/MergePanel';
import { SplitScreen } from '@/features/split/SplitScreen';
import { RepeatScreen } from '@/features/repeat/RepeatScreen';
import { RecoveryDialog } from '@/features/merge/RecoveryDialog';
import { SubfolderSelectionModal } from '@/features/playlist/SubfolderSelectionModal';
import { ScreenshotPanel } from '@/components/screenshot/ScreenshotPanel';
import { useScreenshotStore } from '@/store/screenshotStore';
import { useAppStore } from '@/store/appStore';
import { usePlaylistStore } from '@/store/playlistStore';
import { useMergeStore } from '@/store/mergeStore';
import { useSplitStore } from '@/store/splitStore';
import { useMerge } from '@/hooks/useMerge';
import { useSectionEvents } from '@/hooks/useSectionEvents';
import { useAutosave } from '@/hooks/useAutosave';
import { useGlobalKeyboardShortcuts } from '@/hooks/useKeyboardShortcuts';
import { tauriCommands } from '@/tauri/commands';
import type { RecoveryCheckpoint, MergeRequest, MediaInfo } from '@/types';

// Debug helper (set to false in production)
const DEBUG = false;
function dbg(...args: unknown[]) {
  if (DEBUG) console.log('[Debug:App]', ...args);
}

const SCREEN_TITLES: Record<string, string> = {
  playlist: 'Playlist',
  merge: 'Merge Config & Queue',
  split: 'Video Split Engine',
  settings: 'Settings',
};

function AppContent() {
  const screen = useAppStore((s) => s.screen);  const settings = useAppStore((s) => s.settings);
  const entryCount = usePlaylistStore((s) => s.entries.length);
  const screenshotPanelOpen = useScreenshotStore((s) => s.panelOpen);

  dbg('RENDER — screen:', screen, 'entries:', entryCount, 'settingsLoaded:', !!settings);

  // Init hooks
  useMerge();
  useSectionEvents();
  useAutosave(settings?.autoSavePlaylist ?? true);
  useGlobalKeyboardShortcuts();

  // Cancel in-flight Rust operations on screen change.
  // Prevents "Couldn't find callback id" errors when the user navigates
  // away while batch_probe, generate_thumbnail, or scan_directory are running.
  const prevScreenRef = useRef(screen);
  useEffect(() => {
    if (prevScreenRef.current !== screen) {
      prevScreenRef.current = screen;
      tauriCommands.cancelPendingOperations().catch(() => {});
    }
  }, [screen]);

  // Load settings on mount
  useEffect(() => {
    dbg('mount useEffect — loading settings...');
    let splitUnlisten: (() => void) | null = null;
    const init = async () => {
      try {
        const [loadedSettings, ffmpegPaths, recoveryCheckpoints] = await Promise.all([
          tauriCommands.getSettings(),
          tauriCommands.getFfmpegPath(),
          tauriCommands.checkRecoveryCheckpoints(),
        ]);
        dbg('settings loaded:', loadedSettings);
        dbg('ffmpeg paths:', ffmpegPaths);
        useAppStore.getState().setSettings(loadedSettings);
        useAppStore.getState().setFfmpegPaths(ffmpegPaths);
        if (!ffmpegPaths.ffmpegFound) {
          dbg('ffmpeg NOT found');
          useAppStore.getState().setFfmpegMissing(true);
        }
        // Apply defaultMergeMode from settings to mergeStore on startup
        if (loadedSettings.defaultMergeMode) {
          useMergeStore.getState().setMergeMode(loadedSettings.defaultMergeMode);
        }
        if (recoveryCheckpoints.length > 0) {
          dbg('recovery checkpoints found:', recoveryCheckpoints.length);
          useMergeStore.getState().setRecoveryCheckpoints(recoveryCheckpoints);
        }
        // Initialize split event listeners for crash recovery
        splitUnlisten = await useSplitStore.getState().initSplitEvents();
        dbg('init complete');
      } catch (err) {
        console.error('Init error:', err);
        useAppStore.getState().setSettings({
          maxThumbnailCacheMb: 500,
          recentExports: [],
          defaultMergeMode: 'lossless',
          checkCompatBeforeMerge: true,
          autoSavePlaylist: true,
        });
      }
    };
    init();
    return () => {
      if (splitUnlisten) {
        splitUnlisten();
      }
    };
  }, []);

  const recoveryCheckpoints = useMergeStore((s) => s.recoveryCheckpoints);

  const handleResume = async (checkpoint: RecoveryCheckpoint) => {
    useMergeStore.getState().setRecoveryCheckpoints([]);
    useAppStore.getState().setScreen('merge');

    try {
      dbg('[Recovery] Resuming merge job:', checkpoint.jobId);

      // P0 FIX: Use persisted durations if available, avoid re-probing
      const mediaInfos: (MediaInfo | null)[] = [];
      let inputDurations: number[] = [];
      let totalDuration = 0;

      // Strategy 1: Persisted total_duration directly
      if (checkpoint.totalDuration != null && checkpoint.totalDuration > 0) {
        dbg('[Recovery] Using persisted totalDuration:', checkpoint.totalDuration);
        totalDuration = checkpoint.totalDuration;

        // Strategy 1a: If inputDurations also persisted, use them directly
        if (checkpoint.inputDurations && checkpoint.inputDurations.length > 0) {
          inputDurations = [...checkpoint.inputDurations];
          dbg('[Recovery] Using persisted inputDurations, count:', inputDurations.length);
        } else {
          // Need to at least probe for mediaInfos (needed for merge)
          const probeResults = await tauriCommands.batchProbe(checkpoint.inputFiles);
          probeResults.forEach((res, idx) => {
            if (typeof res !== 'string') {
              mediaInfos[idx] = res;
              inputDurations[idx] = res.duration ?? 0;
            } else {
              mediaInfos[idx] = null;
              inputDurations[idx] = 0;
            }
          });
        }
      }
      // Strategy 2: Compute total from persisted input_durations
      else if (checkpoint.inputDurations && checkpoint.inputDurations.length > 0) {
        dbg('[Recovery] Computing totalDuration from persisted inputDurations');
        inputDurations = [...checkpoint.inputDurations];
        totalDuration = inputDurations.reduce((sum, d) => sum + d, 0);
        dbg('[Recovery] Computed totalDuration:', totalDuration);

        // Still need mediaInfos for merge
        const probeResults = await tauriCommands.batchProbe(checkpoint.inputFiles);
        probeResults.forEach((res, idx) => {
          if (typeof res !== 'string') {
            mediaInfos[idx] = res;
          } else {
            mediaInfos[idx] = null;
          }
        });
      }
      // Strategy 3: Fallback to batchProbe (backward compatibility)
      else {
        dbg('[Recovery] No persisted durations, falling back to batchProbe');
        const probeResults = await tauriCommands.batchProbe(checkpoint.inputFiles);
        probeResults.forEach((res, idx) => {
          if (typeof res !== 'string') {
            mediaInfos[idx] = res;
            const dur = res.duration ?? 0;
            inputDurations[idx] = dur;
            totalDuration += dur;
          } else {
            mediaInfos[idx] = null;
            inputDurations[idx] = 0;
          }
        });
      }

      // TRACE 1: Checkpoint contents
      dbg('[RECOVERY_LOAD] checkpoint.totalDuration=', checkpoint.totalDuration);
      dbg('[RECOVERY_LOAD] checkpoint.inputDurations=', checkpoint.inputDurations?.length ?? 0);

      const request: MergeRequest = {
        jobId: checkpoint.jobId,
        phase: checkpoint.phase as import('@/types').MergePhase,
        inputFiles: checkpoint.inputFiles,
        mediaInfos: mediaInfos.filter((m): m is MediaInfo => m !== null),
        inputNames: checkpoint.inputFiles.map((p) => p.split(/[/\\]/).pop() ?? 'file'),
        inputDurations,
        outputPath: checkpoint.outputPath,
        mode: checkpoint.mode as import('@/types').MergeMode,
        totalDuration,
        subtitleMode: checkpoint.subtitleMode as import('@/types').SubtitleMode | undefined,
        exportMergedSrt: checkpoint.exportMergedSrt ?? false,
        selectedSubtitleStreamIndices: checkpoint.selectedSubtitleStreamIndices,
        // Encoding settings
        ...(checkpoint.videoCodec && { videoCodec: checkpoint.videoCodec }),
        ...(checkpoint.audioCodec && { audioCodec: checkpoint.audioCodec }),
        ...(checkpoint.videoCrf != null && { videoCrf: checkpoint.videoCrf }),
        ...(checkpoint.videoPreset && { videoPreset: checkpoint.videoPreset }),
        ...(checkpoint.audioBitrate && { audioBitrate: checkpoint.audioBitrate }),
        ...(checkpoint.targetResolution && { targetResolution: checkpoint.targetResolution }),
        ...(checkpoint.targetFps && { targetFps: checkpoint.targetFps }),
        ...(checkpoint.hwAccel && { hwAccel: checkpoint.hwAccel }),
        // Canvas cards
        ...(checkpoint.cardConfig && { cardConfig: checkpoint.cardConfig }),
        // Repeat
        ...(checkpoint.repeatConfig && { repeatConfig: checkpoint.repeatConfig }),
        // Split
        ...(checkpoint.splitConfig && { splitConfig: checkpoint.splitConfig }),
        // Naming
        ...(checkpoint.namingConfig && { namingConfig: checkpoint.namingConfig }),
        // Audio repair
        ...(checkpoint.audioRepairMode && { audioRepairMode: checkpoint.audioRepairMode as import('@/types').AudioRepairMode }),
        ...(checkpoint.validateAudio != null && { validateAudio: checkpoint.validateAudio }),
        ...(checkpoint.largePlaylistStrategy && { largePlaylistStrategy: checkpoint.largePlaylistStrategy as import('@/types').LargePlaylistStrategy }),
        // Fast/Smart MKV
        ...(checkpoint.convertToMp4 != null && { convertToMp4: checkpoint.convertToMp4 }),
      };

      // TRACE 2: Request construction
      const inputDurSum = request.inputDurations.reduce((s: number, d: number) => s + d, 0);
      dbg('[RECOVERY_REQUEST] totalDuration=', request.totalDuration);
      dbg('[RECOVERY_REQUEST] inputDurations.length=', request.inputDurations.length);
      dbg('[RECOVERY_REQUEST] inputDurations.sum=', inputDurSum);

      useMergeStore.getState().startJob(request);
      // Restore outputFilename from checkpoint so the user's naming is preserved
      const outputFileName = checkpoint.outputPath.split(/[/\\]/).pop() ?? '';
      if (outputFileName) {
        useMergeStore.getState().setOutputFilename(outputFileName.replace(/\.[^.]+$/, ''));
      }
      await tauriCommands.startMerge(request);
      dbg('[Recovery] Merge resume initiated for job:', checkpoint.jobId);
    } catch (err) {
      console.error('[Recovery] Failed to resume merge:', err);
      useAppStore.getState().showToast({
        type: 'error',
        title: 'Failed to resume merge',
        description: String(err),
        durationMs: 8000,
      });
    }
  };

  const handleStartOver = async (checkpoint: RecoveryCheckpoint) => {
    try {
      await tauriCommands.deleteRecoveryCheckpoint(checkpoint.jobId);
      useMergeStore.getState().setRecoveryCheckpoints(
        recoveryCheckpoints.filter((cp) => cp.jobId !== checkpoint.jobId)
      );
    } catch (err) {
      console.error('Failed to delete checkpoint:', err);
    }
  };

  const renderScreen = () => {
    dbg('renderScreen — screen:', screen);
    switch (screen) {
      case 'playlist': return <PlaylistScreen />;
      case 'split': return <SplitScreen />;
      case 'repeat': return <RepeatScreen />;
      case 'merge': return <MergePanel />;
      case 'settings': return <SettingsScreen />;
      default: return <PlaylistScreen />;
    }
  };

  return (
    <div className="flex flex-col h-screen w-screen bg-bg-base text-text-primary overflow-hidden font-sans">
      <Titlebar />

      <WorkspaceLayout
        sidebar={<Sidebar />}
        main={
          <main className="h-full flex flex-col min-w-0 relative">
            {/* Header */}
            <header className="flex items-center h-10 px-4 border-b border-border bg-bg-surface/80 backdrop-blur-sm shrink-0">
              <h1 className="text-sm font-semibold text-text-primary">
                {SCREEN_TITLES[screen] ?? screen}
              </h1>
            </header>

            {/* Screen content */}
            <div className="flex-1 min-h-0 relative">
              <AnimatePresence mode="wait">
                <motion.div
                  key={screen}
                  initial={{ opacity: 0, x: 8 }}
                  animate={{ opacity: 1, x: 0 }}
                  exit={{ opacity: 0, x: -8 }}
                  transition={{ duration: 0.18, ease: [0.16, 1, 0.3, 1] }}
                  className="absolute inset-0 flex flex-col"
                >
                  {renderScreen()}
                </motion.div>
              </AnimatePresence>
            </div>
          </main>
        }
        inspector={screenshotPanelOpen ? <ScreenshotPanel /> : null}
      />

      <ToastContainer />

      <RecoveryDialog
        checkpoints={recoveryCheckpoints}
        onResume={handleResume}
        onStartOver={handleStartOver}
      />

      <SubfolderSelectionModal />
    </div>
  );
}

export default function App() {
  return (
    <ErrorBoundary label="App">
      <AppContent />
    </ErrorBoundary>
  );
}