import React from 'react';
import { motion } from 'framer-motion';
import { Loader2, AlertCircle, CheckCircle2, Clock, HardDrive } from 'lucide-react';
import { useSplitStore } from '@/store/splitStore';
import { formatDuration, formatBytes } from '@/utils';
import { cn } from '@/utils/cn';

export function SplitPreview() {
  const currentPlan = useSplitStore((s) => s.currentPlan);
  const isGeneratingPlan = useSplitStore((s) => s.isGeneratingPlan);
  const planError = useSplitStore((s) => s.planError);
  const generatePlan = useSplitStore((s) => s.generatePlan);
  const inputFile = useSplitStore((s) => s.inputFile);

  if (!inputFile) {
    return (
      <div className="flex items-center justify-center h-32 rounded-lg border border-dashed border-border bg-bg-elevated/30">
        <p className="text-xs text-text-muted">Select an input file to generate a split preview</p>
      </div>
    );
  }

  if (isGeneratingPlan) {
    return (
      <div className="flex items-center justify-center gap-2 h-32 rounded-lg border border-border bg-bg-elevated/30">
        <Loader2 className="w-4 h-4 animate-spin text-accent-400" />
        <span className="text-xs text-text-muted">Generating split plan...</span>
      </div>
    );
  }

  if (planError) {
    return (
      <div className="flex flex-col items-center justify-center gap-2 h-32 rounded-lg border border-border bg-warning/5">
        <AlertCircle className="w-4 h-4 text-warning" />
        <p className="text-xs text-text-muted text-center max-w-md">{planError}</p>
        <button
          onClick={generatePlan}
          className="text-xs text-accent-400 hover:text-accent-300 underline"
        >
          Try again
        </button>
      </div>
    );
  }

  if (!currentPlan) {
    return (
      <div className="flex items-center justify-center h-32 rounded-lg border border-dashed border-border bg-bg-elevated/30">
        <button
          onClick={generatePlan}
          className="px-3 py-1.5 rounded-md text-xs font-medium bg-accent-500/20 text-accent-400 border border-accent-500/30 hover:bg-accent-500/30 transition-colors"
        >
          Generate Preview
        </button>
      </div>
    );
  }

  const segments = currentPlan.segments;
  const totalDuration = currentPlan.inputDuration;

  return (
    <div className="space-y-4">
      {/* Summary stats */}
      <div className="flex items-center gap-4 px-3 py-2 rounded-xl bg-bg-elevated/40 border border-border">
        <div className="flex items-center gap-1.5">
          <CheckCircle2 className="w-3.5 h-3.5 text-success" />
          <span className="text-xs text-text-secondary font-medium">{segments.length} segments</span>
        </div>
        <div className="flex items-center gap-1.5">
          <Clock className="w-3.5 h-3.5 text-text-muted" />
          <span className="text-xs text-text-muted">Total Duration: {formatDuration(totalDuration)}</span>
        </div>
        {currentPlan.inputSizeBytes > 0 && (
          <div className="flex items-center gap-1.5">
            <HardDrive className="w-3.5 h-3.5 text-text-muted" />
            <span className="text-xs text-text-muted">Source Size: {formatBytes(currentPlan.inputSizeBytes)}</span>
          </div>
        )}
      </div>

      {/* Visual timeline bar */}
      <div className="relative h-10 rounded-xl overflow-hidden bg-bg-elevated border border-border shadow-inner">
        {segments.map((seg) => {
          const leftPct = (seg.startTime / totalDuration) * 100;
          const widthPct = (seg.duration / totalDuration) * 100;
          const hue = (seg.index * 137.5) % 360;
          return (
            <div
              key={seg.index}
              className="absolute top-0 h-full flex items-center justify-center transition-colors group cursor-help"
              style={{
                left: `${leftPct}%`,
                width: `${Math.max(widthPct, 1.5)}%`,
                backgroundColor: `hsla(${hue}, 60%, 50%, 0.25)`,
                borderRight: '1px solid rgba(255,255,255,0.08)',
              }}
              title={`${seg.label}: ${formatDuration(seg.startTime)} → ${formatDuration(seg.endTime)} (${formatDuration(seg.duration)})`}
            >
              {widthPct > 8 && (
                <span className="text-[10px] font-semibold text-white/90 truncate px-1.5 select-none drop-shadow-sm">
                  {seg.label}
                </span>
              )}
            </div>
          );
        })}
      </div>

      {/* Segment list */}
      <div className="space-y-1.5 max-h-64 overflow-y-auto custom-scrollbar pr-1">
        {segments.map((seg, idx) => {
          const hue = (seg.index * 137.5) % 360;
          return (
            <motion.div
              key={seg.index}
              initial={{ opacity: 0, y: 4 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: idx * 0.02 }}
              className={cn(
                'flex items-center gap-3 px-3.5 py-2.5 rounded-xl border border-transparent transition-all duration-200',
                'bg-bg-base/30 hover:bg-bg-base/50 hover:border-border/60 hover:shadow-glow-sm group'
              )}
            >
              {/* Colored index badge */}
              <span
                className="w-5 h-5 rounded-full flex items-center justify-center text-[9px] font-bold font-mono text-white shrink-0 shadow-sm select-none"
                style={{ backgroundColor: `hsl(${hue}, 55%, 45%)` }}
              >
                {seg.index}
              </span>

              {/* Label */}
              <span className="text-xs font-semibold text-text-primary truncate min-w-0 flex-1 max-w-[200px]" title={seg.label}>
                {seg.label}
              </span>

              {/* Time range pill */}
              <div className="flex items-center gap-1.5 text-2xs text-text-muted font-mono font-medium bg-bg-overlay/40 px-2 py-0.5 rounded border border-border/10 shrink-0">
                <span>{formatDuration(seg.startTime)}</span>
                <span className="opacity-40">→</span>
                <span>{formatDuration(seg.endTime)}</span>
              </div>

              {/* Duration and size metrics */}
              <div className="flex items-center gap-3 shrink-0 ml-auto font-mono">
                <span className="text-2xs font-semibold text-text-secondary bg-bg-elevated/40 px-2 py-0.5 rounded border border-border/10">
                  {formatDuration(seg.duration)}
                </span>
                {seg.estimatedSizeBytes && (
                  <span className="text-[10px] text-text-muted hidden sm:inline-block w-14 text-right">
                    {formatBytes(seg.estimatedSizeBytes)}
                  </span>
                )}
              </div>
            </motion.div>
          );
        })}
      </div>
    </div>
  );
}
