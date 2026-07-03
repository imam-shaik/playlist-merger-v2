import React, { useEffect, useCallback, useRef, useState } from 'react';
import { motion } from 'framer-motion';
import { X, Zap, AlertTriangle, Loader2, Palette, Folder, FileVideo } from 'lucide-react';
import { useMergeStore } from '@/store/mergeStore';
import { usePlaylistStore } from '@/store/playlistStore';
import { useAppStore } from '@/store/appStore';
import { useMerge } from '@/hooks/useMerge';
import { tauriCommands } from '@/tauri/commands';
import { cn } from '@/utils/cn';

// Subcomponents from merge feature
import { MergeSummaryCard } from '@/features/merge/MergeSummaryCard';
import { OutputSettings } from '@/features/merge/OutputSettings';
import { MergeModeSelector } from '@/features/merge/MergeModeSelector';
import { EncodingSettings } from '@/features/merge/EncodingSettings';
import { CompatibilitySection } from '@/features/merge/CompatibilitySection';
import { SplitConfigSection } from '@/features/merge/SplitConfigSection';
import { MergeNamingSection } from '@/features/merge/MergeNamingSection';
import { MergeActionBar } from '@/features/merge/MergeActionBar';
import { LargePlaylistStrategyDialog } from '@/features/merge/LargePlaylistStrategyDialog';
import { SubtitleTrackSelector } from './SubtitleTrackSelector';
import { CANVAS_COLORS } from '@/constants';

// ─── Canvas Color Swatch Picker ────────────────────────────────────────────

