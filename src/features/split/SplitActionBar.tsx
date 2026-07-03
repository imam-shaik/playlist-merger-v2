import React from 'react';
import { Play, Loader2, Square } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { useSplitStore } from '@/store/splitStore';

export function SplitActionBar() {
  const inputFile = useSplitStore((s) => s.inputFile);
  const currentPlan = useSplitStore((s) => s.currentPlan);
  const isGeneratingPlan = useSplitStore((s) => s.isGeneratingPlan);
  const activeJob = useSplitStore((s) => s.activeJob);
  const generatePlan = useSplitStore((s) => s.generatePlan);
  const executePlan = useSplitStore((s) => s.executePlan);
  const cancelJob = useSplitStore((s) => s.cancelJob);

  const isSplitting = activeJob?.stage === 'splitting';
  const canGeneratePlan = inputFile.length > 0 && !isGeneratingPlan && !isSplitting;
  const canExecute = currentPlan !== null && !isSplitting && currentPlan.segments.length > 0;

  return (
    <div className="flex items-center gap-2">
      <Button
        variant="secondary"
        size="sm"
        onClick={generatePlan}
        disabled={!canGeneratePlan}
      >
        {isGeneratingPlan ? (
          <>
            <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />
            Planning...
          </>
        ) : (
          'Generate Preview'
        )}
      </Button>

      {isSplitting ? (
        <Button
          variant="danger"
          size="sm"
          onClick={() => activeJob && cancelJob(activeJob.id)}
        >
          <Square className="w-3.5 h-3.5 mr-1.5" />
          Cancel
        </Button>
      ) : (
        <Button
          variant="primary"
          size="sm"
          onClick={executePlan}
          disabled={!canExecute}
        >
          <Play className="w-3.5 h-3.5 mr-1.5" />
          Start Split
        </Button>
      )}

      {isSplitting && activeJob && (
        <div className="flex items-center gap-2 ml-2">
          <div className="w-1.5 h-1.5 rounded-full bg-success animate-pulse" />
          <span className="text-xs text-text-muted tabular-nums">
            Segment {activeJob.progress.segmentIndex}/{activeJob.progress.segmentCount}
          </span>
          <span className="text-xs text-text-muted">
            ({activeJob.progress.progress.toFixed(0)}%)
          </span>
        </div>
      )}
    </div>
  );
}
