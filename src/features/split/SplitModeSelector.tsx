import React from 'react';
import { motion } from 'framer-motion';
import {
  Layers, Clock, BookMarked, SlidersHorizontal,
  ListVideo, HardDrive, GraduationCap, Subtitles,
} from 'lucide-react';
import { cn } from '@/utils/cn';
import { useSplitStore } from '@/store/splitStore';
import { SPLIT_MODES_DEFINITIONS, SPLIT_PART_DURATION_PRESETS, SPLIT_OUTPUT_SIZE_PRESETS, COURSE_MODE_OPTIONS, COURSE_HOURS_PRESETS, PART_COUNT_PRESETS_SPLIT } from '@/constants';
import type { SplitMode } from '@/types';

const MODE_ICONS: Record<string, React.ReactNode> = {
  byParts: <Layers className="w-4 h-4" />,
  byDuration: <Clock className="w-4 h-4" />,
  byChapters: <BookMarked className="w-4 h-4" />,
  customRanges: <SlidersHorizontal className="w-4 h-4" />,
  byPlaylistItems: <ListVideo className="w-4 h-4" />,
  byOutputSize: <HardDrive className="w-4 h-4" />,
  smartCourse: <GraduationCap className="w-4 h-4" />,
};

export function SplitModeSelector() {
  const splitMode = useSplitStore((s) => s.splitMode);
  const splitParams = useSplitStore((s) => s.splitParams);
  const setSplitMode = useSplitStore((s) => s.setSplitMode);
  const setSplitParams = useSplitStore((s) => s.setSplitParams);
  const inputDuration = useSplitStore((s) => s.inputDuration);
  const inputSizeBytes = useSplitStore((s) => s.inputSizeBytes);

  // --- Custom byParts state ---
  const [customPartCount, setCustomPartCount] = React.useState('');
  const [partCountError, setPartCountError] = React.useState<string | null>(null);

  // --- Custom byDuration state ---
  const [customDurationVal, setCustomDurationVal] = React.useState('');
  const [customDurationUnit, setCustomDurationUnit] = React.useState<'seconds' | 'minutes' | 'hours'>('minutes');
  const [durationError, setDurationError] = React.useState<string | null>(null);

  // --- Custom byOutputSize state ---
  const [customSizeVal, setCustomSizeVal] = React.useState('');
  const [customSizeUnit, setCustomSizeUnit] = React.useState<'MB' | 'GB'>('MB');
  const [sizeError, setSizeError] = React.useState<string | null>(null);

  // --- Custom smartCourse state ---
  const [customHoursVal, setCustomHoursVal] = React.useState('');
  const [hoursError, setHoursError] = React.useState<string | null>(null);

  const updateStoreWithError = (hasError: boolean) => {
    const store = useSplitStore.getState();
    if (hasError) {
      store.setPlanError('Please resolve validation errors in split parameters first.');
      store.setCurrentPlan(null);
    } else {
      if (store.planError === 'Please resolve validation errors in split parameters first.') {
        store.setPlanError(null);
      }
    }
  };

  // Sync custom inputs from store if they change externally (like clicking a preset)
  const isPresetPartCount = PART_COUNT_PRESETS_SPLIT.some((p) => parseInt(p.value) === splitParams.partCount);
  React.useEffect(() => {
    if (splitParams.partCount !== undefined && !isPresetPartCount) {
      setCustomPartCount(splitParams.partCount.toString());
      setPartCountError(null);
    } else if (isPresetPartCount) {
      setCustomPartCount('');
      setPartCountError(null);
    }
  }, [splitParams.partCount, isPresetPartCount]);

  const isPresetDuration = SPLIT_PART_DURATION_PRESETS.some((p) => parseInt(p.value) === splitParams.partDuration);
  React.useEffect(() => {
    if (splitParams.partDuration !== undefined && !isPresetDuration) {
      const sec = splitParams.partDuration;
      if (sec % 3600 === 0) {
        setCustomDurationVal((sec / 3600).toString());
        setCustomDurationUnit('hours');
      } else if (sec % 60 === 0) {
        setCustomDurationVal((sec / 60).toString());
        setCustomDurationUnit('minutes');
      } else {
        setCustomDurationVal(sec.toString());
        setCustomDurationUnit('seconds');
      }
      setDurationError(null);
    } else if (isPresetDuration) {
      setCustomDurationVal('');
      setDurationError(null);
    }
  }, [splitParams.partDuration, isPresetDuration]);

  const isPresetSize = SPLIT_OUTPUT_SIZE_PRESETS.some((p) => parseInt(p.value) === splitParams.maxSizeBytes);
  React.useEffect(() => {
    if (splitParams.maxSizeBytes !== undefined && !isPresetSize) {
      const bytes = splitParams.maxSizeBytes;
      if (bytes % (1024 * 1024 * 1024) === 0) {
        setCustomSizeVal((bytes / (1024 * 1024 * 1024)).toString());
        setCustomSizeUnit('GB');
      } else if (bytes % (1024 * 1024) === 0) {
        setCustomSizeVal((bytes / (1024 * 1024)).toString());
        setCustomSizeUnit('MB');
      } else {
        setCustomSizeVal((Math.round((bytes / (1024 * 1024)) * 10) / 10).toString());
        setCustomSizeUnit('MB');
      }
      setSizeError(null);
    } else if (isPresetSize) {
      setCustomSizeVal('');
      setSizeError(null);
    }
  }, [splitParams.maxSizeBytes, isPresetSize]);

  const isPresetHours = COURSE_HOURS_PRESETS.some((p) => parseInt(p.value) === splitParams.hoursPerUnit);
  React.useEffect(() => {
    if (splitParams.hoursPerUnit !== undefined && !isPresetHours) {
      setCustomHoursVal(splitParams.hoursPerUnit.toString());
      setHoursError(null);
    } else if (isPresetHours) {
      setCustomHoursVal('');
      setHoursError(null);
    }
  }, [splitParams.hoursPerUnit, isPresetHours]);

  const handlePartCountChange = (valStr: string) => {
    setCustomPartCount(valStr);
    if (valStr.trim() === '') {
      setPartCountError(null);
      updateStoreWithError(false);
      setSplitParams({ partCount: 2 });
      return;
    }
    const val = parseInt(valStr, 10);
    if (isNaN(val)) {
      setPartCountError('Must be a number');
      updateStoreWithError(true);
    } else if (val < 2 || val > 1000) {
      setPartCountError('Must be between 2 and 1000');
      updateStoreWithError(true);
    } else if (val.toString() !== valStr.trim()) {
      setPartCountError('Must be a whole number');
      updateStoreWithError(true);
    } else {
      setPartCountError(null);
      updateStoreWithError(false);
      setSplitParams({ partCount: val });
    }
  };

  const handleDurationChange = (valStr: string, unit: 'seconds' | 'minutes' | 'hours') => {
    setCustomDurationVal(valStr);
    setCustomDurationUnit(unit);
    if (valStr.trim() === '') {
      setDurationError(null);
      updateStoreWithError(false);
      setSplitParams({ partDuration: 3600 });
      return;
    }
    const val = parseFloat(valStr);
    if (isNaN(val)) {
      setDurationError('Must be a number');
      updateStoreWithError(true);
    } else if (val <= 0) {
      setDurationError('Must be greater than 0');
      updateStoreWithError(true);
    } else {
      let mult = 1;
      if (unit === 'minutes') mult = 60;
      if (unit === 'hours') mult = 3600;
      const seconds = Math.round(val * mult);
      setDurationError(null);
      updateStoreWithError(false);
      setSplitParams({ partDuration: seconds });
    }
  };

  const handleSizeChange = (valStr: string, unit: 'MB' | 'GB') => {
    setCustomSizeVal(valStr);
    setCustomSizeUnit(unit);
    if (valStr.trim() === '') {
      setSizeError(null);
      updateStoreWithError(false);
      setSplitParams({ maxSizeBytes: 4_000_000_000 });
      return;
    }
    const val = parseFloat(valStr);
    if (isNaN(val)) {
      setSizeError('Must be a number');
      updateStoreWithError(true);
      return;
    }
    const multiplier = unit === 'GB' ? 1024 * 1024 * 1024 : 1024 * 1024;
    const bytes = Math.round(val * multiplier);
    const minBytes = 10 * 1024 * 1024;
    if (bytes < minBytes) {
      setSizeError('Minimum size is 10 MB');
      updateStoreWithError(true);
    } else {
      setSizeError(null);
      updateStoreWithError(false);
      setSplitParams({ maxSizeBytes: bytes });
    }
  };

  const handleHoursChange = (valStr: string) => {
    setCustomHoursVal(valStr);
    if (valStr.trim() === '') {
      setHoursError(null);
      updateStoreWithError(false);
      setSplitParams({ hoursPerUnit: 2 });
      return;
    }
    const val = parseFloat(valStr);
    if (isNaN(val)) {
      setHoursError('Must be a number');
      updateStoreWithError(true);
    } else if (val <= 0) {
      setHoursError('Must be greater than 0');
      updateStoreWithError(true);
    } else {
      setHoursError(null);
      updateStoreWithError(false);
      setSplitParams({ hoursPerUnit: val });
    }
  };

  return (
    <div className="space-y-4">
      <label className="block text-xs font-medium text-text-secondary">
        Split Mode
      </label>

      {/* Mode grid */}
      <div className="grid grid-cols-2 gap-2">
        {SPLIT_MODES_DEFINITIONS.map((mode) => {
          const isActive = splitMode === mode.value;
          return (
            <button
              key={mode.value}
              onClick={() => setSplitMode(mode.value as SplitMode)}
              title={mode.value === 'byParts' ? 'Files are distributed evenly across N parts. Part durations may vary depending on source file lengths.' : undefined}
              className={cn(
                'relative flex items-start gap-2.5 p-3 rounded-lg text-left transition-all duration-150',
                'border focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-500/50',
                isActive
                  ? 'bg-accent-500/5 border-accent-500/30 text-accent-300 shadow-glow-sm'
                  : 'bg-bg-elevated/20 border-border/80 text-text-secondary hover:border-border-strong hover:bg-bg-elevated/40 hover:text-text-primary hover:shadow-glow-sm shadow-sm'
              )}
            >
              {isActive && (
                <motion.div
                  layoutId="split-mode-indicator"
                  className="absolute top-0 left-0 w-full h-full rounded-lg ring-1 ring-accent-500/20 pointer-events-none"
                  transition={{ type: 'spring', stiffness: 400, damping: 30 }}
                />
              )}
              <span className={cn(
                'shrink-0 mt-0.5',
                isActive ? 'text-accent-400' : 'text-text-muted'
              )}>
                {MODE_ICONS[mode.value]}
              </span>
              <div className="min-w-0">
                <div className="text-xs font-semibold leading-tight truncate">
                  {mode.label}
                </div>
                <div className="text-2xs text-text-muted mt-0.5 leading-tight line-clamp-2">
                  {mode.description}
                </div>
              </div>
            </button>
          );
        })}
      </div>

      {/* Mode-specific parameters */}
      <div className="pt-3 border-t border-border/60">
        {splitMode === 'byParts' && (
          <div className="space-y-1.5">
            <label className="block text-xs font-semibold text-text-secondary">
              Number of Parts
            </label>
            <div className="flex flex-wrap gap-1.5">
              {PART_COUNT_PRESETS_SPLIT.map((preset) => (
                <button
                  key={preset.value}
                  onClick={() => {
                    setSplitParams({ partCount: parseInt(preset.value) });
                  }}
                  className={cn(
                    'px-3 py-1.5 rounded-lg text-xs font-medium transition-all duration-150',
                    splitParams.partCount === parseInt(preset.value)
                      ? 'bg-accent-500/10 text-accent-300 border border-accent-500/30 shadow-glow-sm'
                      : 'bg-bg-elevated/30 text-text-secondary border border-border/60 hover:border-border-strong hover:bg-bg-elevated/50 hover:text-text-primary'
                  )}
                >
                  {preset.label}
                </button>
              ))}
            </div>
            {/* Custom Parts Input */}
            <div className="mt-2 space-y-1 bg-bg-elevated/10 p-2 rounded-lg border border-border/40">
              <div className="flex items-center gap-2">
                <span className="text-2xs font-semibold text-text-secondary">Custom:</span>
                <input
                  type="number"
                  min={2}
                  max={1000}
                  value={customPartCount}
                  onChange={(e) => handlePartCountChange(e.target.value)}
                  placeholder="Parts"
                  className={cn(
                    "w-20 px-2 py-1 rounded bg-bg-elevated/40 border text-xs text-text-primary focus:outline-none focus:ring-1 focus:ring-accent-500/50",
                    partCountError ? "border-danger focus:ring-danger/50" : "border-border hover:border-border-strong"
                  )}
                />
                <span className="text-2xs text-text-muted">parts</span>
              </div>
              {partCountError && (
                <p className="text-2xs text-danger font-medium mt-0.5">{partCountError}</p>
              )}
              {!partCountError && splitParams.partCount && inputDuration > 0 && (
                <>
                  {inputDuration / splitParams.partCount < 10 && (
                    <p className="text-2xs text-warning font-medium mt-0.5">
                      ⚠️ Segments will be extremely short (~{(inputDuration / splitParams.partCount).toFixed(1)}s per part)
                    </p>
                  )}
                </>
              )}
            </div>
            {inputDuration > 0 && splitParams.partCount && !partCountError && (
              <p className="text-2xs text-text-muted">
                ~{formatDurationShort(inputDuration / splitParams.partCount)} per part
              </p>
            )}
          </div>
        )}

        {splitMode === 'byDuration' && (
          <div className="space-y-1.5">
            <label className="block text-xs font-semibold text-text-secondary">
              Duration per Part
            </label>
            <div className="flex flex-wrap gap-1.5">
              {SPLIT_PART_DURATION_PRESETS.map((preset) => (
                <button
                  key={preset.value}
                  onClick={() => {
                    setSplitParams({ partDuration: parseInt(preset.value) });
                  }}
                  className={cn(
                    'px-3 py-1.5 rounded-lg text-xs font-medium transition-all duration-150',
                    splitParams.partDuration === parseInt(preset.value)
                      ? 'bg-accent-500/10 text-accent-300 border border-accent-500/30 shadow-glow-sm'
                      : 'bg-bg-elevated/30 text-text-secondary border border-border/60 hover:border-border-strong hover:bg-bg-elevated/50 hover:text-text-primary'
                  )}
                >
                  {preset.label}
                </button>
              ))}
            </div>
            {/* Custom Duration Input */}
            <div className="mt-2 space-y-1 bg-bg-elevated/10 p-2 rounded-lg border border-border/40">
              <div className="flex items-center gap-2">
                <span className="text-2xs font-semibold text-text-secondary">Custom Duration:</span>
                <input
                  type="number"
                  step="any"
                  value={customDurationVal}
                  onChange={(e) => handleDurationChange(e.target.value, customDurationUnit)}
                  placeholder="Value"
                  className={cn(
                    "w-20 px-2 py-1 rounded bg-bg-elevated/40 border text-xs text-text-primary focus:outline-none focus:ring-1 focus:ring-accent-500/50",
                    durationError ? "border-danger focus:ring-danger/50" : "border-border hover:border-border-strong"
                  )}
                />
                <select
                  value={customDurationUnit}
                  onChange={(e) => handleDurationChange(customDurationVal, e.target.value as 'seconds' | 'minutes' | 'hours')}
                  className="px-2 py-1 rounded bg-bg-elevated/60 border border-border text-xs text-text-primary focus:outline-none focus:ring-1 focus:ring-accent-500/50"
                >
                  <option value="seconds">Seconds</option>
                  <option value="minutes">Minutes</option>
                  <option value="hours">Hours</option>
                </select>
              </div>
              {durationError && (
                <p className="text-2xs text-danger font-medium mt-0.5">{durationError}</p>
              )}
              {!durationError && splitParams.partDuration !== undefined && (
                <div className="space-y-0.5 mt-0.5">
                  <p className="text-2xs text-text-muted font-medium">
                    ≈ {splitParams.partDuration} seconds
                  </p>
                  {splitParams.partDuration < 10 && (
                    <p className="text-2xs text-warning font-medium">
                      ⚠️ Segments will be extremely short (~{splitParams.partDuration}s)
                    </p>
                  )}
                  {inputDuration > 0 && splitParams.partDuration > inputDuration && (
                    <p className="text-2xs text-warning font-medium">
                      ⚠️ Duration exceeds source length. Only one segment will be created.
                    </p>
                  )}
                </div>
              )}
            </div>
          </div>
        )}

        {splitMode === 'customRanges' && (
          <div className="space-y-1">
            <label className="block text-xs font-semibold text-text-secondary">
              Custom Time Ranges
            </label>
            <p className="text-2xs text-text-muted">
              Ranges will be defined after generating a plan preview with the current file.
            </p>
          </div>
        )}

        {splitMode === 'byChapters' && (
          <div>
            <p className="text-xs text-text-muted leading-relaxed">
              Click <strong>"Generate Preview"</strong> to detect chapters from the video metadata.
            </p>
          </div>
        )}

        {splitMode === 'byPlaylistItems' && (
          <div className="space-y-1.5">
            <label className="block text-xs font-semibold text-text-secondary">
              Videos per Segment
            </label>
            <div className="flex items-center gap-2">
              <input
                type="number"
                min={1}
                max={100}
                value={splitParams.itemsPerSegment ?? 10}
                onChange={(e) => setSplitParams({ itemsPerSegment: Math.max(1, parseInt(e.target.value) || 1) })}
                className="w-20 px-2.5 py-1.5 rounded-md bg-bg-elevated/40 border border-border hover:border-border-strong text-sm text-text-primary transition-colors focus:outline-none focus:ring-2 focus:ring-accent-500/50"
              />
              <span className="text-xs text-text-muted">videos per output file</span>
            </div>
          </div>
        )}

        {splitMode === 'byOutputSize' && (
          <div className="space-y-1.5">
            <label className="block text-xs font-semibold text-text-secondary">
              Maximum Output Size
            </label>
            <div className="flex flex-wrap gap-1.5">
              {SPLIT_OUTPUT_SIZE_PRESETS.map((preset) => (
                <button
                  key={preset.value}
                  onClick={() => {
                    setSplitParams({ maxSizeBytes: parseInt(preset.value) });
                  }}
                  className={cn(
                    'px-3 py-1.5 rounded-lg text-xs font-medium transition-all duration-150',
                    splitParams.maxSizeBytes === parseInt(preset.value)
                      ? 'bg-accent-500/10 text-accent-300 border border-accent-500/30 shadow-glow-sm'
                      : 'bg-bg-elevated/30 text-text-secondary border border-border/60 hover:border-border-strong hover:bg-bg-elevated/50 hover:text-text-primary'
                  )}
                >
                  {preset.label}
                </button>
              ))}
            </div>
            {/* Custom Output Size Input */}
            <div className="mt-2 space-y-1 bg-bg-elevated/10 p-2 rounded-lg border border-border/40">
              <div className="flex items-center gap-2">
                <span className="text-2xs font-semibold text-text-secondary">Custom Output Size:</span>
                <input
                  type="number"
                  step="any"
                  value={customSizeVal}
                  onChange={(e) => handleSizeChange(e.target.value, customSizeUnit)}
                  placeholder="Value"
                  className={cn(
                    "w-20 px-2 py-1 rounded bg-bg-elevated/40 border text-xs text-text-primary focus:outline-none focus:ring-1 focus:ring-accent-500/50",
                    sizeError ? "border-danger focus:ring-danger/50" : "border-border hover:border-border-strong"
                  )}
                />
                <select
                  value={customSizeUnit}
                  onChange={(e) => handleSizeChange(customSizeVal, e.target.value as 'MB' | 'GB')}
                  className="px-2 py-1 rounded bg-bg-elevated/60 border border-border text-xs text-text-primary focus:outline-none focus:ring-1 focus:ring-accent-500/50"
                >
                  <option value="MB">MB</option>
                  <option value="GB">GB</option>
                </select>
              </div>
              {sizeError && (
                <p className="text-2xs text-danger font-medium mt-0.5">{sizeError}</p>
              )}
              {!sizeError && splitParams.maxSizeBytes !== undefined && (
                <div className="space-y-0.5 mt-0.5">
                  <p className="text-2xs text-text-muted font-medium">
                    ≈ {splitParams.maxSizeBytes.toLocaleString()} bytes
                  </p>
                  {inputSizeBytes > 0 && splitParams.maxSizeBytes > inputSizeBytes && (
                    <p className="text-2xs text-warning font-medium">
                      ⚠️ Requested size exceeds source size. Only one segment will be produced.
                    </p>
                  )}
                </div>
              )}
            </div>
          </div>
        )}

        {splitMode === 'smartCourse' && (
          <div className="space-y-4">
            <div className="space-y-1.5">
              <label className="block text-xs font-semibold text-text-secondary">
                Course Mode
              </label>
              <div className="flex gap-1.5">
                {COURSE_MODE_OPTIONS.map((opt) => (
                  <button
                    key={opt.value}
                    onClick={() => setSplitParams({ courseMode: opt.value as 'daily' | 'weekly' })}
                    className={cn(
                      'px-3 py-1.5 rounded-lg text-xs font-medium transition-all duration-150',
                      splitParams.courseMode === opt.value
                        ? 'bg-accent-500/10 text-accent-300 border border-accent-500/30 shadow-glow-sm'
                        : 'bg-bg-elevated/30 text-text-secondary border border-border/60 hover:border-border-strong hover:bg-bg-elevated/50 hover:text-text-primary'
                    )}
                  >
                    {opt.label}
                  </button>
                ))}
              </div>
            </div>
            <div className="space-y-1.5">
              <label className="block text-xs font-semibold text-text-secondary">
                Hours per {splitParams.courseMode === 'daily' ? 'Day' : 'Week'}
              </label>
              <div className="flex flex-wrap gap-1.5">
                {COURSE_HOURS_PRESETS.map((preset) => (
                  <button
                    key={preset.value}
                    onClick={() => {
                      setSplitParams({ hoursPerUnit: parseInt(preset.value) });
                    }}
                    className={cn(
                      'px-3 py-1.5 rounded-lg text-xs font-medium transition-all duration-150',
                      splitParams.hoursPerUnit === parseInt(preset.value)
                        ? 'bg-accent-500/10 text-accent-300 border border-accent-500/30 shadow-glow-sm'
                        : 'bg-bg-elevated/30 text-text-secondary border border-border/60 hover:border-border-strong hover:bg-bg-elevated/50 hover:text-text-primary'
                    )}
                  >
                    {preset.label}
                  </button>
                ))}
              </div>
              {/* Custom Smart Course Hours Input */}
              <div className="mt-2 space-y-1 bg-bg-elevated/10 p-2 rounded-lg border border-border/40">
                <div className="flex items-center gap-2">
                  <span className="text-2xs font-semibold text-text-secondary">Custom Hours:</span>
                  <input
                    type="number"
                    step="any"
                    value={customHoursVal}
                    onChange={(e) => handleHoursChange(e.target.value)}
                    placeholder="Hours"
                    className={cn(
                      "w-20 px-2 py-1 rounded bg-bg-elevated/40 border text-xs text-text-primary focus:outline-none focus:ring-1 focus:ring-accent-500/50",
                      hoursError ? "border-danger focus:ring-danger/50" : "border-border hover:border-border-strong"
                    )}
                  />
                  <span className="text-2xs text-text-muted">hours</span>
                </div>
                {hoursError && (
                  <p className="text-2xs text-danger font-medium mt-0.5">{hoursError}</p>
                )}
                {!hoursError && splitParams.hoursPerUnit !== undefined && (
                  <div className="space-y-0.5 mt-0.5">
                    <p className="text-2xs text-text-muted font-medium">
                      ≈ {Math.round(splitParams.hoursPerUnit * 3600)} seconds
                    </p>
                    {inputDuration > 0 && (splitParams.hoursPerUnit * 3600) > inputDuration && (
                      <p className="text-2xs text-warning font-medium">
                        ⚠️ Course unit duration exceeds source length. Only one segment will be created.
                      </p>
                    )}
                  </div>
                )}
              </div>
            </div>
          </div>
        )}
      </div>

      {/* Subtitle Options */}
      <div className="pt-3 border-t border-border/60 space-y-3">
        <div className="flex items-center gap-2">
          <Subtitles className="w-4 h-4 text-text-muted" />
          <label className="block text-xs font-medium text-text-secondary">
            Subtitle Options
          </label>
        </div>

        <div className="grid grid-cols-3 gap-2">
          <button
            onClick={() => setSplitParams({ subtitleMode: 'copyAll' })}
            title="Keep subtitle streams embedded in each output segment (stream copy). No separate .srt files are generated."
            className={cn(
              'px-3 py-2 rounded-lg text-xs font-medium transition-all duration-150 border',
              (splitParams.subtitleMode === 'copyAll' || !splitParams.subtitleMode)
                ? 'bg-accent-500/10 text-accent-300 border-accent-500/30'
                : 'bg-bg-elevated/30 text-text-secondary border-border/60 hover:border-border-strong hover:bg-bg-elevated/50 hover:text-text-primary'
            )}
          >
            Copy All
          </button>
          <button
            onClick={() => setSplitParams({ subtitleMode: 'extractSplit' })}
            title="Extract embedded subtitles, split them per segment, and export as separate .srt files alongside each video part."
            className={cn(
              'px-3 py-2 rounded-lg text-xs font-medium transition-all duration-150 border',
              splitParams.subtitleMode === 'extractSplit'
                ? 'bg-accent-500/10 text-accent-300 border-accent-500/30'
                : 'bg-bg-elevated/30 text-text-secondary border-border/60 hover:border-border-strong hover:bg-bg-elevated/50 hover:text-text-primary'
            )}
          >
            Extract & Split
          </button>
          <button
            onClick={() => setSplitParams({ subtitleMode: 'ignore' })}
            title="Skip all subtitle processing — output segments have no subtitle tracks and no .srt files are produced."
            className={cn(
              'px-3 py-2 rounded-lg text-xs font-medium transition-all duration-150 border',
              splitParams.subtitleMode === 'ignore'
                ? 'bg-accent-500/10 text-accent-300 border-accent-500/30'
                : 'bg-bg-elevated/30 text-text-secondary border-border/60 hover:border-border-strong hover:bg-bg-elevated/50 hover:text-text-primary'
            )}
          >
            Skip
          </button>
        </div>

        {/* Export SRT toggle - only shown when extractSplit is selected */}
        {splitParams.subtitleMode === 'extractSplit' && (
          <div className="flex items-center justify-between px-3 py-2 rounded-lg bg-bg-elevated/30 border border-border/40">
            <span className="text-xs text-text-muted">Export SRT files per segment</span>
            <button
              onClick={() => setSplitParams({ exportSrt: !splitParams.exportSrt })}
              className={cn(
                'relative w-10 h-5 rounded-full transition-colors duration-200',
                splitParams.exportSrt ? 'bg-accent-500' : 'bg-border-strong'
              )}
            >
              <span
                className={cn(
                  'absolute top-0.5 w-4 h-4 rounded-full bg-white transition-transform duration-200',
                  splitParams.exportSrt ? 'left-5 translate-x-0' : 'left-0.5'
                )}
              />
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

function formatDurationShort(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (h > 0 && m > 0) return `${h}h ${m}m`;
  if (h > 0) return `${h}h`;
  return `${m}m`;
}