function CanvasCardConfig() {
  const cardEnabled = useMergeStore((s) => s.cardEnabled);
  const setCardEnabled = useMergeStore((s) => s.setCardEnabled);
  const cardColor = useMergeStore((s) => s.cardColor);
  const setCardColor = useMergeStore((s) => s.setCardColor);
  const cardDuration = useMergeStore((s) => s.cardDuration);
  const setCardDuration = useMergeStore((s) => s.setCardDuration);
  const cardShowInReport = useMergeStore((s) => s.cardShowInReport);
  const setCardShowInReport = useMergeStore((s) => s.setCardShowInReport);
  const cardFontColor = useMergeStore((s) => s.cardFontColor);
  const cardFrequency = useMergeStore((s) => s.cardFrequency);
  const setCardFrequency = useMergeStore((s) => s.setCardFrequency);

  return (
    <div className="p-4 rounded-xl border border-border/80 bg-bg-surface/30 backdrop-blur-md flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Palette className="w-4 h-4 text-accent-400" />
          <div className="flex flex-col gap-0.5">
            <span className="text-xs font-semibold text-text-primary">Canvas Overlay Cards</span>
            <span className="text-[10px] text-text-muted">
              Insert color transition cards between merged videos with next-video info
            </span>
          </div>
        </div>
        <button
          type="button"
          onClick={() => setCardEnabled(!cardEnabled)}
          title="Canvas cards are rendered as short video clips (2–5s) in H.264. Original videos remain stream-copied (lossless). Adds extra duration between segments equal to card duration × (file count − 1)."
          className={cn(
            "relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none",
            cardEnabled ? "bg-accent-500" : "bg-bg-overlay border-border"
          )}
          role="switch"
          aria-checked={cardEnabled}
        >
          <span
            aria-hidden="true"
            className={cn(
              "pointer-events-none inline-block h-4 w-4 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out",
              cardEnabled ? "translate-x-4" : "translate-x-0"
            )}
          />
        </button>
      </div>

      {cardEnabled && (
        <motion.div
          initial={{ opacity: 0, height: 0 }}
          animate={{ opacity: 1, height: 'auto' }}
          className="flex flex-col gap-3 overflow-hidden"
        >
          {/* Color Swatches */}
          <div className="flex flex-col gap-1.5">
            <span className="text-[10px] font-semibold text-text-muted uppercase tracking-wider">Background Color</span>
            <div className="flex flex-wrap gap-2">
              {CANVAS_COLORS.map((swatch) => (
                <button
                  key={swatch.color}
                  type="button"
                  onClick={() => setCardColor(swatch.color)}
                  className={cn(
                    'w-8 h-8 rounded-lg border-2 transition-all hover:scale-110',
                    cardColor === swatch.color
                      ? 'border-accent-400 ring-2 ring-accent-500/30 scale-110'
                      : 'border-border hover:border-text-muted'
                  )}
                  style={{ backgroundColor: swatch.color }}
                  title={`${swatch.label} — ${swatch.color}`}
                  aria-label={`Select ${swatch.label} canvas color`}
                />
              ))}
            </div>
          </div>

          {/* Preview */}
          <div
            className="rounded-lg border border-border/60 px-4 py-3 flex flex-col items-center justify-center gap-1 min-h-[64px]"
            style={{ backgroundColor: cardColor }}
          >
            <span className="text-[11px] font-semibold" style={{ color: cardFontColor }}>
              Next: Example Video.mp4
            </span>
            <span className="text-[9px] font-medium opacity-80" style={{ color: cardFontColor }}>
              00:03:30
            </span>
          </div>

          {/* Card Frequency */}
          <div className="flex items-center justify-between">
            <span className="text-[10px] font-semibold text-text-muted uppercase tracking-wider">Frequency</span>
            <div className="flex bg-bg-overlay rounded-lg p-0.5 border border-border/40">
              <button
                type="button"
                onClick={() => setCardFrequency('perVideo')}
                className={cn(
                  "px-2 py-1 rounded-md text-[9px] font-medium transition-all flex items-center gap-1.5",
                  cardFrequency === 'perVideo' 
                    ? "bg-accent-500 text-white shadow-sm" 
                    : "text-text-secondary hover:text-text-primary"
                )}
              >
                <FileVideo className="w-3 h-3" />
                Per Video
              </button>
              <button
                type="button"
                onClick={() => setCardFrequency('perFolder')}
                className={cn(
                  "px-2 py-1 rounded-md text-[9px] font-medium transition-all flex items-center gap-1.5",
                  cardFrequency === 'perFolder' 
                    ? "bg-accent-500 text-white shadow-sm" 
                    : "text-text-secondary hover:text-text-primary"
                )}
              >
                <Folder className="w-3 h-3" />
                Per Folder
              </button>
            </div>
          </div>

          {/* Duration slider */}
          <div className="flex flex-col gap-1.5">
            <div className="flex items-center justify-between">
              <span className="text-[10px] font-semibold text-text-muted uppercase tracking-wider">
                Card Duration
              </span>
              <span className="text-[10px] font-mono text-text-secondary">{cardDuration}s</span>
            </div>
            <input
              type="range"
              min="1"
              max="8"
              step="0.5"
              value={cardDuration}
              onChange={(e) => setCardDuration(parseFloat(e.target.value))}
              className="w-full h-1.5 bg-bg-overlay rounded-full appearance-none cursor-pointer accent-accent-500 [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:w-3.5 [&::-webkit-slider-thumb]:h-3.5 [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-accent-500 [&::-webkit-slider-thumb]:shadow-md"
              aria-label="Card duration in seconds"
            />
            <div className="flex justify-between text-[8px] text-text-disabled">
              <span>1s</span>
              <span>4s</span>
              <span>8s</span>
            </div>
          </div>

          {/* Show in report toggle */}
          <div className="flex items-center justify-between">
            <span className="text-[10px] font-medium text-text-secondary">Show canvas entries in merge report</span>
            <button
              type="button"
              onClick={() => setCardShowInReport(!cardShowInReport)}
              className={cn(
                "relative inline-flex h-4 w-7 shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none",
                cardShowInReport ? "bg-accent-500" : "bg-bg-overlay border-border"
              )}
              role="switch"
              aria-checked={cardShowInReport}
            >
              <span
                aria-hidden="true"
                className={cn(
                  "pointer-events-none inline-block h-3 w-3 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out",
                  cardShowInReport ? "translate-x-3" : "translate-x-0"
                )}
              />
            </button>
          </div>
        </motion.div>
      )}
    </div>
  );
}

