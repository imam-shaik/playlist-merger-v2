import React from 'react';
import { cn } from '@/utils/cn';
import { useSplitStore } from '@/store/splitStore';
import { NAMING_MODES } from '@/constants';
import type { NamingModeId } from '@/types';

export function NamingModeSelector() {
  const namingConfig = useSplitStore((s) => s.namingConfig);
  const setNamingConfig = useSplitStore((s) => s.setNamingConfig);
  const mode = namingConfig.mode;

  const activeMode = NAMING_MODES.find((m) => m.id === mode);

  return (
    <div className="space-y-3">
      <label className="block text-xs font-medium text-text-secondary">
        Output Naming
      </label>

      {/* Mode pills */}
      <div className="flex flex-wrap gap-1.5">
        {NAMING_MODES.map((m) => {
          const isActive = m.id === mode;
          return (
            <button
              key={m.id}
              onClick={() => {
                setNamingConfig({
                  ...namingConfig,
                  mode: m.id as NamingModeId,
                  template: m.defaultTemplate,
                });
              }}
              className={cn(
                'px-3 py-1.5 rounded-lg text-[11px] font-medium transition-all duration-150',
                'border focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-500/50',
                isActive
                  ? 'bg-accent-500/10 border-accent-500/30 text-accent-300'
                  : 'bg-bg-elevated/20 border-border/80 text-text-muted hover:border-border-strong hover:text-text-secondary'
              )}
            >
              {m.label}
            </button>
          );
        })}
      </div>

      {/* Active mode description */}
      {activeMode && (
        <p className="text-[10px] text-text-muted leading-relaxed">
          {activeMode.description}
        </p>
      )}

      {/* Prefix/Suffix inputs for relevant modes */}
      {mode === 'prefix' && (
        <div className="flex items-center gap-2">
          <label className="text-[10px] text-text-muted shrink-0">Prefix</label>
          <input
            type="text"
            value={namingConfig.prefix ?? ''}
            onChange={(e) =>
              setNamingConfig({ ...namingConfig, prefix: e.target.value })
            }
            placeholder="Day_"
            className="flex-1 min-w-0 px-2.5 py-1.5 rounded-lg bg-bg-elevated/30 border border-border/80 text-xs text-text-primary placeholder:text-text-muted focus:outline-none focus:border-accent-500/50 focus:ring-1 focus:ring-accent-500/30"
          />
        </div>
      )}

      {mode === 'suffix' && (
        <div className="flex items-center gap-2">
          <label className="text-[10px] text-text-muted shrink-0">Suffix</label>
          <input
            type="text"
            value={namingConfig.suffix ?? ''}
            onChange={(e) =>
              setNamingConfig({ ...namingConfig, suffix: e.target.value })
            }
            placeholder="Completed"
            className="flex-1 min-w-0 px-2.5 py-1.5 rounded-lg bg-bg-elevated/30 border border-border/80 text-xs text-text-primary placeholder:text-text-muted focus:outline-none focus:border-accent-500/50 focus:ring-1 focus:ring-accent-500/30"
          />
        </div>
      )}
    </div>
  );
}