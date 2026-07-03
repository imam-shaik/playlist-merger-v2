// ────────────────────────────────────────────────
// SmartMkvDashboard — Shows the Smart MKV analysis
// breakdown: which properties get normalized, remuxed,
// or skipped compared to standard Smart mode.
// ────────────────────────────────────────────────

import React from 'react';
import { cn } from '@/utils/cn';

export interface SmartMkvBreakdown {
  willNormalize: number;
  willRemux: number;
  willSkip: number;
  totalFiles: number;
  categories: {
    normalize: { property: string; count: number }[];
    remux: { property: string; count: number }[];
    skip: { property: string; count: number }[];
  };
}

interface SmartMkvDashboardProps {
  breakdown: SmartMkvBreakdown;
  className?: string;
}

function CategoryBadge({ count, label, color }: { count: number; label: string; color: string }) {
  return (
    <div className="flex items-center gap-1.5 px-2 py-1 rounded-full bg-bg-base border border-border/40 text-[10px]">
      <span className={cn('w-2 h-2 rounded-full', color)} />
      <span className="font-semibold text-text-primary">{count}</span>
      <span className="text-text-muted">{label}</span>
    </div>
  );
}

export function SmartMkvDashboard({ breakdown, className }: SmartMkvDashboardProps) {
  const totalNeedingWork = breakdown.willNormalize + breakdown.willRemux;
  const savingsPct = breakdown.totalFiles > 0
    ? Math.round((breakdown.willSkip / (totalNeedingWork + breakdown.willSkip)) * 100)
    : 0;

  return (
    <div className={cn('rounded-lg border border-border/30 bg-bg-base/40 overflow-hidden', className)}>
      {/* Header */}
      <div className="px-3 py-2 bg-bg-overlay/40 border-b border-border/10">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <span className="text-[10px] font-semibold text-text-primary uppercase tracking-wider">
              Smart MKV Analysis
            </span>
            {breakdown.totalFiles > 0 && (
              <span className="text-[9px] text-text-muted">
                {breakdown.totalFiles} files
              </span>
            )}
          </div>
          {savingsPct >= 50 && (
            <span className="text-[9px] text-success font-semibold">
              🚀 {savingsPct}% fewer normalizations
            </span>
          )}
        </div>
      </div>

      {/* Summary badges */}
      <div className="px-3 py-2 flex flex-wrap items-center gap-1.5 border-b border-border/10">
        <CategoryBadge count={breakdown.willNormalize} label="will normalize" color="bg-amber-500" />
        {breakdown.willRemux > 0 && (
          <CategoryBadge count={breakdown.willRemux} label="will remux" color="bg-blue-500" />
        )}
        <CategoryBadge count={breakdown.willSkip} label="skipped (MKV-safe)" color="bg-green-500" />
      </div>

      {/* Category breakdown */}
      <div className="px-3 py-2 space-y-2">
        {/* Will Normalize */}
        {breakdown.categories.normalize.length > 0 && (
          <div>
            <p className="text-[9px] font-semibold text-amber-400 uppercase tracking-wider mb-1">
              🔧 Will Normalize ({breakdown.willNormalize})
            </p>
            <div className="flex flex-wrap gap-1">
              {breakdown.categories.normalize.map((c) => (
                <span
                  key={c.property}
                  className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[9px] bg-amber-500/10 text-amber-300 border border-amber-500/20"
                >
                  {c.property}
                  <span className="font-semibold">{c.count}</span>
                </span>
              ))}
            </div>
          </div>
        )}

        {/* Will Remux */}
        {breakdown.categories.remux.length > 0 && (
          <div>
            <p className="text-[9px] font-semibold text-blue-400 uppercase tracking-wider mb-1">
              ⚡ Will Remux ({breakdown.willRemux})
            </p>
            <div className="flex flex-wrap gap-1">
              {breakdown.categories.remux.map((c) => (
                <span
                  key={c.property}
                  className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[9px] bg-blue-500/10 text-blue-300 border border-blue-500/20"
                >
                  {c.property}
                  <span className="font-semibold">{c.count}</span>
                </span>
              ))}
            </div>
          </div>
        )}

        {/* Will Skip (MKV-safe) */}
        {breakdown.categories.skip.length > 0 && (
          <div>
            <p className="text-[9px] font-semibold text-green-400 uppercase tracking-wider mb-1">
              ✅ Skipped — MKV-safe ({breakdown.willSkip})
            </p>
            <div className="flex flex-wrap gap-1">
              {breakdown.categories.skip.map((c) => (
                <span
                  key={c.property}
                  className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[9px] bg-green-500/10 text-green-300 border border-green-500/20"
                >
                  {c.property}
                  <span className="font-semibold">{c.count}</span>
                </span>
              ))}
            </div>
          </div>
        )}

        {/* Empty state */}
        {breakdown.willNormalize === 0 && breakdown.willRemux === 0 && breakdown.willSkip === 0 && (
          <p className="text-[9px] text-text-muted text-center py-1">
            No analysis data available yet
          </p>
        )}
      </div>
    </div>
  );
}
