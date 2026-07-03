// ────────────────────────────────────────────────
// MergePanel — Orchestrator for merge configuration
// Delegates UI to focused subcomponents.
// ────────────────────────────────────────────────

import React from 'react';

// Subcomponents
import { MergeTaskQueue } from './MergeTaskQueue';

export function MergePanel() {
  return (
    <div className="flex flex-col h-full relative">
      <div className="flex-1 overflow-y-auto p-4 flex flex-col gap-4">
        {/* Merge Tasks Queue */}
        <MergeTaskQueue />
      </div>
    </div>
  );
}
