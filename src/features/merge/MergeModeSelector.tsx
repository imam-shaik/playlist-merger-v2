import React from 'react';
import { Zap, Sliders, CheckCircle2, AlertTriangle, Gauge, SearchCheck, ShieldCheck, FastForward, Brain } from 'lucide-react';
import { cn } from '@/utils/cn';
import { Badge } from '@/components/ui/Badge';
import { useMergeStore } from '@/store/mergeStore';
import type { MergeMode, AudioRepairMode, CompatibilityReport } from '@/types';

interface MergeModeSelectorProps {
  report: CompatibilityReport | null;
}

export function MergeModeSelector({ report }: MergeModeSelectorProps) {
  const mergeMode = useMergeStore((s) => s.mergeMode);
  const setMergeMode = useMergeStore((s) => s.setMergeMode);
  const audioRepairMode = useMergeStore((s) => s.audioRepairMode);
  const setAudioRepairMode = useMergeStore((s) => s.setAudioRepairMode);
  const convertToMp4 = useMergeStore((s) => s.convertToMp4);
  const setConvertToMp4 = useMergeStore((s) => s.setConvertToMp4);

  const audioRepairOptions: Array<{
    mode: AudioRepairMode;
    icon: React.ReactNode;
    title: string;
    subtitle: string;
  }> = [
    {
      mode: 'smart',
      icon: <SearchCheck className="w-3.5 h-3.5" />,
      title: 'Smart',
      subtitle: 'Detect issues, repair needed audio',
    },
    {
      mode: 'fast',
      icon: <Gauge className="w-3.5 h-3.5" />,
      title: 'Fast',
      subtitle: 'Direct stream copy',
    },
    {
      mode: 'safe',
      icon: <ShieldCheck className="w-3.5 h-3.5" />,
      title: 'Safe',
      subtitle: 'Repair audio in every file',
    },
  ];

  return (
    <div>
      <p className="text-xs font-medium text-text-secondary mb-2">Merge Mode</p>
      <div className="grid grid-cols-2 gap-2">
        {([
          {
            mode: 'lossless' as MergeMode,
            icon: <Zap className="w-4 h-4" />,
            title: 'Lossless',
            subtitle: 'Stream copy - zero video quality change',
          },
          {
            mode: 'custom' as MergeMode,
            icon: <Sliders className="w-4 h-4" />,
            title: 'Custom Quality',
            subtitle: 'Re-encode - fix incompatibilities',
          },
          {
            mode: 'smartMkv' as MergeMode,
            icon: <Brain className="w-4 h-4" />,
            title: 'Smart MKV',
            subtitle: 'Analyze + fix only what\'s broken',
          },
          {
            mode: 'fastMkv' as MergeMode,
            icon: <FastForward className="w-4 h-4" />,
            title: 'Fast MKV',
            subtitle: 'Stream copy - maximum speed',
          },
        ] as const).map((opt) => (
          <button
            key={opt.mode}
            type="button"
            onClick={() => setMergeMode(opt.mode)}
            className={cn(
              'flex flex-col gap-2 p-3 rounded-xl border text-left transition-all duration-150',
              mergeMode === opt.mode
                ? 'bg-accent-muted border-accent-500/40 shadow-glow-sm'
                : 'bg-bg-elevated border-border hover:border-border-strong',
            )}
            aria-pressed={mergeMode === opt.mode}
            aria-label={`${opt.title} merge mode`}
          >
            <div className="flex items-center justify-between">
              <span className={cn(mergeMode === opt.mode ? 'text-accent-400' : 'text-text-muted')}>
                {opt.icon}
              </span>
              <div className="flex items-center gap-1.5">
                {report?.recommendedMode === opt.mode && (
                  <Badge variant="info">Recommended</Badge>
                )}
                {/* Show auto-upgrade indicator when selected mode will be upgraded */}
                {mergeMode === opt.mode && report?.recommendedMode && report.recommendedMode !== opt.mode && (
                  <Badge variant="warning" className="text-2xs">
                    → {report.recommendedMode === 'custom' ? 'Custom' : report.recommendedMode === 'smartMkv' ? 'Smart MKV' : report.recommendedMode}
                  </Badge>
                )}
              </div>
            </div>
            <div>
              <p className={cn('text-sm font-semibold', mergeMode === opt.mode ? 'text-text-primary' : 'text-text-secondary')}>
                {opt.title}
              </p>
              <p className="text-2xs text-text-muted mt-0.5">{opt.subtitle}</p>
            </div>
          </button>
        ))}
      </div>

      {mergeMode === 'lossless' && (
        <div className="mt-2 space-y-2">
          {report?.issues.some((i) => i.severity === 'error') ? (
            <>
              <div className="px-3 py-2 rounded-lg bg-danger/5 border border-danger/20 flex items-start gap-2">
                <AlertTriangle className="w-3.5 h-3.5 text-danger shrink-0 mt-0.5" />
                <div className="flex flex-col gap-0.5">
                  <p className="text-xs text-danger font-medium">
                    Lossless will re-encode
                  </p>
                  <p className="text-2xs text-danger/80 leading-tight">
                    {report.issues.find((i) => i.severity === 'error')?.description ??
                      'One or more files are incompatible with stream copy.'}
                  </p>
                </div>
              </div>
            </>
          ) : (
            <div className="px-3 py-2 rounded-lg bg-success/5 border border-success/20 flex items-center gap-2">
              <CheckCircle2 className="w-3.5 h-3.5 text-success shrink-0" />
              <p className="text-xs text-success">
                Video is kept lossless. Choose how aggressively audio is checked or repaired.
              </p>
            </div>
          )}

          <div>
            <p className="text-[10px] font-semibold text-text-muted uppercase tracking-wider mb-1.5">Audio Repair</p>
            <div className="grid grid-cols-3 gap-1.5">
              {audioRepairOptions.map((opt) => (
                <button
                  key={opt.mode}
                  type="button"
                  onClick={() => setAudioRepairMode(opt.mode)}
                  title={
                    opt.mode === 'smart'
                      ? 'Deep validation + seek checks. Only repairs files with detected audio issues. Best balance of speed and safety.'
                      : opt.mode === 'fast'
                      ? 'No detection, no repair. Direct stream copy — fastest option but does not check for audio problems.'
                      : 'Skip detection, repair all files with audio. Most thorough but slowest — re-encodes every audio stream.'
                  }
                  className={cn(
                    'min-h-[74px] rounded-lg border px-2.5 py-2 text-left transition-all',
                    audioRepairMode === opt.mode
                      ? 'border-accent-500/50 bg-accent-500/10'
                      : 'border-border/50 bg-bg-elevated hover:border-border-strong',
                  )}
                  aria-pressed={audioRepairMode === opt.mode}
                  aria-label={`${opt.title} audio repair`}
                >
                  <span className={cn('inline-flex mb-1', audioRepairMode === opt.mode ? 'text-accent-400' : 'text-text-muted')}>
                    {opt.icon}
                  </span>
                  <span className={cn('block text-[11px] font-semibold', audioRepairMode === opt.mode ? 'text-accent-400' : 'text-text-primary')}>
                    {opt.title}
                  </span>
                  <span className="block text-[9px] text-text-muted leading-tight mt-0.5">
                    {opt.subtitle}
                  </span>
                </button>
              ))}
            </div>
          </div>
        </div>
      )}

      {mergeMode === 'custom' && (
        <div className="mt-2 px-3 py-2 rounded-lg bg-warning/5 border border-warning/20 flex items-center gap-2">
          <AlertTriangle className="w-3.5 h-3.5 text-warning shrink-0" />
          <p className="text-xs text-warning">
            Re-encoding will occur. Quality depends on your settings below.
          </p>
        </div>
      )}

      {mergeMode === 'fastMkv' && (
        <div className="mt-2 space-y-2">
          <div className="px-3 py-2 rounded-lg bg-accent/5 border border-accent/20">
            <p className="text-xs text-accent font-medium mb-1">Fast MKV Merge</p>
            <ul className="text-2xs text-text-muted space-y-0.5">
              <li>• No normalization</li>
              <li>• No audio repair</li>
              <li>• No video repair</li>
              <li>• No compatibility fixes</li>
              <li>• Maximum speed</li>
              <li>• Best for already compatible videos</li>
            </ul>
          </div>

          <div className="px-3 py-2 rounded-lg bg-warning/5 border border-warning/20 flex items-start gap-2">
            <AlertTriangle className="w-3.5 h-3.5 text-warning shrink-0 mt-0.5" />
            <p className="text-2xs text-warning leading-tight">
              Fast MKV Merge skips normalization and compatibility repair.
              Use only when all input files have matching codecs and resolutions.
            </p>
          </div>

          <label className="flex items-center gap-2 px-3 py-2 rounded-lg border border-border/50 bg-bg-elevated cursor-pointer">
            <input
              type="checkbox"
              checked={convertToMp4}
              onChange={(e) => setConvertToMp4(e.target.checked)}
              className="rounded"
            />
            <div>
              <p className="text-xs font-medium text-text-primary">Convert to MP4 after merge</p>
              <p className="text-2xs text-text-muted">Incompatible codecs will be re-encoded automatically</p>
            </div>
          </label>
        </div>
      )}

      {mergeMode === 'smartMkv' && (
        <div className="mt-2 space-y-2">
          <div className="px-3 py-2 rounded-lg bg-accent/5 border border-accent/20">
            <p className="text-xs text-accent font-medium mb-1">Smart MKV Merge</p>
            <ul className="text-2xs text-text-muted space-y-0.5">
              <li>• Analyzes all input files</li>
              <li>• Normalizes only files that need it</li>
              <li>• Audio repair for broken streams</li>
              <li>• Video normalization for mismatched profiles</li>
              <li>• Skips healthy files entirely</li>
              <li>• Output = MKV container</li>
            </ul>
          </div>

          <div className="px-3 py-2 rounded-lg bg-success/5 border border-success/20 flex items-start gap-2">
            <CheckCircle2 className="w-3.5 h-3.5 text-success shrink-0 mt-0.5" />
            <p className="text-2xs text-success leading-tight">
              Best balance of speed and safety. Repairs only what is broken,
              keeps healthy files untouched. Faster than Lossless MP4.
            </p>
          </div>

          <label className="flex items-center gap-2 px-3 py-2 rounded-lg border border-border/50 bg-bg-elevated cursor-pointer">
            <input
              type="checkbox"
              checked={convertToMp4}
              onChange={(e) => setConvertToMp4(e.target.checked)}
              className="rounded"
            />
            <div>
              <p className="text-xs font-medium text-text-primary">Convert to MP4 after merge</p>
              <p className="text-2xs text-text-muted">Incompatible codecs will be re-encoded automatically</p>
            </div>
          </label>
        </div>
      )}
    </div>
  );
}
