import React from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { AlertTriangle, Play, RefreshCw } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import type { RecoveryCheckpoint } from '@/types';

interface RecoveryDialogProps {
  checkpoints: RecoveryCheckpoint[];
  onResume: (checkpoint: RecoveryCheckpoint) => void;
  onStartOver: (checkpoint: RecoveryCheckpoint) => void;
}

function formatTimestamp(ts: number): string {
  const d = new Date(ts);
  return d.toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

function CheckpointCard({ checkpoint, onResume, onStartOver }: {
  checkpoint: RecoveryCheckpoint;
  onResume: () => void;
  onStartOver: () => void;
}) {
  const totalFiles = checkpoint.inputFiles.length;
  const completedCount = checkpoint.completedFiles.length;
  const remaining = totalFiles - completedCount;
  const pct = totalFiles > 0
    ? Math.round((completedCount / totalFiles) * 100)
    : 0;

  const outputName = checkpoint.outputPath.split(/[/\\]/).pop() ?? checkpoint.outputPath;

  return (
    <div className="bg-bg-elevated border border-border rounded-lg p-4 space-y-3">
      <div className="flex items-start justify-between gap-3">
        <div className="flex items-center gap-2 text-warning">
          <AlertTriangle className="w-4 h-4 shrink-0 mt-0.5" />
          <span className="text-sm font-medium text-text-primary">Interrupted Merge</span>
        </div>
        <span className="text-2xs text-text-muted">{formatTimestamp(checkpoint.startedAt)}</span>
      </div>

      <div className="space-y-1">
        <div className="text-sm text-text-primary font-medium truncate" title={checkpoint.outputPath}>
          {outputName}
        </div>
        <div className="text-xs text-text-muted">
          {completedCount} of {totalFiles} files normalized
          {' · '}{remaining} remaining
        </div>
        <div className="flex items-center gap-2">
          <div className="flex-1 h-1.5 bg-bg-surface rounded-full overflow-hidden">
            <div
              className="h-full bg-accent-500 rounded-full transition-all"
              style={{ width: `${pct}%` }}
            />
          </div>
          <span className="text-2xs text-text-muted">{pct}%</span>
        </div>
      </div>

      <div className="flex gap-2">
        <Button
          variant="primary"
          size="sm"
          leftIcon={<Play className="w-3 h-3" />}
          onClick={onResume}
          className="flex-1"
        >
          Resume
        </Button>
        <Button
          variant="ghost"
          size="sm"
          leftIcon={<RefreshCw className="w-3 h-3" />}
          onClick={onStartOver}
        >
          Start Over
        </Button>
      </div>
    </div>
  );
}

export function RecoveryDialog({ checkpoints, onResume, onStartOver }: RecoveryDialogProps) {
  if (checkpoints.length === 0) return null;

  return (
    <AnimatePresence>
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
      >
        <motion.div
          initial={{ opacity: 0, scale: 0.95, y: 8 }}
          animate={{ opacity: 1, scale: 1, y: 0 }}
          exit={{ opacity: 0, scale: 0.95, y: 8 }}
          transition={{ duration: 0.2, ease: [0.16, 1, 0.3, 1] }}
          className="bg-bg-surface border border-border shadow-modal rounded-xl w-full max-w-md mx-4 overflow-hidden"
        >
          <div className="flex items-center justify-between px-5 py-4 border-b border-border">
            <div className="flex items-center gap-2">
              <AlertTriangle className="w-4 h-4 text-warning" />
              <h2 className="text-sm font-semibold text-text-primary">Resume Previous Merge?</h2>
            </div>
          </div>

          <div className="px-5 py-4 space-y-3 max-h-96 overflow-y-auto">
            <p className="text-xs text-text-secondary">
              {checkpoints.length === 1
                ? 'A previous merge was interrupted. You can resume where it left off, or start over.'
                : `Found ${checkpoints.length} interrupted merges. Choose which one to resume.`}
            </p>
            <div className="space-y-2">
              {checkpoints.map((cp) => (
                <CheckpointCard
                  key={cp.jobId}
                  checkpoint={cp}
                  onResume={() => onResume(cp)}
                  onStartOver={() => onStartOver(cp)}
                />
              ))}
            </div>
          </div>
        </motion.div>
      </motion.div>
    </AnimatePresence>
  );
}