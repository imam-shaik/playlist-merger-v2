// ────────────────────────────────────────────────
// RepeatScreen — Top-level screen for repeating /
// extending playlist output by count or duration.
// Both modes can be active simultaneously.
// ────────────────────────────────────────────────

import React, { useState, useMemo, useCallback } from 'react';
import { motion } from 'framer-motion';
import {
  Repeat, ChevronDown, ChevronRight, Check, Play,
  Sliders, Eye, Info, AlertTriangle, ToggleLeft, ToggleRight,
} from 'lucide-react';
import { cn } from '@/utils/cn';
import { useMergeStore } from '@/store/mergeStore';
import { usePlaylistStore } from '@/store/playlistStore';
import { useWorkspaceStore } from '@/store/workspaceStore';
import { useMerge } from '@/hooks/useMerge';
import { useAppStore } from '@/store/appStore';
import { formatDuration, totalDuration } from '@/utils';
import { REPEAT_COUNT_PRESETS, REPEAT_DURATION_PRESETS_MINUTES, REPEAT_DURATION_PRESETS_HOURS } from '@/constants';
import type { DurationUnit } from '@/types';

export function RepeatScreen() {
  const entries = usePlaylistStore((s) => s.entries);
  const totalDur = totalDuration(entries);
  const repeatConfig = useMergeStore((s) => s.repeatConfig);
  const setRepeatConfig = useMergeStore((s) => s.setRepeatConfig);
  const { startMerge } = useMerge();

  const sectionsCollapsed = useWorkspaceStore((s) => s.sectionsCollapsed);
  const toggleSection = useWorkspaceStore((s) => s.toggleSection);

  const [customCount, setCustomCount] = useState('');
  const [customDurationValue, setCustomDurationValue] = useState('');
  const [customBoundaryLabel, setCustomBoundaryLabel] = useState('');

  const { enabled, byCount, repeatCount, untilDuration, targetDurationSeconds, durationUnit, insertBoundaryCards, boundaryCardTemplate } = repeatConfig;

  // ── Compute effective repeat count ──────────────────────────────────────
  // When both modes are active, use the MAX of both values.
  // When only one is active, use that one.
  const effectiveRepeatCount = useMemo(() => {
    if (!enabled) return 0;
    if (totalDur <= 0) return 0;

    let countFromCount = 0;
    let countFromDuration = 0;

    if (byCount) {
      const custom = customCount ? parseInt(customCount, 10) : 0;
      countFromCount = custom > 0 ? custom : repeatCount;
    }

    if (untilDuration) {
      const custom = customDurationValue ? parseFloat(customDurationValue) : 0;
      const targetSeconds = custom > 0
        ? (durationUnit === 'hours' ? custom * 3600 : custom * 60)
        : targetDurationSeconds;
      if (targetSeconds > 0) {
        countFromDuration = Math.ceil(targetSeconds / totalDur);
      }
    }

    if (byCount && untilDuration) return Math.max(countFromCount, countFromDuration);
    if (byCount) return countFromCount;
    if (untilDuration) return countFromDuration;
    return 0;
  }, [enabled, byCount, repeatCount, untilDuration, targetDurationSeconds, durationUnit, totalDur, customCount, customDurationValue]);

  // ── Compute final duration ──────────────────────────────────────────────
  const finalDuration = totalDur * (effectiveRepeatCount || 1);
  const totalReferences = entries.length * (effectiveRepeatCount || 0);
  const isSingleVideo = entries.length === 1;
  const singleVideo = entries[0];

  // ── Duration validation ─────────────────────────────────────────────────
  const targetSeconds = useMemo(() => {
    if (!untilDuration) return 0;
    const custom = customDurationValue ? parseFloat(customDurationValue) : 0;
    return custom > 0
      ? (durationUnit === 'hours' ? custom * 3600 : custom * 60)
      : targetDurationSeconds;
  }, [untilDuration, customDurationValue, durationUnit, targetDurationSeconds]);

  const isTargetLessThanActual = untilDuration && targetSeconds > 0 && targetSeconds < totalDur;
  const isTargetTooSmall = untilDuration && targetSeconds > 0 && targetSeconds < 60;

  // ── Handlers ────────────────────────────────────────────────────────────
  const handleToggleMaster = useCallback(() => {
    const newEnabled = !enabled;
    setRepeatConfig({ ...repeatConfig, enabled: newEnabled });
    // Auto-enable both sub-modes when turning on for the first time
    if (newEnabled && !byCount && !untilDuration) {
      setRepeatConfig({
        ...repeatConfig,
        enabled: true,
        byCount: true,
        untilDuration: true,
      });
    }
  }, [enabled, byCount, untilDuration, repeatConfig, setRepeatConfig]);

  const handleToggleByCount = useCallback(() => {
    const newByCount = !byCount;
    setRepeatConfig({ ...repeatConfig, byCount: newByCount, enabled: true });
  }, [byCount, repeatConfig, setRepeatConfig]);

  const handleToggleUntilDuration = useCallback(() => {
    const newUntilDuration = !untilDuration;
    setRepeatConfig({ ...repeatConfig, untilDuration: newUntilDuration, enabled: true });
  }, [untilDuration, repeatConfig, setRepeatConfig]);

  const handleCountPreset = useCallback((value: string) => {
    const count = parseInt(value, 10);
    setCustomCount('');
    setRepeatConfig({ ...repeatConfig, enabled: true, byCount: true, repeatCount: count });
  }, [repeatConfig, setRepeatConfig]);

  const handleDurationPreset = useCallback((seconds: number) => {
    setCustomDurationValue('');
    setRepeatConfig({
      ...repeatConfig,
      enabled: true,
      untilDuration: true,
      targetDurationSeconds: seconds,
    });
  }, [repeatConfig, setRepeatConfig]);

  const handleDurationUnitChange = useCallback((unit: DurationUnit) => {
    setRepeatConfig({ ...repeatConfig, durationUnit: unit });
    setCustomDurationValue('');
  }, [repeatConfig, setRepeatConfig]);

  const handleToggleBoundaryCards = useCallback(() => {
    setRepeatConfig({ ...repeatConfig, insertBoundaryCards: !insertBoundaryCards });
  }, [insertBoundaryCards, repeatConfig, setRepeatConfig]);

  const handleBoundaryLabelPreset = useCallback((template: string) => {
    setCustomBoundaryLabel('');
    setRepeatConfig({ ...repeatConfig, boundaryCardTemplate: template });
  }, [repeatConfig, setRepeatConfig]);

  const handleCustomBoundaryLabel = useCallback((label: string) => {
    setCustomBoundaryLabel(label);
  }, []);

  const handleApplyCustomBoundaryLabel = useCallback(() => {
    if (customBoundaryLabel.trim() !== '') {
      setRepeatConfig({ ...repeatConfig, boundaryCardTemplate: customBoundaryLabel });
    }
  }, [customBoundaryLabel, repeatConfig, setRepeatConfig]);

  const handleMerge = async () => {
    if (entries.length < 1) {
      useAppStore.getState().showToast({ type: 'error', title: 'Add files to the playlist first' });
      return;
    }
    if (!enabled || effectiveRepeatCount === 0) {
      useAppStore.getState().showToast({ type: 'error', title: 'Enable a repeat mode first' });
      return;
    }
    await startMerge(() => {
      useAppStore.getState().setScreen('merge');
    });
  };

  // ── Collapsed badge label ───────────────────────────────────────────────
  const activeModeLabel = useMemo(() => {
    if (!enabled) return 'Disabled';
    const parts: string[] = [];
    if (byCount) parts.push(`×${customCount || repeatCount}`);
    if (untilDuration) {
      const custom = customDurationValue ? parseFloat(customDurationValue) : 0;
      const secs = custom > 0
        ? (durationUnit === 'hours' ? custom * 3600 : custom * 60)
        : targetDurationSeconds;
      parts.push(`Until ${formatDuration(secs)}`);
    }
    return parts.join(' + ') || 'Enabled';
  }, [enabled, byCount, repeatCount, untilDuration, targetDurationSeconds, durationUnit, customCount, customDurationValue]);

  return (
    <div className="flex-1 flex flex-col min-h-0 bg-bg-base/10">
      <div className="flex-1 overflow-y-auto custom-scrollbar">
        <div className="w-full p-6 space-y-5">

          {/* Header */}
          <div className="flex items-center gap-3 pb-5 border-b border-border/80">
            <div className="w-9 h-9 rounded-xl bg-accent-500/10 border border-accent-500/20 flex items-center justify-center shadow-glow-sm">
              <Repeat className="w-4.5 h-4.5 text-accent-400" />
            </div>
            <div>
              <h2 className="text-sm font-semibold text-text-primary tracking-wide">Repeat / Extend Video</h2>
              <p className="text-[11px] text-text-muted mt-0.5">
                Repeat your playlist to create a longer single output — no file copies, references only
              </p>
            </div>
          </div>

          {/* ── Repeat Config Section ── */}
          <div className="glass rounded-xl overflow-hidden shadow-card hover:border-border-strong transition-all duration-300">
            <button
              onClick={() => toggleSection('encoding')}
              className={cn(
                "flex items-center justify-between w-full px-4 py-3.5 text-left transition-colors duration-150 bg-bg-surface/40 hover:bg-bg-surface/70 border-b border-transparent",
                !sectionsCollapsed.encoding && "border-border/60 bg-bg-surface/60"
              )}
            >
              <div className="flex items-center gap-2.5 min-w-0">
                <Sliders className="w-4 h-4 text-accent-400 shrink-0" />
                <div className="min-w-0">
                  <span className="text-xs font-semibold text-text-secondary uppercase tracking-wider block">
                    Repeat Settings
                  </span>
                  {!sectionsCollapsed.encoding && (
                    <span className="text-[10px] text-text-muted block mt-0.5">
                      Enable and configure repeat options
                    </span>
                  )}
                </div>
              </div>
              <div className="flex items-center gap-2">
                {sectionsCollapsed.encoding && (
                  <span className="text-[9px] bg-accent-500/10 border border-accent-500/20 text-accent-400 px-2 py-0.5 rounded font-mono font-bold uppercase tracking-wider">
                    {activeModeLabel}
                  </span>
                )}
                {sectionsCollapsed.encoding ? (
                  <ChevronRight className="w-4 h-4 text-text-muted" />
                ) : (
                  <ChevronDown className="w-4 h-4 text-text-muted" />
                )}
              </div>
            </button>
            <motion.div
              animate={{
                height: sectionsCollapsed.encoding ? 0 : 'auto',
                opacity: sectionsCollapsed.encoding ? 0 : 1,
              }}
              transition={{ duration: 0.2, ease: 'easeInOut' }}
              className="overflow-hidden"
            >
              <div className="p-4 bg-bg-elevated/10 space-y-5">

                {/* Master toggle */}
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    {enabled ? (
                      <ToggleRight className="w-5 h-5 text-accent-400" />
                    ) : (
                      <ToggleLeft className="w-5 h-5 text-text-disabled" />
                    )}
                    <div>
                      <span className="text-xs font-semibold text-text-primary">
                        {enabled ? 'Repeat Enabled' : 'Repeat Disabled'}
                      </span>
                      <p className="text-[10px] text-text-muted">
                        {enabled ? 'Playlist will be repeated in the output' : 'Output is a single pass of the playlist'}
                      </p>
                    </div>
                  </div>
                  <button
                    onClick={handleToggleMaster}
                    className={cn(
                      "relative inline-flex h-6 w-11 shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none",
                      enabled ? "bg-accent-500" : "bg-bg-overlay border-border"
                    )}
                    role="switch"
                    aria-checked={enabled}
                  >
                    <span
                      aria-hidden="true"
                      className={cn(
                        "pointer-events-none inline-block h-5 w-5 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out",
                        enabled ? "translate-x-5" : "translate-x-0"
                      )}
                    />
                  </button>
                </div>

                {enabled && (
                  <motion.div
                    initial={{ opacity: 0, height: 0 }}
                    animate={{ opacity: 1, height: 'auto' }}
                    className="space-y-5 overflow-hidden"
                  >
                    {/* ── By Count ── */}
                    <div className="border border-border/60 rounded-lg p-3 space-y-3">
                      <div className="flex items-center justify-between">
                        <div className="flex items-center gap-2">
                          <button
                            onClick={handleToggleByCount}
                            className={cn(
                              'w-4 h-4 rounded border-2 flex items-center justify-center transition-all shrink-0',
                              byCount
                                ? 'bg-accent-500 border-accent-500'
                                : 'border-text-disabled bg-transparent hover:border-text-muted'
                            )}
                          >
                            {byCount && <Check className="w-3 h-3 text-white" />}
                          </button>
                          <span className="text-xs font-semibold text-text-primary">Repeat By Count</span>
                        </div>
                        {byCount && (
                          <span className="text-[9px] bg-accent-500/10 border border-accent-500/20 text-accent-400 px-2 py-0.5 rounded font-mono font-bold">
                            ×{customCount || repeatCount}
                          </span>
                        )}
                      </div>

                      {byCount && (
                        <div>
                          <div className="flex flex-wrap gap-2">
                            {REPEAT_COUNT_PRESETS.map((preset) => (
                              <button
                                key={preset.value}
                                onClick={() => handleCountPreset(preset.value)}
                                className={cn(
                                  'px-4 py-2 rounded-lg border text-xs font-medium transition-all',
                                  !customCount && repeatCount === parseInt(preset.value, 10)
                                    ? 'bg-accent-muted border-accent-500/40 text-accent-400'
                                    : 'bg-bg-overlay border-border hover:border-border-strong text-text-secondary'
                                )}
                                aria-pressed={!customCount && repeatCount === parseInt(preset.value, 10)}
                              >
                                {preset.label}
                              </button>
                            ))}
                          </div>
                          <div className="mt-2 flex items-center gap-2">
                            <span className="text-2xs text-text-muted">Custom:</span>
                            <input
                              type="number"
                              min={2}
                              max={100}
                              value={customCount}
                              onChange={(e) => {
                                const valStr = e.target.value;
                                setCustomCount(valStr);
                                if (valStr.trim() !== '') {
                                  const val = parseInt(valStr, 10);
                                  if (!isNaN(val) && val >= 2 && val <= 100) {
                                    setRepeatConfig({ ...repeatConfig, enabled: true, byCount: true, repeatCount: val });
                                  }
                                }
                              }}
                              placeholder="Count"
                              className={cn(
                                'w-24 h-8 px-3 text-xs bg-bg-overlay border rounded-lg',
                                'text-text-primary focus:outline-none focus:ring-1 focus:ring-accent-500/40',
                                'border-border focus:border-accent-500/60'
                              )}
                              aria-label="Custom repeat count"
                            />
                            <span className="text-2xs text-text-muted">times</span>
                          </div>
                        </div>
                      )}
                    </div>

                    {/* ── Until Duration ── */}
                    <div className={cn(
                      'border rounded-lg p-3 space-y-3',
                      isTargetLessThanActual ? 'border-warning/40 bg-warning/5' : 'border-border/60'
                    )}>
                      <div className="flex items-center justify-between">
                        <div className="flex items-center gap-2">
                          <button
                            onClick={handleToggleUntilDuration}
                            className={cn(
                              'w-4 h-4 rounded border-2 flex items-center justify-center transition-all shrink-0',
                              untilDuration
                                ? 'bg-accent-500 border-accent-500'
                                : 'border-text-disabled bg-transparent hover:border-text-muted'
                            )}
                          >
                            {untilDuration && <Check className="w-3 h-3 text-white" />}
                          </button>
                          <span className="text-xs font-semibold text-text-primary">Until Duration</span>
                        </div>
                        {untilDuration && (
                          <span className="text-[9px] bg-accent-500/10 border border-accent-500/20 text-accent-400 px-2 py-0.5 rounded font-mono font-bold">
                            {formatDuration(targetSeconds)}
                          </span>
                        )}
                      </div>

                      {untilDuration && (
                        <div className="space-y-3">
                          {/* Unit selector */}
                          <div className="flex gap-2">
                            {[
                              { value: 'minutes' as DurationUnit, label: 'Minutes' },
                              { value: 'hours' as DurationUnit, label: 'Hours' },
                            ].map((opt) => (
                              <button
                                key={opt.value}
                                onClick={() => handleDurationUnitChange(opt.value)}
                                className={cn(
                                  'flex-1 px-3 py-1.5 rounded-lg border text-xs font-medium transition-all',
                                  durationUnit === opt.value
                                    ? 'bg-accent-muted border-accent-500/40 text-accent-400'
                                    : 'bg-bg-overlay border-border hover:border-border-strong text-text-secondary'
                                )}
                                aria-pressed={durationUnit === opt.value}
                              >
                                {opt.label}
                              </button>
                            ))}
                          </div>

                          {/* Presets based on selected unit */}
                          <div className="flex flex-wrap gap-2">
                            {(durationUnit === 'minutes' ? REPEAT_DURATION_PRESETS_MINUTES : REPEAT_DURATION_PRESETS_HOURS).map((preset) => (
                              <button
                                key={preset.value}
                                onClick={() => handleDurationPreset(preset.seconds)}
                                className={cn(
                                  'px-3 py-1.5 rounded-lg border text-xs font-medium transition-all',
                                  !customDurationValue && targetDurationSeconds === preset.seconds
                                    ? 'bg-accent-muted border-accent-500/40 text-accent-400'
                                    : 'bg-bg-overlay border-border hover:border-border-strong text-text-secondary'
                                )}
                                aria-pressed={!customDurationValue && targetDurationSeconds === preset.seconds}
                              >
                                {preset.label}
                              </button>
                            ))}
                          </div>

                          {/* Custom input */}
                          <div className="flex items-center gap-2">
                            <span className="text-2xs text-text-muted">Custom:</span>
                            <input
                              type="number"
                              min={durationUnit === 'minutes' ? 1 : 0.5}
                              step={durationUnit === 'minutes' ? 5 : 0.5}
                              value={customDurationValue}
                              onChange={(e) => {
                                const valStr = e.target.value;
                                setCustomDurationValue(valStr);
                              }}
                              placeholder={durationUnit === 'minutes' ? 'Minutes' : 'Hours'}
                              className={cn(
                                'w-28 h-8 px-3 text-xs bg-bg-overlay border rounded-lg',
                                'text-text-primary focus:outline-none focus:ring-1 focus:ring-accent-500/40',
                                isTargetLessThanActual
                                  ? 'border-warning focus:border-warning/60'
                                  : 'border-border focus:border-accent-500/60'
                              )}
                              aria-label={`Custom target duration in ${durationUnit}`}
                            />
                            <span className="text-2xs text-text-muted">{durationUnit}</span>
                          </div>

                          {/* Validation warnings */}
                          {isTargetLessThanActual && (
                            <div className="flex items-start gap-2 px-2 py-1.5 rounded bg-warning/10 border border-warning/20">
                              <AlertTriangle className="w-3 h-3 text-warning shrink-0 mt-0.5" />
                              <p className="text-[10px] text-warning leading-tight">
                                Target ({formatDuration(targetSeconds)}) is less than playlist duration ({formatDuration(totalDur)}).
                                The repeat count will be {Math.ceil(targetSeconds / totalDur)}× (minimum 1 full cycle).
                              </p>
                            </div>
                          )}
                          {isTargetTooSmall && !isTargetLessThanActual && (
                            <div className="flex items-start gap-2 px-2 py-1.5 rounded bg-warning/10 border border-warning/20">
                              <AlertTriangle className="w-3 h-3 text-warning shrink-0 mt-0.5" />
                              <p className="text-[10px] text-warning leading-tight">
                                Target is very short. Minimum recommended: {formatDuration(totalDur)} (1× playlist duration).
                              </p>
                            </div>
                          )}

                          {/* Info: actual playlist duration */}
                          <div className="flex items-center gap-2 text-[10px] text-text-muted">
                            <Info className="w-3 h-3 shrink-0" />
                            <span>Playlist duration: <span className="text-text-secondary font-mono font-medium">{formatDuration(totalDur)}</span></span>
                          </div>
                        </div>
                      )}
                    </div>

                    {/* ── Both active info ── */}
                    {byCount && untilDuration && (
                      <div className="flex items-start gap-2 px-3 py-2 rounded-lg bg-accent-500/5 border border-accent-500/15">
                        <Info className="w-3.5 h-3.5 text-accent-400 shrink-0 mt-0.5" />
                        <p className="text-[10px] text-accent-300/80 leading-tight">
                          Both modes active — final count is the <span className="font-semibold">higher</span> of the two values.
                        </p>
                      </div>
                    )}

                    {/* ── Boundary Cards ── */}
                    <div className="border border-border/60 rounded-lg p-3 space-y-3">
                      <div className="flex items-center justify-between">
                        <div className="flex items-center gap-2">
                          <button
                            onClick={handleToggleBoundaryCards}
                            className={cn(
                              'w-4 h-4 rounded border-2 flex items-center justify-center transition-all shrink-0',
                              insertBoundaryCards
                                ? 'bg-accent-500 border-accent-500'
                                : 'border-text-disabled bg-transparent hover:border-text-muted'
                            )}
                          >
                            {insertBoundaryCards && <Check className="w-3 h-3 text-white" />}
                          </button>
                          <span className="text-xs font-semibold text-text-primary">Insert Boundary Cards</span>
                        </div>
                        {insertBoundaryCards && (
                          <span className="text-[9px] bg-accent-500/10 border border-accent-500/20 text-accent-400 px-2 py-0.5 rounded font-mono font-bold">
                            {boundaryCardTemplate.replace('{n}', '1')}
                          </span>
                        )}
                      </div>

                      {insertBoundaryCards && (
                        <div className="space-y-2">
                          <div className="text-[10px] text-text-muted">Label template</div>
                          <div className="flex flex-wrap gap-2">
                            {[
                              { value: '🔁 Repeat {n}', label: '🔁 Repeat {n}' },
                              { value: 'Cycle {n}', label: 'Cycle {n}' },
                              { value: '--- Repeat {n} ---', label: '--- Repeat {n} ---' },
                            ].map((opt) => (
                              <button
                                key={opt.value}
                                onClick={() => handleBoundaryLabelPreset(opt.value)}
                                className={cn(
                                  'px-3 py-1.5 rounded-lg border text-xs font-medium transition-all',
                                  !customBoundaryLabel && boundaryCardTemplate === opt.value
                                    ? 'bg-accent-muted border-accent-500/40 text-accent-400'
                                    : 'bg-bg-overlay border-border hover:border-border-strong text-text-secondary'
                                )}
                                aria-pressed={!customBoundaryLabel && boundaryCardTemplate === opt.value}
                              >
                                {opt.label}
                              </button>
                            ))}
                          </div>
                          <div className="flex items-center gap-2">
                            <span className="text-2xs text-text-muted">Custom:</span>
                            <input
                              type="text"
                              value={customBoundaryLabel}
                              onChange={(e) => handleCustomBoundaryLabel(e.target.value)}
                              onBlur={handleApplyCustomBoundaryLabel}
                              onKeyDown={(e) => {
                                if (e.key === 'Enter') {
                                  handleApplyCustomBoundaryLabel();
                                }
                              }}
                              placeholder={boundaryCardTemplate}
                              className={cn(
                                'flex-1 h-8 px-3 text-xs bg-bg-overlay border rounded-lg',
                                'text-text-primary focus:outline-none focus:ring-1 focus:ring-accent-500/40',
                                'border-border focus:border-accent-500/60'
                              )}
                              aria-label="Custom boundary card label"
                            />
                          </div>
                          <div className="text-[9px] text-text-muted">
                            Use &#123;n&#125; for cycle number (e.g., "Repeat &#123;n&#125;" shows "Repeat 1", "Repeat 2", ...)
                          </div>
                        </div>
                      )}
                    </div>
                  </motion.div>
                )}
              </div>
            </motion.div>
          </div>

          {/* ── Preview Section ── */}
          <div className="glass rounded-xl overflow-hidden shadow-card hover:border-border-strong transition-all duration-300">
            <button
              onClick={() => toggleSection('summary')}
              className={cn(
                "flex items-center justify-between w-full px-4 py-3.5 text-left transition-colors duration-150 bg-bg-surface/40 hover:bg-bg-surface/70 border-b border-transparent",
                !sectionsCollapsed.summary && "border-border/60 bg-bg-surface/60"
              )}
            >
              <div className="flex items-center gap-2.5 min-w-0">
                <Eye className="w-4 h-4 text-accent-400 shrink-0" />
                <div className="min-w-0">
                  <span className="text-xs font-semibold text-text-secondary uppercase tracking-wider block">
                    Repeat Preview
                  </span>
                  {!sectionsCollapsed.summary && (
                    <span className="text-[10px] text-text-muted block mt-0.5">
                      Verify the repeated output structure
                    </span>
                  )}
                </div>
              </div>
              <div className="flex items-center gap-2">
                {sectionsCollapsed.summary && enabled && effectiveRepeatCount > 0 && (
                  <span className="text-[9px] bg-accent-500/10 border border-accent-500/20 text-accent-400 px-2 py-0.5 rounded font-mono font-bold uppercase tracking-wider">
                    ×{effectiveRepeatCount}
                  </span>
                )}
                {sectionsCollapsed.summary ? (
                  <ChevronRight className="w-4 h-4 text-text-muted" />
                ) : (
                  <ChevronDown className="w-4 h-4 text-text-muted" />
                )}
              </div>
            </button>
            <motion.div
              animate={{
                height: sectionsCollapsed.summary ? 0 : 'auto',
                opacity: sectionsCollapsed.summary ? 0 : 1,
              }}
              transition={{ duration: 0.2, ease: 'easeInOut' }}
              className="overflow-hidden"
            >
              <div className="p-4 bg-bg-elevated/10">

                {!enabled || effectiveRepeatCount === 0 || totalDur === 0 ? (
                  <div className="text-center py-8">
                    <Repeat className="w-8 h-8 text-text-disabled mx-auto mb-2" />
                    <p className="text-xs text-text-muted">
                      {entries.length === 0
                        ? 'Add files to the playlist to see preview'
                        : !enabled
                        ? 'Enable repeat to see preview'
                        : 'Configure repeat options to see preview'}
                    </p>
                  </div>
                ) : isSingleVideo && singleVideo ? (
                  /* ── Single Video Preview ── */
                  <div className="space-y-3">
                    <div className="bg-bg-base/60 rounded-lg p-3 border border-border/20">
                      <p className="text-2xs font-semibold text-text-muted mb-2">Playlist Preview</p>
                      {Array.from({ length: Math.min(effectiveRepeatCount, 8) }, (_, i) => (
                        <div key={i} className="flex items-center gap-2 py-1">
                          <Check className="w-3 h-3 text-success shrink-0" />
                          <span className="text-xs text-text-secondary font-mono">
                            {singleVideo.name}
                          </span>
                          <span className="text-2xs text-text-muted">
                            ({formatDuration(singleVideo.mediaInfo?.duration ?? 0)})
                          </span>
                        </div>
                      ))}
                      {effectiveRepeatCount > 8 && (
                        <p className="text-2xs text-text-muted mt-1">
                          ...and {effectiveRepeatCount - 8} more repetitions
                        </p>
                      )}
                    </div>
                    <StatsBlock
                      totalDur={totalDur}
                      effectiveRepeatCount={effectiveRepeatCount}
                      finalDuration={finalDuration}
                    />
                  </div>
                ) : (
                  /* ── Multiple Videos Preview ── */
                  <div className="space-y-3">
                    <div className="bg-bg-base/60 rounded-lg p-3 border border-border/20">
                      <p className="text-2xs font-semibold text-text-muted mb-2">Playlist Repeat Preview</p>
                      {Array.from({ length: Math.min(effectiveRepeatCount, 5) }, (_, cycleIdx) => (
                        <div key={cycleIdx} className="mb-2 last:mb-0">
                          <p className="text-2xs font-semibold text-accent-400/80 mb-1">
                            Cycle {cycleIdx + 1}
                          </p>
                          <div className="space-y-0.5 pl-2">
                            {entries.slice(0, 5).map((entry, i) => (
                              <div key={i} className="flex items-center gap-2">
                                <Check className="w-3 h-3 text-success shrink-0" />
                                <span className="text-2xs text-text-secondary font-mono truncate">
                                  {entry.name}
                                </span>
                              </div>
                            ))}
                            {entries.length > 5 && (
                              <p className="text-[9px] text-text-muted pl-5">
                                ...and {entries.length - 5} more
                              </p>
                            )}
                          </div>
                        </div>
                      ))}
                      {effectiveRepeatCount > 5 && (
                        <p className="text-2xs text-text-muted mt-1">
                          ...and {effectiveRepeatCount - 5} more cycles
                        </p>
                      )}
                    </div>
                    <StatsBlock
                      totalDur={totalDur}
                      effectiveRepeatCount={effectiveRepeatCount}
                      finalDuration={finalDuration}
                      videosPerCycle={entries.length}
                      totalReferences={totalReferences}
                    />
                  </div>
                )}
              </div>
            </motion.div>
          </div>

          {/* ── Action Bar ── */}
          <div className="glass rounded-xl p-4 flex items-center justify-between gap-4 border border-border shadow-card">
            <div className="flex items-start gap-2.5 min-w-0">
              <Info className="w-4 h-4 text-text-muted shrink-0 mt-0.5" />
              <p className="text-[10px] text-text-muted leading-relaxed">
                {!enabled
                  ? 'Enable repeat to extend your playlist output.'
                  : `Creates 1 output file with ${effectiveRepeatCount}× repetition. No file copies — FFmpeg concat references the same files.`}
              </p>
            </div>
            <button
              onClick={handleMerge}
              disabled={entries.length < 1 || !enabled || effectiveRepeatCount === 0}
              className={cn(
                'flex items-center gap-2 px-5 py-2.5 rounded-lg text-xs font-semibold transition-all shrink-0',
                entries.length >= 1 && enabled && effectiveRepeatCount > 0
                  ? 'bg-accent-500 text-white hover:bg-accent-600 shadow-glow-sm'
                  : 'bg-bg-overlay text-text-disabled cursor-not-allowed'
              )}
            >
              <Play className="w-3.5 h-3.5" />
              Merge {effectiveRepeatCount > 0 ? `(${formatDuration(finalDuration)})` : ''}
            </button>
          </div>

        </div>
      </div>
    </div>
  );
}

