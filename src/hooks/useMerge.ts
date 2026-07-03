import { useEffect, useCallback, useRef } from 'react';
import { useMergeStore } from '@/store/mergeStore';
import { useAppStore } from '@/store/appStore';
import { tauriCommands, tauriEvents, saveOutputFileDialog } from '@/tauri/commands';
import { usePlaylistStore } from '@/store/playlistStore';
import { generateId, ensureVideoExtension, forceVideoExtension, sanitizeFilename, getDirectory, formatBytes } from '@/utils';
import { translateError } from '@/utils/errorMessages';
import type { MediaInfo, MergeRequest, MergeStatsRecord, MergeMode, AudioRepairMode, PhaseTimes } from '@/types';

function normalizePath(p: string): string {
  const normalized = p.replace(/\\/g, '/').replace(/\/+\./g, '/.').split('/').reduce<(string[])>((acc, seg) => {
    if (seg === '' || seg === '.') return acc;
    if (seg === '..') { acc.pop(); return acc; }
    acc.push(seg);
    return acc;
  }, []).join('/');
  // Case-insensitive normalization for Windows (NTFS is case-insensitive)
  // On Linux/macOS, paths are case-sensitive, but this app targets Windows
  return normalized.toLowerCase();
}

export function useMerge() {
  // Subscribe to individual actions only (stable references, never cause re-renders)
  const updateProgress = useMergeStore((s) => s.updateProgress);
  const completeJob = useMergeStore((s) => s.completeJob);
  const failJob = useMergeStore((s) => s.failJob);
  const cancelJob = useMergeStore((s) => s.cancelJob);
  const addLog = useMergeStore((s) => s.addLog);
  const clearLogs = useMergeStore((s) => s.clearLogs);
  const showToast = useAppStore((s) => s.showToast);
  const setSettings = useAppStore((s) => s.setSettings);
  // Use a ref to store unlisten functions — avoids stale closure issues
  const unlistenRef = useRef<Array<() => void>>([]);
  // Track previous phase per job for synthesizing log entries
  const prevPhaseRef = useRef<Record<string, string>>({});
  // Track rate samples per job: { timestamp, currentFileIndex }
  const rateSamplesRef = useRef<Record<string, Array<{ t: number; idx: number }>>>({});
  // Track phase start times per job: { [jobId]: { [phase]: startTimestampMs } }
  const phaseStartTimesRef = useRef<Record<string, Record<string, number>>>({});
  const setPhaseTimes = useMergeStore((s) => s.setPhaseTimes);

  useEffect(() => {
    let cancelled = false;
    const fns: Array<() => void> = [];

    const setup = async () => {
      try {
        const u1 = await tauriEvents.onMergeProgress((event) => {
          if (cancelled) return;

          updateProgress(event.jobId, event.progress);

          // Record rate sample for stable ETA (moving average over 30s window)
          const now = Date.now();
          const WINDOW_MS = 30_000;
          if (event.progress.currentFileIndex !== undefined) {
            const samples = rateSamplesRef.current[event.jobId] ?? [];
            samples.push({ t: now, idx: event.progress.currentFileIndex });
            // Keep only samples within the last 30 seconds
            const cutoff = now - WINDOW_MS;
            const pruned = samples.filter(s => s.t >= cutoff);
            rateSamplesRef.current[event.jobId] = pruned;
          }

          // Synthesize log entries on phase changes and track phase start times
          const prevPhase = prevPhaseRef.current[event.jobId];
          const newPhase = event.progress.phase;
          if (prevPhase !== newPhase) {
            prevPhaseRef.current[event.jobId] = newPhase;

            // Initialize phase start times for this job if not yet tracked
            if (!phaseStartTimesRef.current[event.jobId]) {
              phaseStartTimesRef.current[event.jobId] = {};
            }
            // Record the start time of the new phase and persist to store
            phaseStartTimesRef.current[event.jobId][newPhase] = now;
            setPhaseTimes(event.jobId, { ...phaseStartTimesRef.current[event.jobId] });

            const phaseLabels: Record<string, string> = {
              probing: 'Analysing files...',
              validating: 'Validating file integrity...',
              preparing: 'Preparing files...',
              normalizing: 'Normalising files...',
              writing: 'Merging timeline...',
              finalizing: 'Finalising output...',
              complete: 'Merge complete!',
              failed: 'Merge failed',
              cancelled: 'Merge cancelled',
            };
            const label = phaseLabels[newPhase] || newPhase;
            addLog(event.jobId, {
              timestamp: now,
              level: 'info',
              message: `Phase: ${label}`,
            });
            // Log large playlist detection
            if (newPhase === 'validating' && event.progress.isLargePlaylist) {
              addLog(event.jobId, {
                timestamp: now,
                level: 'info',
                message: 'Large playlist detected — Fast Validation enabled',
              });
            }
            // Log warning if present
            if (event.progress.warning) {
              addLog(event.jobId, {
                timestamp: now,
                level: 'warn',
                message: event.progress.warning,
              });
            }
          }
          // Log every 50 files validated for long playlists (reduced from 25)
          if (newPhase === 'validating' && event.progress.currentFileIndex !== undefined) {
            const idx = event.progress.currentFileIndex;
            if (idx > 0 && idx % 50 === 0) {
              addLog(event.jobId, {
                timestamp: now,
                level: 'info',
                message: `${idx} files validated`,
              });
            }
          }
        });
        fns.push(u1);

        // Listen for per-file validation progress (structured events for audio check cards)
        const u1c = await tauriEvents.onMergeFileProgress((event) => {
          if (cancelled) return;
          useMergeStore.getState().setFileProgress(event.jobId, event);
        });
        fns.push(u1c);

        // Listen for subtitle extraction warnings
        const uSub = await tauriEvents.onSubtitleWarning((event) => {
          if (cancelled) return;
          useMergeStore.getState().addSubtitleWarning(event.jobId, {
            fileIndex: event.fileIndex,
            filePath: event.file,
            reason: event.error,
          });
        });
        fns.push(uSub);

        const u2 = await tauriEvents.onMergeComplete(async (event) => {
          if (cancelled) return;
          console.log(`[EVENT_RECEIVED] event=merge-complete jobId=${event.jobId}`);
          completeJob(event.jobId, {
            jobId: event.jobId,
            outputPath: event.outputPath,
            outputSizeBytes: event.outputSizeBytes,
            outputDurationSecs: event.outputDurationSecs,
            segments: event.segments,
            outputPaths: event.outputPaths,
            parts: event.parts,
            reportPaths: event.reportPaths,
            srtExportPaths: event.srtExportPaths,
            warnings: event.warnings,
            audioRepairSummary: event.audioRepairSummary,
            requestedMode: event.requestedMode,
            actualMode: event.actualMode,
            upgradeReason: event.upgradeReason,
          });
          console.log(`[STORE_UPDATE_DONE] event=merge-complete jobId=${event.jobId} store=completeJob`);

          // Record merge stats for historical ETA learning
          const completedJob = useMergeStore.getState().jobs.find(j => j.id === event.jobId);
          if (completedJob) {
            const now = Date.now();
            const startedAt = completedJob.startedAt || now;
            const totalTimeSeconds = Math.max(0, Math.floor((now - startedAt) / 1000));
            
            // Compute per-phase times from recorded phase start times
            const phaseStartTimes = completedJob.phaseTimes || {};
            const phaseOrder: Array<keyof PhaseTimes> = [
              'probing', 'validating', 'preparing', 'normalizing', 'writing', 'finalizing',
            ];
            const phaseTimes: Partial<PhaseTimes> = {};
            
            for (let i = 0; i < phaseOrder.length; i++) {
              const p = phaseOrder[i];
              const startTime = phaseStartTimes[p];
              if (startTime) {
                // Find next phase that actually started, or use 'now' if none
                let nextStart = now;
                for (let j = i + 1; j < phaseOrder.length; j++) {
                  const np = phaseOrder[j];
                  if (phaseStartTimes[np]) {
                    nextStart = phaseStartTimes[np]!;
                    break;
                  }
                }
                phaseTimes[p] = Math.max(0, Math.floor((nextStart - startTime) / 1000));
              }
            }

            const record: MergeStatsRecord = {
              id: generateId(),
              files: completedJob.request.inputFiles.length,
              totalMediaDurationSeconds: isNaN(completedJob.request.totalDuration) ? 0 : completedJob.request.totalDuration,
              mode: completedJob.request.mode as MergeMode,
              audioRepairMode: (completedJob.request.audioRepairMode as AudioRepairMode) ?? 'smart',
              largePlaylistStrategy: completedJob.request.largePlaylistStrategy,
              subtitleMode: completedJob.request.subtitleMode ?? 'embed',
              totalTimeSeconds: isNaN(totalTimeSeconds) ? 0 : totalTimeSeconds,
              phaseTimes,
              completedAt: now,
            };
            const settings = useAppStore.getState().settings;
            if (settings) {
              const history = settings.mergeStatsHistory ?? [];
              const updated = {
                ...settings,
                mergeStatsHistory: [record, ...history].slice(0, 20),
              };
              setSettings(updated);
              tauriCommands.saveSettings(updated).catch(() => {});
            }
          }

          // Note: Thumbnail cache is NOT cleared here.
          // Thumbnails are needed by the merge report UI (segments, thumbnails).
          // They will be cleaned up on app restart or when the playlist is cleared.

          const partCount = event.parts?.length ?? (event.outputPaths ? event.outputPaths.length : 0);
          const title = partCount > 1 ? `Merge complete! (${partCount} parts)` : 'Merge complete!';
          const outputName = event.outputPath.replace(/\\/g, '/').split('/').pop() ?? event.outputPath;

          // Build description including warnings and SRT export info
          const descriptionLines: string[] = [];
          if (partCount > 1) {
            descriptionLines.push(`${outputName} and ${partCount - 1} more parts created`);
          } else {
            descriptionLines.push(outputName);
          }

          // Add SRT export info
          if (event.srtExportPaths && event.srtExportPaths.length > 0) {
            const srtNames = event.srtExportPaths.map(p => p.replace(/\\/g, '/').split('/').pop() || p);
            descriptionLines.push(`📄 SRT: ${srtNames.join(', ')}`);
          }

          // Add audio repair summary
          if (event.audioRepairSummary && event.audioRepairSummary.filesRepaired > 0) {
            const r = event.audioRepairSummary;
            const parts: string[] = [];
            if (r.dueToCorruption > 0) parts.push(`${r.dueToCorruption} corruption`);
            if (r.dueToProfileMismatch > 0) parts.push(`${r.dueToProfileMismatch} profile`);
            if (r.dueToSafeMode > 0) parts.push(`${r.dueToSafeMode} safe-mode`);
            const reasonStr = parts.length > 0 ? ` (${parts.join(', ')})` : '';
            descriptionLines.push(`🔧 ${r.filesRepaired}/${r.totalFiles} file${r.filesRepaired !== 1 ? 's' : ''} re-encoded for audio repair${reasonStr}`);
          }

          // Show mode upgrade notification
          if (event.upgradeReason && event.requestedMode && event.actualMode && event.requestedMode !== event.actualMode) {
            descriptionLines.push(`🔄 Mode: ${event.requestedMode} → ${event.actualMode}`);
          }

          // Show warnings as secondary info
          if (event.warnings && event.warnings.length > 0) {
            event.warnings.forEach(w => descriptionLines.push(`⚠️ ${w}`));
          }

          showToast({
            type: event.warnings && event.warnings.length > 0 ? 'warning' : 'success',
            title,
            description: descriptionLines.join(' | '),
            durationMs: 10000,
          });

          // Clean up: clear logs and rate samples for this job
          clearLogs(event.jobId);
          delete rateSamplesRef.current[event.jobId];
          delete prevPhaseRef.current[event.jobId];
        });
        fns.push(u2);

        const u3 = await tauriEvents.onMergeError((event) => {
          if (cancelled) return;
          console.log(`[EVENT_RECEIVED] event=merge-error jobId=${event.jobId} error=${event.error}`);
          const translated = translateError(event.error);
          // Forward error to live log with user-friendly message + technical details
          addLog(event.jobId, {
            timestamp: Date.now(),
            level: 'error',
            message: `Merge failed: ${translated.userMessage}`,
            details: translated.details,
          });
          if (event.cancelled) {
            addLog(event.jobId, {
              timestamp: Date.now(),
              level: 'info',
              message: 'Merge cancelled by user',
            });
            cancelJob(event.jobId);
          } else {
            failJob(event.jobId, event.error);
            showToast({
              type: 'error',
              title: 'Merge failed',
              description: translated.userMessage,
              technicalDetails: translated.details !== translated.userMessage ? translated.details : undefined,
              durationMs: 8000,
            });
          }
          // Clean up per-job ref data to prevent memory leaks on cancel/fail
          delete rateSamplesRef.current[event.jobId];
          delete prevPhaseRef.current[event.jobId];
          delete phaseStartTimesRef.current[event.jobId];
        });
        fns.push(u3);

        if (!cancelled) {
          unlistenRef.current = fns;
        } else {
          fns.forEach((fn) => fn());
        }
      } catch (err) {
        console.error('Failed to set up merge event listeners:', err);
      }
    };

    setup();

    return () => {
      cancelled = true;
      // Clean up any listeners that were already registered
      unlistenRef.current.forEach((fn) => fn());
      unlistenRef.current = [];
    };
    // Stable action references in deps — only run once on mount
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const startMerge = useCallback(async (onPathSelected?: () => void): Promise<boolean> => {
    // Use getState() to read latest state — avoids stale closures
    const playlist = usePlaylistStore.getState();
    const store = useMergeStore.getState();
    const entries = playlist.entries;

    if (entries.length === 0) {
      useAppStore.getState().showToast({ type: 'error', title: 'Playlist is empty' });
      return false;
    }

    if (entries.length < 2) {
      useAppStore.getState().showToast({ type: 'error', title: 'Need at least 2 files to merge' });
      return false;
    }

    // ── Repeat validation ──────────────────────────────────────────────────
    if (store.repeatConfig?.enabled) {
      if (store.repeatConfig.untilDuration && (store.repeatConfig.targetDurationSeconds == null || store.repeatConfig.targetDurationSeconds <= 0)) {
        useAppStore.getState().showToast({ type: 'error', title: 'Invalid repeat duration', description: 'Set a target duration greater than 0 when using "Repeat until duration" mode.' });
        return false;
      }
      if (store.repeatConfig.byCount && (store.repeatConfig.repeatCount == null || store.repeatConfig.repeatCount < 2)) {
        useAppStore.getState().showToast({ type: 'error', title: 'Invalid repeat count', description: 'Repeat count must be at least 2.' });
        return false;
      }
    }

    const isSrtOnly = store.subtitleMode === 'srtMergeOnly';
    const isFastMkv = store.mergeMode === 'fastMkv';
    const isSmartMkv = store.mergeMode === 'smartMkv';
    const convertToMp4 = store.convertToMp4;

    // For Fast/Smart MKV: use .mkv when not converting, .mp4 when converting
    const defaultExtension = (isFastMkv || isSmartMkv) && !convertToMp4 ? 'mkv' : 'mp4';

    // ── Disk space pre-check ──────────────────────────────────────────────
    // Warn if estimated output exceeds available disk space (non-blocking)
    const estimatedSize = entries.reduce((t, e) => t + (e.size || 0), 0);
    const targetDir = store.outputPath || useAppStore.getState().settings?.lastExportDir;
    if (targetDir && estimatedSize > 0) {
      try {
        const space = await tauriCommands.getDiskSpace(targetDir);
        if (space.availableBytes > 0 && estimatedSize > space.availableBytes * 0.95) {
          useAppStore.getState().showToast({
            type: 'warning',
            title: 'Low disk space',
            description: `Estimated output (~${formatBytes(estimatedSize)}) may exceed available space (${formatBytes(space.availableBytes)}).`,
          });
        }
      } catch {
        // Disk space check failed — continue without blocking
      }
    }

    // Open native save dialog
    const savedPath = await saveOutputFileDialog(
      isSrtOnly
        ? (sanitizeFilename(store.outputFilename) || 'merged_subtitles') + '.srt'
        : ensureVideoExtension(sanitizeFilename(store.outputFilename) || 'merged_output', defaultExtension),
      store.outputPath || useAppStore.getState().settings?.lastExportDir || undefined,
      isSrtOnly
    );
    if (!savedPath) return false;

    // Force extension for MKV modes to prevent user confusion.
    // If user types "file.mp4" in SmartMkv mode, replace with "file.mkv".
    const outputPath = isSrtOnly
      ? (savedPath.toLowerCase().endsWith('.srt') ? savedPath : `${savedPath}.srt`)
      : forceVideoExtension(savedPath, defaultExtension);

    // ── Duplicate output path guard ──────────────────────────────────────────
    // Prevent queuing two merges to the same output file. This avoids data
    // corruption when two jobs race to write the same file. The backend also
    // writes a .merging marker as a second layer of protection, but we catch
    // duplicates at queue time to give immediate user feedback.
    const existingJobWithSamePath = useMergeStore.getState().jobs.some(
      (j) => normalizePath(j.request.outputPath) === normalizePath(outputPath) && j.progress.phase !== 'complete' && j.progress.phase !== 'failed' && j.progress.phase !== 'cancelled'
    );
    if (existingJobWithSamePath) {
      useAppStore.getState().showToast({
        type: 'error',
        title: 'Output file already in queue',
        description: `A merge to "${outputPath.replace(/\\/g, '/').split('/').pop()}" is already queued or running. Choose a different output path or cancel the existing job.`,
        durationMs: 8000,
      });
      return false;
    }

    // Capture configuration snapshots immediately to prevent UI state race conditions
    const mergeMode = store.mergeMode;
    const audioRepairMode = store.audioRepairMode;
    const largePlaylistStrategy = store.largePlaylistStrategy;
    const subtitleMode = store.subtitleMode;
    const exportMergedSrt = store.exportMergedSrt;
    const videoCodec = store.videoCodec;
    const audioCodec = store.audioCodec;
    const videoCrf = store.videoCrf;
    const videoPreset = store.videoPreset;
    const audioBitrate = store.audioBitrate;
    const targetResolution = store.targetResolution;
    const targetFps = store.targetFps;
    const hwAccel = store.hwAccel;
    const splitConfig = store.splitConfig;
    const namingConfig = store.namingConfig;
    const repeatConfig = store.repeatConfig;
    const cardEnabled = store.cardEnabled;
    const cardColor = store.cardColor;
    const cardFontColor = store.cardFontColor;
    const cardDuration = store.cardDuration;
    const cardShowInReport = store.cardShowInReport;
    const cardFrequency = store.cardFrequency;

    // Trigger the callback to let the UI close modal & redirect immediately
    if (onPathSelected) {
      onPathSelected();
    }

    // Process the probe, job initialization and backend call asynchronously
    (async () => {
      // Persist last export dir
      useMergeStore.getState().setOutputPath(getDirectory(outputPath));
      const settings = useAppStore.getState().settings;
      if (settings) {
        const updated = { ...settings, lastExportDir: getDirectory(outputPath) };
        useAppStore.getState().setSettings(updated);
        tauriCommands.saveSettings(updated).catch(console.error);
      }

      // ── Ensure all entries are probed before constructing the request ──
      const unprobedEntries = entries.filter(e => !e.mediaInfo);
      if (unprobedEntries.length > 0) {
        try {
          const results = await tauriCommands.batchProbe(unprobedEntries.map(e => e.path));
          const playlistStore = usePlaylistStore.getState();
          results.forEach((res, idx) => {
            if (typeof res !== 'string') {
              const entry = unprobedEntries[idx];
              if (entry) {
                playlistStore.setMediaInfo(entry.id, res);
              }
            }
          });
        } catch (probeErr) {
          console.warn('Pre-merge probe failed, continuing with available data:', probeErr);
        }
      }

      // Re-read entries from store after probe so inputDurations/externalSubtitles are fresh
      const probedEntries = usePlaylistStore.getState().entries;
      const totalDur = usePlaylistStore.getState().getTotalDuration();

      // Final validation guard — prevent sending zero-duration request to backend
      if (totalDur <= 0) {
        useAppStore.getState().showToast({
          type: 'error',
          title: 'Cannot start merge',
          description: 'File durations could not be determined. Ensure video files are valid and try again.',
        });
        return;
      }

      const jobId = generateId();
      const isCustom = mergeMode === 'custom';
      
      // Extract external subtitles from mediaInfo
      const externalSubtitles = probedEntries.map(e => {
        const extSub = e.mediaInfo?.subtitleStreams?.find(s => s.isExternal);
        return extSub?.path ?? null;
      });

      // Build per-file selected subtitle stream indices from store
      const selectedStreamIndices = probedEntries.map(e =>
        useMergeStore.getState().selectedSubtitleStreamIndices[e.id] ?? null
      );
      const hasCustomSelections = selectedStreamIndices.some(idx => idx !== null);

      const request: MergeRequest = {
        jobId,
        inputFiles: probedEntries.map((e) => e.path),
        mediaInfos: probedEntries.map((e) => e.mediaInfo).filter((m): m is MediaInfo => m !== null),
        inputNames: probedEntries.map((e) => e.name),
        inputDurations: probedEntries.map((e) => e.mediaInfo?.duration ?? 0),
        inputThumbnails: probedEntries.map((e) => e.thumbnailPath),
        externalSubtitles,
        outputPath,
        mode: mergeMode,
        audioRepairMode,
        ...(largePlaylistStrategy && { largePlaylistStrategy }),
        ...(isFastMkv && { convertToMp4 }),
        ...(isSmartMkv && { convertToMp4 }),
        totalDuration: totalDur,
        subtitleMode,
        exportMergedSrt,
        ...(hasCustomSelections && {
          selectedSubtitleStreamIndices: selectedStreamIndices,
        }),
        ...(isCustom && {
          videoCodec,
          audioCodec,
          videoCrf,
          videoPreset,
          audioBitrate,
          targetResolution: targetResolution || undefined,
          targetFps: targetFps || undefined,
          hwAccel: hwAccel || undefined,
        }),
        // Deep audio validation when smart mode is selected
        ...(audioRepairMode === 'smart' && { validateAudio: true }),
        ...(splitConfig.mode !== 'none' && {
          splitConfig,
        }),
        ...(namingConfig && {
          namingConfig,
        }),
        ...(repeatConfig && repeatConfig.enabled && {
          repeatConfig,
        }),
        ...(cardEnabled && {
          cardConfig: {
            color: cardColor,
            fontColor: cardFontColor,
            duration: cardDuration,
            showInReport: cardShowInReport,
            frequency: cardFrequency,
          },
        }),
      };

      // Add to store immediately so the job appears in the queue
      useMergeStore.getState().startJob(request);

      // ── Reset the playlist workspace and temporary merge configs immediately ──
      // This allows the user to immediately import a new folder or playlist while
      // the backend checks/validates the current job.
      // NOTE: outputFilename is NOT cleared — preserves manual naming across merge cycles.
      usePlaylistStore.getState().newPlaylist();
      useMergeStore.setState({
        compatibilityReport: null,
        selectedSubtitleStreamIndices: {},
        selectedPlaylistIds: [],
      });
      try {
        await tauriCommands.startMerge(request);
      } catch (err) {
        console.error('[Merge] Failed to start merge:', err);
        // Check if the job was already cancelled before we got the error.
        // This prevents overriding 'cancelled' with 'failed' when cancel
        // was initiated during a long-running phase (e.g., normalization).
        const currentJob = useMergeStore.getState().jobs.find(j => j.id === jobId);
        if (currentJob?.progress.phase !== 'cancelled') {
          useMergeStore.getState().failJob(jobId, String(err));
          const translated = translateError(String(err));
          useAppStore.getState().showToast({
            type: 'error',
            title: 'Failed to start merge',
            description: translated.userMessage,
            technicalDetails: translated.details !== translated.userMessage ? translated.details : undefined,
            durationMs: 8000,
          });
        }
      }
    })();

    return true;
  }, []);

  const cancelMerge = useCallback(async (jobId: string) => {
    try {
      await tauriCommands.cancelMerge(jobId);
      // Immediately update the frontend store so the user sees instant feedback.
      // The backend will eventually detect the cancel flag and clean up.
      useMergeStore.getState().cancelJob(jobId);
    } catch (err) {
      console.error('Cancel error:', err);
    }
  }, []);

  return { startMerge, cancelMerge };
}