interface MergeConfigModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export function MergeConfigModal({ isOpen, onClose }: MergeConfigModalProps) {
  const entries = usePlaylistStore((s) => s.entries);
  const compatibilityReport = useMergeStore((s) => s.compatibilityReport);
  const isCheckingCompat = useMergeStore((s) => s.isCheckingCompat);
  const checkCompatSettings = useAppStore((s) => s.settings?.checkCompatBeforeMerge);
  const audioRepairMode = useMergeStore((s) => s.audioRepairMode);
  const { startMerge } = useMerge();

  const subtitleMode = useMergeStore((s) => s.subtitleMode);
  const setSubtitleMode = useMergeStore((s) => s.setSubtitleMode);
  const exportMergedSrt = useMergeStore((s) => s.exportMergedSrt);
  const setExportMergedSrt = useMergeStore((s) => s.setExportMergedSrt);
  const mergeMode = useMergeStore((s) => s.mergeMode);

  // ── Shared probe cache ref ────────────────────────────────────────────────
  // Avoids double-probing when both subtitle detection and compat check run.
  // Once a probe completes, its results are cached here so the other flow
  // can reuse them instead of calling batchProbe again.
  type ProbeResults = Array<import('@/types').MediaInfo | string>;
  const probeCacheRef = useRef<Promise<ProbeResults> | null>(null);
  // Generation counter to prevent stale compat check results from overwriting newer ones
  const compatGenRef = useRef(0);

  // ── Subtitle detection state ───────────────────────────────────────────────
  const [isProbingSubtitles, setIsProbingSubtitles] = useState(false);
  const [subtitlesDetected, setSubtitlesDetected] = useState<boolean | null>(null);
  const [bitmapSubtitlesDetected, setBitmapSubtitlesDetected] = useState(false);

  const hasSubtitles = subtitlesDetected === true;

  // ── Large playlist strategy dialog state ─────────────────────────────────
  const [showStrategyDialog, setShowStrategyDialog] = useState(false);
  const LARGE_PLAYLIST_THRESHOLD = 60;
  const shouldShowStrategyDialog = audioRepairMode === 'smart' && entries.length >= LARGE_PLAYLIST_THRESHOLD;

  const modalRef = useRef<HTMLDivElement>(null);
  const lastProbedPathsRef = useRef<string>('');

  /**
   * Shared probe runner — caches in-flight batchProbe so that if both
   * subtitle detection and compat check request a probe concurrently,
   * only one ffprobe batch is dispatched.
   */
  const getProbeResults = useCallback(async (): Promise<ProbeResults> => {
    if (probeCacheRef.current) return probeCacheRef.current;

    const promise = (async () => {
      const entriesSnapshot = usePlaylistStore.getState().entries;
      const paths = entriesSnapshot.map(e => e.path);
      const idByPath = new Map(entriesSnapshot.map(e => [e.path, e.id]));

      try {
        const results = await tauriCommands.batchProbe(paths);

        // Batch-update the store with fresh probe data (avoids re-render storm)
        const ps = usePlaylistStore.getState();
        const batchUpdates = new Map<string, import('@/types').MediaInfo>();
        results.forEach((res, idx) => {
          if (typeof res !== 'string') {
            const path = paths[idx];
            const id = idByPath.get(path);
            if (id) {
              batchUpdates.set(id, res);
            }
          }
        });
        if (batchUpdates.size > 0) {
          ps.batchSetMediaInfo(batchUpdates);
        }

        return results;
      } catch (err) {
        console.warn('batchProbe failed:', err);
        // On failure, still clear the cache so next call retries
        probeCacheRef.current = null;
        throw err;
      }
    })();

    probeCacheRef.current = promise;
    return promise;
  }, []);

