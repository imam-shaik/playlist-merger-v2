import React from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import {
  Loader2, CheckCircle2, XCircle, AlertCircle,
  Square, X, FolderOpen, Clock, HardDrive, FileText
} from 'lucide-react';
import { useSplitStore } from '@/store/splitStore';
import { useAppStore } from '@/store/appStore';
import { formatDuration, formatBytes, getFilename } from '@/utils';
import { cn } from '@/utils/cn';

export function SplitTaskQueue() {
  const jobs = useSplitStore((s) => s.jobs);
  const removeJob = useSplitStore((s) => s.removeJob);
  const cancelJob = useSplitStore((s) => s.cancelJob);

  if (jobs.length === 0) {
    return (
      <div className="flex items-center justify-center h-24 rounded-lg border border-dashed border-border bg-bg-elevated/30">
        <p className="text-xs text-text-muted">No split jobs yet</p>
      </div>
    );
  }

  return (
    <div className="space-y-2.5 max-h-96 overflow-y-auto custom-scrollbar pr-1">
      <AnimatePresence initial={false}>
        {jobs.map((job) => {
          const isActive = job.stage === 'splitting';
          const isDone = job.stage === 'complete';
          const isFailed = job.stage === 'failed';
          const isCancelled = job.stage === 'cancelled';

          const handleRevealFile = async (path: string) => {
            try {
              const { tauriCommands } = await import('@/tauri/commands');
              await tauriCommands.revealInExplorer(path);
            } catch (err) {
              console.error('Failed to reveal file:', err);
            }
          };

          const handleOpenReport = async (path: string) => {
            try {
              const { tauriCommands } = await import('@/tauri/commands');
              await tauriCommands.openWithDefault(path);
            } catch (err) {
              console.error('Failed to open report:', err);
              useAppStore.getState().showToast({ type: 'error', title: 'Error opening report' });
            }
          };

          const reportPath = job.result?.reportPaths?.[0];

          return (
            <motion.div
              key={job.id}
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: 'auto' }}
              exit={{ opacity: 0, height: 0 }}
              className={cn(
                'rounded-xl border p-4 transition-all duration-300 flex flex-col gap-3',
                isActive && 'border-accent-500/30 bg-accent-500/5 shadow-glow-sm',
                isDone && 'border-success/20 bg-success/5',
                isFailed && 'border-danger/20 bg-danger/5',
                isCancelled && 'border-warning/20 bg-warning/5',
                !isActive && !isDone && !isFailed && !isCancelled && 'border-border bg-bg-overlay/40'
              )}
            >
              {/* Header */}
              <div className="flex items-center justify-between gap-3 min-w-0">
                <div className="flex items-center gap-2 min-w-0">
                  {/* Status icon */}
                  {isActive && <Loader2 className="w-3.5 h-3.5 animate-spin text-accent-400 shrink-0" />}
                  {isDone && <CheckCircle2 className="w-3.5 h-3.5 text-success shrink-0" />}
                  {isFailed && <XCircle className="w-3.5 h-3.5 text-danger shrink-0" />}
                  {isCancelled && <AlertCircle className="w-3.5 h-3.5 text-warning shrink-0" />}

                  <span className="text-xs font-semibold text-text-primary truncate" title={job.plan.inputFile}>
                    {getFilename(job.plan.inputFile)}
                  </span>
                </div>

                <div className="flex items-center gap-1.5 shrink-0">
                  {/* Report Button */}
                  {isDone && reportPath && (
                    <button
                      onClick={() => handleOpenReport(reportPath)}
                      className="p-1 rounded-lg text-success/80 hover:text-success hover:bg-success/10 transition-colors"
                      title="Open Split Report"
                    >
                      <FileText className="w-3.5 h-3.5" />
                    </button>
                  )}

                  <span className={cn(
                    'text-[9px] uppercase tracking-wider font-bold px-2 py-0.5 rounded-full border',
                    isActive && 'bg-accent-500/10 border-accent-500/20 text-accent-400',
                    isDone && 'bg-success/10 border-success/20 text-success',
                    isFailed && 'bg-danger/10 border-danger/20 text-danger',
                    isCancelled && 'bg-warning/10 border-warning/20 text-warning',
                  )}>
                    {job.stage === 'splitting' ? 'splitting' : job.stage}
                  </span>

                  {!isActive && (
                    <button
                      onClick={() => removeJob(job.id)}
                      className="p-1 rounded-lg text-text-muted hover:text-text-primary hover:bg-bg-elevated transition-colors"
                      title="Clear Job"
                      aria-label="Remove job from queue"
                    >
                      <X className="w-3.5 h-3.5" />
                    </button>
                  )}

                  {isActive && (
                    <button
                      onClick={() => cancelJob(job.id)}
                      className="p-1 rounded-lg text-text-muted hover:text-danger hover:bg-danger/10 transition-colors"
                      title="Cancel Split"
                      aria-label="Cancel split job"
                    >
                      <Square className="w-3.5 h-3.5" />
                    </button>
                  )}
                </div>
              </div>

              {/* Progress bar (only for active jobs) */}
              {isActive && (
                <div className="space-y-1.5">
                  <div className="h-2 rounded-full bg-bg-elevated overflow-hidden border border-border/10 shadow-inner relative">
                    <motion.div
                      className="h-full rounded-full bg-gradient-to-r from-accent-500 to-indigo-400 shadow-glow-sm"
                      initial={{ width: 0 }}
                      animate={{ width: `${job.progress.progress}%` }}
                      transition={{ duration: 0.3, ease: 'easeOut' }}
                    />
                  </div>
                  <p className="text-[10px] text-text-secondary leading-normal font-medium flex items-center gap-1.5">
                    <span className="inline-flex w-1.5 h-1.5 rounded-full bg-accent-500 animate-pulse" />
                    {job.progress.message}
                  </p>
                </div>
              )}

              {/* Details Tag list */}
              <div className="flex flex-wrap items-center gap-2 text-[10px] text-text-muted font-medium border-t border-border/30 pt-2.5">
                <span className="flex items-center gap-1 bg-bg-elevated/30 border border-border/40 px-2 py-0.5 rounded-lg">
                  <Clock className="w-3 h-3 text-text-muted" />
                  {formatDuration(job.plan.inputDuration)}
                </span>
                <span className="bg-bg-elevated/30 border border-border/40 px-2 py-0.5 rounded-lg">
                  {job.plan.segments.length} segments
                </span>
                {job.result && (
                  <span className="flex items-center gap-1 bg-bg-elevated/30 border border-border/40 px-2 py-0.5 rounded-lg">
                    <HardDrive className="w-3 h-3 text-text-muted" />
                    {formatBytes(job.result.outputSizesBytes.reduce((a, b) => a + b, 0))}
                  </span>
                )}
              </div>

              {/* Error message */}
              {job.error && (
                <p className="text-2xs text-danger/80 border border-danger/10 bg-danger/5 rounded px-2.5 py-1.5 leading-normal max-h-16 overflow-y-auto">
                  {job.error}
                </p>
              )}

              {/* Output paths for completed jobs */}
              {job.result && isDone && (
                <div className="mt-1 border-t border-border/30 pt-3 space-y-1.5">
                  <p className="text-[10px] font-semibold text-text-muted uppercase tracking-wider">Generated Output Files</p>
                  <div className="space-y-1.5 max-h-36 overflow-y-auto custom-scrollbar pr-0.5">
                    {job.result.outputPaths.map((path, idx) => (
                      <div
                        key={idx}
                        onClick={() => handleRevealFile(path)}
                        className="flex items-center justify-between gap-2.5 text-2xs text-text-secondary truncate hover:text-accent-400 hover:bg-bg-elevated/50 px-2.5 py-1.5 rounded-lg border border-border/30 cursor-pointer transition-all hover:border-accent-500/20 group"
                        title="Click to reveal file in folder"
                      >
                        <div className="flex items-center gap-2 min-w-0">
                          <FolderOpen className="w-3.5 h-3.5 shrink-0 text-text-muted group-hover:text-accent-400" />
                          <span className="truncate font-medium">{getFilename(path)}</span>
                        </div>
                        <span className="shrink-0 text-text-muted font-mono">
                          {formatBytes(job.result!.outputSizesBytes[idx] ?? 0)}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </motion.div>
          );
        })}
      </AnimatePresence>
    </div>
  );
}
