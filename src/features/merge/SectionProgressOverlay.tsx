import React from 'react';
import { motion } from 'framer-motion';
import { CheckCircle, XCircle, Loader2, Circle, AlertTriangle } from 'lucide-react';
import { cn } from '@/utils/cn';
import { Button } from '@/components/ui/Button';
import { useSectionStore } from '@/store/sectionStore';


interface SectionProgressOverlayProps {
  onRetrySection?: (sectionIndex: number) => void;
  onSkipSection?: (sectionIndex: number) => void;
  onCancel: () => void;
}

export function SectionProgressOverlay({
  onRetrySection,
  onSkipSection,
  onCancel,
}: SectionProgressOverlayProps) {
  const {
    isExecuting,
    currentSectionIndex,
    totalSections,
    sectionPlans,
    sectionResults,
  } = useSectionStore();

  if (!isExecuting && sectionResults.length === 0) {
    return null;
  }

  const getSectionState = (index: number): 'complete' | 'processing' | 'pending' | 'error' => {
    if (index < currentSectionIndex) return 'complete';
    if (index === currentSectionIndex) return 'processing';
    return 'pending';
  };

  const failedSection = sectionResults.find((r) => !r.success);
  const completedCount = sectionResults.filter((r) => r.success).length;

  const formatSize = (bytes: number) => {
    const gb = bytes / 1e9;
    if (gb >= 1) return `${gb.toFixed(1)} GB`;
    const mb = bytes / 1e6;
    return `${mb.toFixed(0)} MB`;
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <motion.div
        initial={{ opacity: 0, scale: 0.95 }}
        animate={{ opacity: 1, scale: 1 }}
        className="bg-white dark:bg-zinc-900 rounded-xl shadow-2xl w-full max-w-lg overflow-hidden"
      >
        {/* Header */}
        <div className="px-6 py-4 border-b border-zinc-200 dark:border-zinc-700">
          <h2 className="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
            Section Merge Progress
          </h2>
          <p className="text-sm text-zinc-500">
            {completedCount} of {totalSections} sections complete
          </p>
        </div>

        {/* Section List */}
        <div className="p-6 space-y-3 max-h-80 overflow-y-auto">
          {sectionPlans.slice(0, Math.max(sectionResults.length + 2, totalSections)).map((plan, idx) => {
            const state = getSectionState(idx);
            const result = sectionResults[idx];
            
            return (
              <div
                key={idx}
                className={cn(
                  'flex items-center gap-3 p-3 rounded-lg border',
                  state === 'complete' && 'border-green-200 dark:border-green-900 bg-green-50 dark:bg-green-900/20',
                  state === 'processing' && 'border-blue-200 dark:border-blue-900 bg-blue-50 dark:bg-blue-900/20',
                  state === 'pending' && 'border-zinc-200 dark:border-zinc-700',
                  state === 'error' && 'border-red-200 dark:border-red-900 bg-red-50 dark:bg-red-900/20'
                )}
              >
                {/* Status Icon */}
                <div className="flex-shrink-0">
                  {state === 'complete' && (
                    <CheckCircle className="w-5 h-5 text-green-600 dark:text-green-400" />
                  )}
                  {state === 'processing' && (
                    <Loader2 className="w-5 h-5 text-blue-600 dark:text-blue-400 animate-spin" />
                  )}
                  {state === 'pending' && (
                    <Circle className="w-5 h-5 text-zinc-300 dark:text-zinc-600" />
                  )}
                  {state === 'error' && (
                    <XCircle className="w-5 h-5 text-red-600 dark:text-red-400" />
                  )}
                </div>

                {/* Section Info */}
                <div className="flex-1 min-w-0">
                  <div className="font-medium text-sm text-zinc-900 dark:text-zinc-100">
                    {plan.outputName}
                  </div>
                  <div className="text-xs text-zinc-500">
                    {plan.boundary.folderNames.slice(0, 2).join(', ')}
                    {plan.boundary.folderNames.length > 2 && ` +${plan.boundary.folderNames.length - 2}`}
                    {' • '}
                    {plan.boundary.videoCount} videos
                    {result && ` • ${formatSize(result.sizeBytes)}`}
                  </div>
                </div>

                {/* Section Status */}
                <div className="flex-shrink-0">
                  {state === 'complete' && result && (
                    <span className="text-xs text-green-600 dark:text-green-400">Done</span>
                  )}
                  {state === 'processing' && (
                    <span className="text-xs text-blue-600 dark:text-blue-400">Processing...</span>
                  )}
                  {state === 'pending' && (
                    <span className="text-xs text-zinc-400">Pending</span>
                  )}
                  {state === 'error' && result?.errorMessage && (
                    <span className="text-xs text-red-600 dark:text-red-400 truncate max-w-32">
                      {result.errorMessage}
                    </span>
                  )}
                </div>
              </div>
            );
          })}
        </div>

        {/* Failed Section Actions */}
        {failedSection && (
          <div className="mx-6 mb-4 p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg">
            <div className="flex items-center gap-2 text-red-600 dark:text-red-400 mb-2">
              <AlertTriangle className="w-4 h-4" />
              <span className="text-sm font-medium">Section Failed</span>
            </div>
            <p className="text-xs text-red-600/80 dark:text-red-400/80 mb-3">
              {failedSection.errorMessage || 'An error occurred during section merge'}
            </p>
            <div className="flex gap-2">
              {onRetrySection && (
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => onRetrySection(failedSection.sectionIndex)}
                >
                  Retry Section {failedSection.sectionIndex + 1}
                </Button>
              )}
              {onSkipSection && (
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => onSkipSection(failedSection.sectionIndex)}
                >
                  Skip & Continue
                </Button>
              )}
            </div>
          </div>
        )}

        {/* Footer */}
        <div className="flex items-center justify-end gap-3 px-6 py-4 border-t border-zinc-200 dark:border-zinc-700 bg-zinc-50 dark:bg-zinc-800">
          <Button variant="outline" onClick={onCancel}>
            Cancel All
          </Button>
        </div>
      </motion.div>
    </div>
  );
}