  // ── Compatibility check logic ──
  const runCompatCheck = useCallback(async () => {
    const entriesList = usePlaylistStore.getState().entries;
    if (entriesList.length < 2) {
      useMergeStore.getState().setCompatibilityReport({
        isCompatible: true,
        issues: [],
        recommendedMode: 'lossless',
      });
      return;
    }
    const currentOutput = useMergeStore.getState().outputFilename;

    // Determine what name would be generated for current entries
    const playlistName = usePlaylistStore.getState().playlistName;
    const nameSource = (playlistName && playlistName !== 'New Playlist')
      ? playlistName
      : entriesList[0].name;
    const expectedName = `${nameSource} (${entriesList.length} videos merged)`;

    // Regenerate output filename if:
    // 1. At default value ('merged_output' or empty)
    // 2. Current name was auto-generated but for different entries (name or count changed)
    const autoGenPattern = /^.+ \(.+videos merged\)$/;
    const isAutoGenerated = autoGenPattern.test(currentOutput);
    const isForDifferentEntries = isAutoGenerated && currentOutput !== expectedName;

    if (currentOutput === 'merged_output' || currentOutput === '' || isForDifferentEntries) {
      useMergeStore.getState().setOutputFilename(expectedName);
    }

    useMergeStore.getState().setCheckingCompat(true);
    useMergeStore.getState().setCompatibilityReport(null);
    // Increment generation — if another check starts before this one finishes,
    // its increment will be higher and this check's result will be discarded.
    const gen = ++compatGenRef.current;
    try {
      // Use shared probe — reuses cached results if subtitle probe already ran
      const probed = await getProbeResults();

      const report: import('@/types').CompatibilityReport = {
        isCompatible: true,
        issues: [],
        recommendedMode: 'lossless',
      };
      const validInfos = probed.filter((r): r is import('@/types').MediaInfo => typeof r !== 'string');
      if (validInfos.length < 2) {
        // Only write if this is still the latest check (no newer check started)
        if (compatGenRef.current === gen) {
      // Only write if this is still the latest check (no newer check started)
      if (compatGenRef.current === gen) {
        useMergeStore.getState().setCompatibilityReport(report);
      }
        }
        return;
      }
      const ref = validInfos[0];
      const rv = ref.videoStreams?.[0];
      const ra = ref.audioStreams?.[0];

      validInfos.slice(1).forEach((info, i) => {
        const idx = i + 2;
        const cv = info.videoStreams?.[0];
        const ca = info.audioStreams?.[0];
        const filename = entriesList[i + 1]?.name ?? `File ${idx}`;
        if (rv && cv) {
          if (rv.codecName !== cv.codecName) {
            report.issues.push({
              kind: 'videoCodecMismatch',
              severity: 'warning',
              description: `"${filename}" uses ${cv.codecName}, others use ${rv.codecName} (will be auto-normalized)`,
              affectedFiles: [info.path],
            });
          }
          if ((rv.width !== cv.width || rv.height !== cv.height) && cv.width) {
            report.issues.push({
              kind: 'resolutionMismatch',
              severity: 'warning',
              description: `"${filename}" is ${cv.width}×${cv.height}, reference is ${rv.width}×${rv.height}`,
              affectedFiles: [info.path],
            });
          }
          if (rv.fps && cv.fps && Math.abs(rv.fps - cv.fps) > 0.01) {
            report.issues.push({
              kind: 'fpsMismatch',
              severity: 'warning',
              description: `"${filename}" is ${cv.fps.toFixed(2)} fps vs ${rv.fps.toFixed(2)} fps (will be auto-normalized)`,
              affectedFiles: [info.path],
            });
          }
          if (rv.pixelFormat && rv.pixelFormat !== cv.pixelFormat) {
            report.issues.push({
              kind: 'pixelFormatMismatch',
              severity: 'warning',
              description: `"${filename}" pixel format ${cv.pixelFormat} vs ${rv.pixelFormat}`,
              affectedFiles: [info.path],
            });
          }
        }
        if (ra && ca) {
          if (ra.codecName !== ca.codecName) {
            report.issues.push({
              kind: 'audioCodecMismatch',
              severity: 'warning',
              description: `"${filename}" audio ${ca.codecName} vs ${ra.codecName} (will be auto-normalized)`,
              affectedFiles: [info.path],
            });
          }
          if (ra.sampleRate && ca.sampleRate && ra.sampleRate !== ca.sampleRate) {
            report.issues.push({
              kind: 'sampleRateMismatch',
              severity: 'warning',
              description: `"${filename}" sample rate ${ca.sampleRate}Hz vs ${ra.sampleRate}Hz (will be auto-normalized)`,
              affectedFiles: [info.path],
            });
          }
        }
      });
      // ── Dynamic recommendation: lossless > smartMkv > custom ──
      const hasErrors = report.issues.some((i) => i.severity === 'error');
      const hasWarnings = report.issues.some((i) => i.severity === 'warning');
      const currentSubtitleMode = useMergeStore.getState().subtitleMode;
      const burnSubtitles = currentSubtitleMode === 'burn';
      if (hasErrors || burnSubtitles) {
        report.recommendedMode = 'custom';
      } else if (hasWarnings) {
        report.recommendedMode = 'smartMkv';
      } else {
        report.recommendedMode = 'lossless';
      }

      // ── 2. Backend deep compatibility check (codec transitions, audio-only) ──
      try {
        const mergeModeValue = useMergeStore.getState().mergeMode;
        const inputFiles = entriesList.map((e) => e.path);
        const backendResult = await tauriCommands.checkMergeCompatibility(
          inputFiles,
          validInfos,
          mergeModeValue,
        );
        if (!backendResult.losslessWillApply && backendResult.autoUpgradeReason) {
          // Lossless will auto-upgrade → recommend Smart MKV or Custom
          report.isCompatible = false;
          const backendHasErrors = backendResult.incompatibleFiles.length > 0;
          report.recommendedMode = backendHasErrors ? 'custom' : 'smartMkv';
        }

        // Merge backend errors (blocking issues: codec transitions, audio-only files)
        for (const item of backendResult.incompatibleFiles) {
          const kind = item.reason.toLowerCase().includes('audio-only')
            ? 'videoCodecMismatch' as const
            : 'videoCodecMismatch' as const;
          report.issues.push({
            kind,
            severity: 'error' as const,
            description: item.reason,
            affectedFiles: [inputFiles[item.fileIndex] ?? ''],
          });
        }

        // Merge backend warnings (non-blocking: channel outliers, etc.)
        for (const w of backendResult.warnings) {
          const kind = w.reason.toLowerCase().includes('channel')
            ? 'channelMismatch' as const
            : w.reason.toLowerCase().includes('sample rate')
            ? 'sampleRateMismatch' as const
            : w.reason.toLowerCase().includes('audio codec')
            ? 'audioCodecMismatch' as const
            : 'videoCodecMismatch' as const;
          report.issues.push({
            kind,
            severity: 'warning' as const,
            description: w.reason,
            affectedFiles: [inputFiles[w.fileIndex] ?? ''],
          });
        }
      } catch (err) {
        console.warn('Backend compatibility check failed (non-fatal):', err);
      }

      // Only write if this is still the latest check (no newer check started)
      if (compatGenRef.current === gen) {
        useMergeStore.getState().setCompatibilityReport(report);
      }
    } catch (err) {
      console.warn('Compatibility check failed:', err);
      useAppStore.getState().showToast({
        type: 'warning',
        title: 'Compatibility check failed',
        description: String(err).substring(0, 100),
      });
    } finally {
      useMergeStore.getState().setCheckingCompat(false);
    }
  }, [getProbeResults]);

