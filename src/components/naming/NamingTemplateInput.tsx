import React, { useState, useEffect, useCallback } from 'react';
import { cn } from '@/utils/cn';
import { useSplitStore } from '@/store/splitStore';
import { tauriCommands } from '@/tauri/commands';
import type { NamingValidation } from '@/types';
import { NamingCheatSheet } from './NamingCheatSheet';

export function NamingTemplateInput() {
  const namingConfig = useSplitStore((s) => s.namingConfig);
  const setNamingConfig = useSplitStore((s) => s.setNamingConfig);
  const inputFile = useSplitStore((s) => s.inputFile);
  const currentPlan = useSplitStore((s) => s.currentPlan);
  const [validation, setValidation] = useState<NamingValidation | null>(null);
  const [showCheatSheet, setShowCheatSheet] = useState(false);

  const template = namingConfig.template;

  const updateValidation = useCallback(async () => {
    if (!template.trim()) {
      setValidation(null);
      return;
    }

    const stem = inputFile ? inputFile.replace(/^.*[/\\]/, '').replace(/\.[^.]+$/, '') : 'Video';
    const now = new Date();

    const segments = currentPlan?.segments.slice(0, 10).map((seg, i) => ({
      filename: stem,
      extension: currentPlan.outputFormat || 'mp4',
      index: i + 1,
      startTime: seg.startTime,
      endTime: seg.endTime,
      duration: seg.duration,
      chapter: seg.label,
      resolution: undefined,
      width: undefined,
      height: undefined,
      folder: undefined,
      originalNum: undefined,
      playlist: undefined,
      playlistIndex: undefined,
      videoCount: undefined,
      totalDuration: undefined,
      date: now.toISOString().split('T')[0],
      time: `${String(now.getHours()).padStart(2, '0')}-${String(now.getMinutes()).padStart(2, '0')}-${String(now.getSeconds()).padStart(2, '0')}`,
      prefix: namingConfig.prefix,
      suffix: namingConfig.suffix,
      lang: undefined,
      langName: undefined,
      partLabel: 'Part',
    })) ?? [];

    try {
      const result = await tauriCommands.validateNamingTemplate(
        template,
        namingConfig,
        segments,
        currentPlan?.segments.length
      );
      setValidation(result);
    } catch {
      setValidation(null);
    }
  }, [template, namingConfig, inputFile, currentPlan]);

  useEffect(() => {
    const timer = setTimeout(updateValidation, 150);
    return () => clearTimeout(timer);
  }, [updateValidation]);

  const batchPreview = validation?.batchPreview ?? [];
  const totalCount = validation?.totalCount ?? 0;
  const errors = validation?.errors ?? [];
  const warnings = validation?.warnings ?? [];
  const hasIssues = errors.length > 0 || warnings.length > 0;

  return (
    <div className="space-y-3">
      {/* Template input */}
      <div>
        <div className="flex items-center justify-between mb-1.5">
          <label className="text-[10px] font-medium text-text-muted">Template</label>
          <button
            onClick={() => setShowCheatSheet((v) => !v)}
            className="text-[10px] text-accent-400 hover:text-accent-300 transition-colors"
          >
            {showCheatSheet ? 'Hide variables' : 'Show variables'}
          </button>
        </div>
        <input
          type="text"
          value={template}
          onChange={(e) =>
            setNamingConfig({ ...namingConfig, template: e.target.value })
          }
          placeholder="{filename}_Part_{num3}"
          className={cn(
            'w-full px-3 py-2 rounded-lg bg-bg-elevated/30 border text-xs font-mono',
            'placeholder:text-text-muted/50 focus:outline-none focus:ring-1',
            hasIssues && errors.length > 0
              ? 'border-red-500/40 focus:border-red-500/50 focus:ring-red-500/30'
              : hasIssues
              ? 'border-yellow-500/40 focus:border-yellow-500/50 focus:ring-yellow-500/30'
              : 'border-border/80 focus:border-accent-500/50 focus:ring-accent-500/30'
          )}
        />
      </div>

      {/* Cheat sheet */}
      {showCheatSheet && (
        <NamingCheatSheet
          onInsert={(variable) => {
            setNamingConfig({
              ...namingConfig,
              template: namingConfig.template + variable,
            });
          }}
        />
      )}

      {/* Validation errors */}
      {errors.length > 0 && (
        <div className="space-y-1">
          {errors.map((err, i) => (
            <div key={i} className="flex items-start gap-1.5 text-[10px] text-red-400">
              <span className="mt-0.5 shrink-0">✕</span>
              <span>{err.message}</span>
            </div>
          ))}
        </div>
      )}

      {/* Validation warnings */}
      {warnings.map((warn, i) => (
        <div key={i} className="flex items-start gap-1.5 text-[10px] text-yellow-400">
          <span className="mt-0.5 shrink-0">⚠</span>
          <span>{warn.message}</span>
        </div>
      ))}

      {/* Batch preview */}
      {batchPreview.length > 0 && (
        <div>
          <p className="text-[10px] font-medium text-text-muted mb-1.5">Preview</p>
          <div className="space-y-0.5 font-mono text-[10px] text-text-secondary bg-bg-elevated/20 rounded-lg p-2 border border-border/60">
            {batchPreview.map((name, i) => (
              <div key={i} className="truncate">
                <span className="text-text-muted">{String(i + 1).padStart(2, ' ')}.</span>{' '}
                <span className="text-text-primary">{name}</span>
              </div>
            ))}
            {totalCount > batchPreview.length && (
              <div className="text-text-muted pt-0.5 border-t border-border/40 mt-1">
                ... ({totalCount - batchPreview.length} more)
              </div>
            )}
          </div>
        </div>
      )}

      {/* Single preview when no plan yet */}
      {!currentPlan && !hasIssues && template.trim() && (
        <p className="text-[10px] text-text-muted italic">
          Generate preview to see output filenames
        </p>
      )}
    </div>
  );
}