// ── Reusable Stats Block ──────────────────────────────────────────────────

function StatsBlock({
  totalDur,
  effectiveRepeatCount,
  finalDuration,
  videosPerCycle,
  totalReferences,
}: {
  totalDur: number;
  effectiveRepeatCount: number;
  finalDuration: number;
  videosPerCycle?: number;
  totalReferences?: number;
}) {
  return (
    <div className="bg-bg-base/60 rounded-lg p-3 border border-border/20 space-y-1">
      {videosPerCycle !== undefined && (
        <div className="flex justify-between text-2xs">
          <span className="text-text-muted">Videos Per Cycle</span>
          <span className="text-text-secondary font-mono">{videosPerCycle}</span>
        </div>
      )}
      {totalReferences !== undefined && (
        <div className="flex justify-between text-2xs">
          <span className="text-text-muted">Total References</span>
          <span className="text-text-secondary font-mono">{totalReferences}</span>
        </div>
      )}
      <div className="flex justify-between text-2xs">
        <span className="text-text-muted">Original Duration</span>
        <span className="text-text-secondary font-mono">{formatDuration(totalDur)}</span>
      </div>
      <div className="flex justify-between text-2xs">
        <span className="text-text-muted">Repeat Count</span>
        <span className="text-accent-400 font-mono font-semibold">×{effectiveRepeatCount}</span>
      </div>
      <div className="border-t border-border/40 pt-1 flex justify-between text-2xs">
        <span className="text-text-secondary font-semibold">Final Duration</span>
        <span className="text-accent-400 font-mono font-bold">{formatDuration(finalDuration)}</span>
      </div>
      <p className="text-[9px] text-text-muted text-center pt-1">
        Output: 1 file — no copies created, references only
      </p>
    </div>
  );
}
