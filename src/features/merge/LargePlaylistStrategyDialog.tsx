import React from 'react';
import { motion } from 'framer-motion';
import { Shield, Zap, AlertTriangle, Gauge, X } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { useMergeStore } from '@/store/mergeStore';
import type { LargePlaylistStrategy } from '@/types';

interface LargePlaylistStrategyDialogProps {
  fileCount: number;
  onConfirm: (strategy: LargePlaylistStrategy) => void;
  onCancel: () => void;
}

const STRATEGIES: Array<{
  value: LargePlaylistStrategy;
  label: string;
  desc: string;
  icon: React.ReactNode;
  color: string;
}> = [
  {
    value: 'fullSmart',
    label: 'Full Smart',
    desc: 'Thoroughly probe & repair every audio stream. Longest pre-merge time, best audio quality protection.',
    icon: <Shield className="w-4 h-4" />,
    color: 'text-success-400',
  },
  {
    value: 'smartLite',
    label: 'SmartLite',
    desc: 'Targeted probe of files with detected issues. Fast pre-merge, reliable protection for most playlists.',
    icon: <Zap className="w-4 h-4" />,
    color: 'text-accent-400',
  },
  {
    value: 'safe',
    label: 'Safe',
    desc: 'Skip audio probing entirely. Fastest pre-merge. Use when audio quality is already verified.',
    icon: <AlertTriangle className="w-4 h-4" />,
    color: 'text-warning',
  },
  {
    value: 'fast',
    label: 'Fast',
    desc: 'Minimal skip — only basic health checks. Skips all audio stream validation.',
    icon: <Gauge className="w-4 h-4" />,
    color: 'text-text-muted',
  },
];

export function LargePlaylistStrategyDialog({ fileCount, onConfirm, onCancel }: LargePlaylistStrategyDialogProps) {
  const setLargePlaylistStrategy = useMergeStore((s) => s.setLargePlaylistStrategy);
  const largePlaylistStrategy = useMergeStore((s) => s.largePlaylistStrategy);

  const handleConfirm = (strategy: LargePlaylistStrategy) => {
    setLargePlaylistStrategy(strategy);
    onConfirm(strategy);
  };

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/70 backdrop-blur-sm p-4">
      <div className="absolute inset-0" onClick={onCancel} />
      <motion.div
        initial={{ opacity: 0, scale: 0.95, y: 10 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        transition={{ duration: 0.2, ease: [0.16, 1, 0.3, 1] }}
        className="relative z-10 bg-bg-elevated border border-border shadow-modal rounded-xl w-full max-w-md overflow-hidden"
      >
        <div className="flex items-center justify-between px-5 py-4 border-b border-border/80">
          <div className="flex items-center gap-2">
            <AlertTriangle className="w-4 h-4 text-warning" />
            <h2 className="text-sm font-semibold text-text-primary">Large Playlist Detected</h2>
          </div>
          <button
            onClick={onCancel}
            className="p-1 rounded-lg text-text-muted hover:text-text-primary hover:bg-bg-overlay transition-all"
            aria-label="Close"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="p-5">
          <p className="text-sm text-text-secondary mb-1">
            Your playlist contains <span className="font-semibold text-text-primary">{fileCount} files</span>.
          </p>
          <p className="text-xs text-text-muted mb-4">
            Smart mode audio validation can take a long time on large playlists.
            Choose how to handle audio validation:
          </p>

          <div className="flex flex-col gap-2">
            {STRATEGIES.map((strategy) => (
              <button
                key={strategy.value}
                onClick={() => handleConfirm(strategy.value)}
                className={`flex items-start gap-3 p-3 rounded-lg border text-left transition-all hover:border-accent-500/50 hover:bg-accent-500/5 ${
                  largePlaylistStrategy === strategy.value
                    ? 'border-accent-500 bg-accent-500/10'
                    : 'border-border bg-bg-surface/30'
                }`}
              >
                <span className={`mt-0.5 shrink-0 ${strategy.color}`}>{strategy.icon}</span>
                <div>
                  <p className="text-sm font-medium text-text-primary">{strategy.label}</p>
                  <p className="text-xs text-text-muted mt-0.5 leading-relaxed">{strategy.desc}</p>
                </div>
              </button>
            ))}
          </div>
        </div>

        <div className="px-5 py-4 border-t border-border/80 bg-bg-surface/30 flex justify-end">
          <Button variant="ghost" size="sm" onClick={onCancel}>
            Cancel
          </Button>
        </div>
      </motion.div>
    </div>
  );
}