  // ── Subtitle re-probe on modal open ────────────────────────────────────────
  // Always re-probe ALL entries when the modal opens so that external subtitle
  // files placed alongside videos after the initial import are detected.
  // Uses the shared probe cache to avoid double-probing with the compat check.
  useEffect(() => {
    if (!isOpen || entries.length === 0) return;

    let cancelled = false;
    
    // [Perf] Only invalidate cache if file list changed
    const currentPaths = entries.map(e => e.path).join('|');
    if (currentPaths !== lastProbedPathsRef.current) {
      probeCacheRef.current = null;
      lastProbedPathsRef.current = currentPaths;
    }

    setIsProbingSubtitles(true);
    setSubtitlesDetected(null);

    (async () => {
      try {
        const results = await getProbeResults();
        if (cancelled) return;

        const found = results.some(
          (r) => typeof r !== 'string' && (r.subtitleStreams?.length ?? 0) > 0
        );
        const hasBitmap = results.some(
          (r) => typeof r !== 'string' && r.subtitleStreams?.some((s) => s.isBitmap) === true
        );

        if (!cancelled) {
          setSubtitlesDetected(found);
          setBitmapSubtitlesDetected(hasBitmap);
        }
      } catch {
        if (!cancelled) {
          setSubtitlesDetected(false);
        }
      } finally {
        if (!cancelled) {
          setIsProbingSubtitles(false);
        }
      }
    })();

    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen, entries.length, getProbeResults]);

