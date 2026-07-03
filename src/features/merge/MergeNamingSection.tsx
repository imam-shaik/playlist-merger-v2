import React, { useState, useEffect, useCallback } from 'react';
import { ChevronDown, ChevronUp, Type } from 'lucide-react';
import { cn } from '@/utils/cn';
import { useMergeStore } from '@/store/mergeStore';
import { usePlaylistStore } from '@/store/playlistStore';
import { NAMING_MODES } from '@/constants';
import { tauriCommands } from '@/tauri/commands';
import type { NamingModeId, NamingValidation } from '@/types';

export function MergeNamingSection() {
  const namingConfig = useMergeStore((s) => s.namingConfig);
  const setNamingConfig = useMergeStore((s) => s.setNamingConfig);
  const outputFilename = useMergeStore((s) => s.outputFilename);
  const entries = usePlaylistStore((s) => s.entries);

  const videoCount = entries.length;
  const totalDurationSecs = entries.reduce((sum, e) => sum + (e.mediaInfo?.duration ?? 0), 0);

  const [expanded, setExpanded] = useState(false);
  const [validation, setValidation] = useState<NamingValidation | null>(null);

  const template = namingConfig.template;

  const updateValidation = useCallback(async () => {
    if (!template.trim()) {
      setValidation(null);
      return;
    }
    const now = new Date();
    const segments = [
      {
        filename: outputFilename || 'Video',
        extension: 'mp4',
        index: 1,
        startTime: 0,
        endTime: 0,
        duration: 0,
        chapter: undefined,
        resolution: undefined,
        width: undefined,
        height: undefined,
        folder: undefined,
        originalNum: undefined,
        playlist: undefined,
        playlistIndex: undefined,
        videoCount: videoCount || undefined,
        totalDuration: totalDurationSecs || undefined,
        date: now.toISOString().split('T')[0],
        time: `${String(now.getHours()).padStart(2, '0')}-${String(now.getMinutes()).padStart(2, '0')}-${String(now.getSeconds()).padStart(2, '0')}`,
        prefix: namingConfig.prefix,
        suffix: namingConfig.suffix,
        lang: undefined,
        langName: undefined,
        partLabel: undefined,
      },
    ];

    try {
      const result = await tauriCommands.validateNamingTemplate(
        template,
        namingConfig,
        segments,
        1
      );
      setValidation(result);
    } catch {
      setValidation(null);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [template, namingConfig, outputFilename]);

  useEffect(() => {
    const timer = setTimeout(updateValidation, 150);
    return () => clearTimeout(timer);
  }, [updateValidation]);

  const activeMode = NAMING_MODES.find((m) => m.id === namingConfig.mode);
  const errors = validation?.errors ?? [];
  const warnings = validation?.warnings ?? [];
  const preview = validation?.resolvedPreview;

  return (
    <div className="p-4 rounded-xl border border-border/80 bg-bg-surface/30 backdrop-blur-md">
      {/* Header */}
      <button
        onClick={() => setExpanded((v) => !v)}
        className="flex items-center justify-between w-full text-left"
      >
        <div className="flex items-center gap-2">
          <Type className="w-4 h-4 text-accent-400" />
          <div>
            <span className="text-xs font-semibold text-text-primary">Output Naming</span>
            <span className="text-[10px] text-text-muted ml-2">{activeMode?.label}</span>
          </div>
        </div>
        {expanded ? (
          <ChevronUp className="w-4 h-4 text-text-muted" />
        ) : (
          <ChevronDown className="w-4 h-4 text-text-muted" />
        )}
      </button>

      {expanded && (
        <div className="mt-4 space-y-4">
          {/* Mode pills — all 9 modes shown in scrollable wrapping layout */}
          <div className="flex flex-wrap gap-1.5 max-h-24 overflow-y-auto scrollbar-thin">
            {NAMING_MODES.map((m) => {
              const isActive = m.id === namingConfig.mode;
              return (
                <button
                  key={m.id}
                  onClick={() =>
                    setNamingConfig({
                      ...namingConfig,
                      mode: m.id as NamingModeId,
                      template: m.defaultTemplate,
                    })
                  }
                  className={cn(
                    'px-2.5 py-1 rounded-lg text-[11px] font-medium transition-all border',
                    isActive
                      ? 'bg-accent-500/10 border-accent-500/30 text-accent-300'
                      : 'bg-bg-elevated/20 border-border/80 text-text-muted hover:border-border-strong'
                  )}
                >
                  {m.label}
                </button>
              );
            })}
          </div>

          {/* Template input */}
          <div>
            <input
              type="text"
              value={template}
              onChange={(e) =>
                setNamingConfig({ ...namingConfig, template: e.target.value })
              }
              placeholder="{filename}_Merged_{num3}"
              className={cn(
                'w-full px-3 py-2 rounded-lg bg-bg-elevated/30 border text-xs font-mono',
                'focus:outline-none focus:ring-1',
                errors.length > 0
                  ? 'border-red-500/40 focus:border-red-500/50 focus:ring-red-500/30'
                  : 'border-border/80 focus:border-accent-500/50 focus:ring-accent-500/30'
              )}
            />
          </div>

          {/* Errors */}
          {errors.length > 0 && (
            <div className="space-y-1">
              {errors.map((err, i) => (
                <div key={i} className="text-[10px] text-red-400 flex items-start gap-1.5">
                  <span className="mt-0.5">✕</span>
                  <span>{err.message}</span>
                </div>
              ))}
            </div>
          )}

          {/* Warnings */}
          {warnings.map((warn, i) => (
            <div key={i} className="text-[10px] text-yellow-400 flex items-start gap-1.5">
              <span className="mt-0.5">⚠</span>
              <span>{warn.message}</span>
            </div>
          ))}

          {/* Preview */}
          {preview && (
            <div className="text-[10px] text-text-muted font-mono bg-bg-elevated/20 rounded px-2 py-1.5 border border-border/60">
              <span className="text-text-secondary">Preview: </span>
              <span className="text-text-primary">{preview}</span>
            </div>
          )}
        </div>
      )}
    </div>
  );
}