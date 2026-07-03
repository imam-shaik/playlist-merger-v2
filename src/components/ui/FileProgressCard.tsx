import React from 'react';
import { cn } from '@/utils/cn';
import { CheckCircle2, XCircle, Loader2 } from 'lucide-react';
import type { MergeFileProgress } from '@/types';
import { formatDuration } from '@/utils';

interface FileProgressCardProps {
  progress: MergeFileProgress;
  className?: string;
}

export function FileProgressCard({ progress, className }: FileProgressCardProps) {
  const { fileIndex, totalFiles, filename, duration, seekPoints, maxGap, result } = progress;

  const isRunning = result === undefined || result === 'running';
  const isPass = result === 'pass';
  const isFail = result === 'fail';

  return (
    <div className={cn('rounded-lg border bg-bg-elevated p-3 font-mono text-[10px]', className)}>
      {/* Header */}
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-1.5">
          {isRunning && <Loader2 className="w-3 h-3 text-accent-400 animate-spin shrink-0" />}
          {isPass && <CheckCircle2 className="w-3 h-3 text-success shrink-0" />}
          {isFail && <XCircle className="w-3 h-3 text-danger shrink-0" />}
          <span className="text-[10px] font-semibold text-text-secondary uppercase tracking-wider">
            Audio Validation
          </span>
        </div>
        <span className="text-[9px] text-text-muted">
          {String(fileIndex + 1).padStart(3, '0')}/{String(totalFiles).padStart(3, '0')}
        </span>
      </div>

      {/* Filename */}
      <p className="text-xs font-semibold text-text-primary truncate mb-2" title={filename}>
        {filename}
      </p>

      {/* Stats grid */}
      <div className="grid grid-cols-2 gap-x-4 gap-y-1 mb-2">
        <div className="flex justify-between">
          <span className="text-text-muted">Duration</span>
          <span className="text-text-secondary">{duration > 0 ? formatDuration(duration) : '—'}</span>
        </div>
        <div className="flex justify-between">
          <span className="text-text-muted">Seek Points</span>
          <span className="text-text-secondary">{seekPoints > 0 ? seekPoints : '—'}</span>
        </div>
        <div className="flex justify-between">
          <span className="text-text-muted">Max Gap</span>
          <span className="text-text-secondary">{maxGap > 0 ? `${maxGap.toFixed(0)}s` : '—'}</span>
        </div>
        <div className="flex justify-between">
          <span className="text-text-muted">Result</span>
          <span className={cn(
            'font-semibold',
            isRunning && 'text-accent-400',
            isPass && 'text-success',
            isFail && 'text-danger',
          )}>
            {isRunning ? 'RUNNING' : isPass ? 'PASS' : isFail ? 'FAIL' : '—'}
          </span>
        </div>
      </div>

      {/* Result bar */}
      <div className="mt-1 h-1 bg-bg-base rounded-full overflow-hidden">
        <div
          className={cn(
            'h-full rounded-full transition-all duration-300',
            isPass && 'bg-success',
            isFail && 'bg-danger',
            isRunning && 'bg-accent-500 animate-pulse',
          )}
          style={{ width: isRunning ? '60%' : '100%' }}
        />
      </div>
    </div>
  );
}