  // Run compatibility check on mount / entries length changes
  useEffect(() => {
    if (isOpen && entries.length > 0 && checkCompatSettings) {
      runCompatCheck();
    }
  }, [isOpen, entries.length, checkCompatSettings, runCompatCheck]);

  // Re-run compat check when subtitle mode or merge mode changes
  // (these affect the recommendation but don't change entries.length)
  useEffect(() => {
    if (isOpen && entries.length > 0 && checkCompatSettings && compatibilityReport) {
      runCompatCheck();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [subtitleMode, mergeMode]);

  // Handle Escape key
  useEffect(() => {
    if (!isOpen) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  const handleMergeSubmit = async () => {
    if (shouldShowStrategyDialog) {
      const store = useMergeStore.getState();
      const settings = useAppStore.getState().settings;
      if (!store.largePlaylistStrategy && settings?.largePlaylistDefault) {
        store.setLargePlaylistStrategy(settings.largePlaylistDefault);
      }
      if (!store.largePlaylistStrategy) {
        setShowStrategyDialog(true);
        return;
      }
    }
    await startMerge(() => {
      useAppStore.getState().setScreen('merge');
      onClose();
    });
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
      {/* Backdrop click close handler */}
      <div className="absolute inset-0" onClick={onClose} />

      <motion.div
        ref={modalRef}
        initial={{ opacity: 0, scale: 0.95, y: 15 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        exit={{ opacity: 0, scale: 0.95, y: 15 }}
        transition={{ duration: 0.2, ease: [0.16, 1, 0.3, 1] }}
        className="relative z-10 bg-bg-elevated border border-border shadow-modal rounded-xl w-full max-w-2xl max-h-[85vh] flex flex-col overflow-hidden"
      >
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-4 border-b border-border/80">
          <div className="flex items-center gap-2">
            <Zap className="w-4 h-4 text-accent-400" />
            <div>
              <h2 className="text-sm font-semibold text-text-primary">Configure Playlist Merge</h2>
              <p className="text-[10px] text-text-muted mt-0.5">Customize output settings and compatibility options</p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-1 rounded-lg text-text-muted hover:text-text-primary hover:bg-bg-overlay transition-all"
            aria-label="Close configuration modal"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Form Body (Scrollable) */}
        <div className="flex-1 overflow-y-auto p-5 space-y-5 scrollbar-thin">
          {/* Summary Card */}
          <MergeSummaryCard />

          {/* Output Settings */}
          <OutputSettings />

          {/* Naming Section */}
          <MergeNamingSection />

          {/* Merge Mode Selector */}
          {subtitleMode !== 'srtMergeOnly' && <MergeModeSelector report={compatibilityReport} />}

          {/* Encoding Settings */}
          {subtitleMode !== 'srtMergeOnly' && mergeMode !== 'fastMkv' && mergeMode !== 'smartMkv' && <EncodingSettings />}

          {/* Compatibility Checker */}
          {subtitleMode !== 'srtMergeOnly' && mergeMode !== 'fastMkv' && mergeMode !== 'smartMkv' && (
            <CompatibilitySection
              report={compatibilityReport}
              isChecking={isCheckingCompat}
              entryCount={entries.length}
              onRecheck={runCompatCheck}
            />
          )}

          {/* Subtitle Mode Selector */}
          {mergeMode !== 'fastMkv' && (
          <div className="p-4 rounded-xl border border-border/80 bg-bg-surface/30 backdrop-blur-md flex flex-col gap-3">
            <div className="flex items-center gap-2">
              <svg className="w-4 h-4 text-accent-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M2 7a2 2 0 012-2h16a2 2 0 012 2v10a2 2 0 01-2 2H4a2 2 0 01-2-2V7z" />
                <path d="M6 12h2m4 0h6m-8 4h8" />
              </svg>
              <div className="flex flex-col gap-0.5">
                <span className="text-xs font-semibold text-text-primary">Subtitles</span>
                <span className="text-[10px] text-text-muted">
                  Choose how subtitle tracks are handled in the merged output
                </span>
              </div>
            </div>

            {/* Subtitle detection status */}
            {isProbingSubtitles && (
              <div className="px-3 py-2 rounded-lg bg-accent-500/10 border border-accent-500/20 flex items-center gap-2">
                <Loader2 className="w-3.5 h-3.5 text-accent-400 shrink-0 animate-spin" />
                <p className="text-[10px] text-accent-400 leading-normal">
                  Scanning for subtitle files…
                </p>
              </div>
            )}
            {!isProbingSubtitles && !hasSubtitles && subtitlesDetected === false && (
              <div className="px-3 py-2 rounded-lg bg-warning/5 border border-warning/20 flex items-center gap-2">
                <AlertTriangle className="w-3.5 h-3.5 text-warning shrink-0" />
                <p className="text-[10px] text-warning leading-normal">
                  No subtitles detected. External SRT files placed alongside videos will be matched during merge.
                </p>
              </div>
            )}
            {hasSubtitles && (
              <div className="px-3 py-2 rounded-lg bg-success/5 border border-success/20 flex items-center gap-2">
                <svg className="w-3.5 h-3.5 text-success shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <polyline points="20 6 9 17 4 12" />
                </svg>
                <p className="text-[10px] text-success leading-normal">
                  Subtitles detected in one or more files.
                </p>
              </div>
            )}
            {hasSubtitles && bitmapSubtitlesDetected && (
              <div className="px-3 py-2 rounded-lg bg-danger/5 border border-danger/20 flex items-center gap-2">
                <AlertTriangle className="w-3.5 h-3.5 text-danger shrink-0" />
                <p className="text-[10px] text-danger leading-normal">
                  Bitmap subtitles (PGS/VobSub) detected — these cannot be exported as SRT. Use "Burn Into Video" to burn them into frames, or "Embed Track" to preserve the original stream (MKV output only).
                </p>
              </div>
            )}

            {/* Radio group for subtitle mode */}
            <div className="flex flex-col gap-1.5">
              <span className="text-[10px] font-semibold text-text-muted uppercase tracking-wider">Handling Mode</span>
              <div className="flex flex-col gap-1">
                {[
                  { value: 'none' as const, label: 'No Subtitles', desc: 'Skip all subtitle processing — output has no subtitle track' },
                  { value: 'embed' as const, label: 'Embed Track', desc: 'Mux subtitles as a subtitle track in the output video (lossless, default)' },
                  { value: 'exportSrt' as const, label: 'Export As SRT', desc: 'Merge videos as usual, then export subtitles as a standalone .srt file (not embedded in video)' },
                  { value: 'srtMergeOnly' as const, label: 'Merge SRTs Only', desc: 'Skip video entirely — combine only subtitle (.srt) files into a single .srt file with no video output' },
                  { value: 'burn' as const, label: 'Burn Into Video', desc: 'Render subtitles directly onto video frames (requires re-encode)' },
                ].map((option) => (
                  <button
                    key={option.value}
                    type="button"
                    onClick={() => setSubtitleMode(option.value)}
                    className={cn(
                      'flex items-start gap-3 px-3 py-2 rounded-lg border text-left transition-all',
                      subtitleMode === option.value
                        ? 'border-accent-500/50 bg-accent-500/10'
                        : 'border-border/40 bg-transparent hover:border-border/80'
                    )}
                  >
                    <span className={cn(
                      'mt-0.5 w-3.5 h-3.5 rounded-full border-2 shrink-0 flex items-center justify-center transition-colors',
                      subtitleMode === option.value
                        ? 'border-accent-500 bg-accent-500'
                        : 'border-text-disabled'
                    )}>
                      {subtitleMode === option.value && (
                        <span className="w-1.5 h-1.5 rounded-full bg-white" />
                      )}
                    </span>
                    <div className="flex flex-col">
                      <span className={cn(
                        'text-[11px] font-semibold',
                        subtitleMode === option.value ? 'text-accent-400' : 'text-text-primary'
                      )}>{option.label}</span>
                      <span className="text-[9px] text-text-muted leading-tight mt-0.5">{option.desc}</span>
                    </div>
                  </button>
                ))}
              </div>
            </div>

            {/* Export merged SRT checkbox — hidden when exportSrt mode already produces .srt */}
            {subtitleMode !== 'srtMergeOnly' && subtitleMode !== 'exportSrt' && (
              <div className="flex items-center justify-between pt-1">
                <div className="flex flex-col gap-0.5">
                  <span className="text-[11px] font-semibold text-text-primary">Export Merged SRT</span>
                  <span className="text-[9px] text-text-muted">
                    Generate a standalone merged subtitle file (.srt) alongside the video output
                  </span>
                </div>
                <button
                  type="button"
                  onClick={() => setExportMergedSrt(!exportMergedSrt)}
                  className={cn(
                    "relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none",
                    exportMergedSrt ? "bg-accent-500" : "bg-bg-overlay border-border"
                  )}
                  role="switch"
                  aria-checked={exportMergedSrt}
                >
                  <span
                    aria-hidden="true"
                    className={cn(
                      "pointer-events-none inline-block h-4 w-4 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out",
                      exportMergedSrt ? "translate-x-4" : "translate-x-0"
                    )}
                  />
                </button>
              </div>
            )}

            {/* Embedded subtitle track selector (only shows for files with 2+ embedded tracks) */}
            {subtitleMode !== 'none' && subtitleMode !== 'exportSrt' && (
              <SubtitleTrackSelector />
            )}

            {/* Info when burn mode is selected */}
            {subtitleMode === 'burn' && (
              <div className="px-3 py-2 rounded-lg bg-accent-500/10 border border-accent-500/20 flex items-start gap-2">
                <AlertTriangle className="w-3.5 h-3.5 text-warning shrink-0 mt-0.5" />
                <p className="text-[10px] text-text-secondary leading-normal">
                  Burn mode re-encodes the video. This is slower than Embed mode and may reduce quality.
                  Use Embed mode for lossless subtitle handling.
                </p>
              </div>
            )}
          </div>
          )}

          {/* Canvas Overlay Cards */}
          {subtitleMode !== 'srtMergeOnly' && <CanvasCardConfig />}

          {/* Split Configuration */}
          {subtitleMode !== 'srtMergeOnly' && <SplitConfigSection />}
        </div>

        {/* Action Bar */}
        <MergeActionBar report={compatibilityReport} onMerge={handleMergeSubmit} />
      </motion.div>

      {/* Large Playlist Strategy Dialog */}
      {showStrategyDialog && (
        <LargePlaylistStrategyDialog
          fileCount={entries.length}
          onConfirm={async () => {
            setShowStrategyDialog(false);
            await startMerge(() => {
              useAppStore.getState().setScreen('merge');
              onClose();
            });
          }}
          onCancel={() => setShowStrategyDialog(false)}
        />
      )}
    </div>
  );
}
