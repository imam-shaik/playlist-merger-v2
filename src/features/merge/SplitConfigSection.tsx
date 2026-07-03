// ────────────────────────────────────────────────
// SplitConfigSection — Split output by part count
// or duration, with visual preview of output names.
// ────────────────────────────────────────────────

import React, { useState, useEffect, useCallback } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { SplitSquareHorizontal, ChevronDown, ChevronUp } from 'lucide-react';
import { cn } from '@/utils/cn';
import { Badge } from '@/components/ui/Badge';
import { useMergeStore } from '@/store/mergeStore';
import { useWorkspaceStore } from '@/store/workspaceStore';
import { formatDuration, totalDuration } from '@/utils';
import { usePlaylistStore } from '@/store/playlistStore';
import { SPLIT_MODES, PART_COUNT_PRESETS, DURATION_PRESETS, FOLDER_OUTPUT_MODES, MAX_OUTPUT_PARTS } from '@/constants';
import { tauriCommands } from '@/tauri/commands';
import type { NamingValidation } from '@/types';

export function SplitConfigSection() {
  const splitConfig = useMergeStore((s) => s.splitConfig);
  const namingConfig = useMergeStore((s) => s.namingConfig);
  const outputFilename = useMergeStore((s) => s.outputFilename);
  const mergeMode = useMergeStore((s) => s.mergeMode);
  const convertToMp4 = useMergeStore((s) => s.convertToMp4);
  const entries = usePlaylistStore((s) => s.entries);
  const totalDur = totalDuration(entries);
  const splitExpanded = useWorkspaceStore((s) => s.sectionsCollapsed.split);
  const toggleSection = useWorkspaceStore((s) => s.toggleSection);

  // Get setter imperatively (stable reference)
  const setSplitConfig = useMergeStore((s) => s.setSplitConfig);

  // Derive correct output extension from merge mode
  const isMkvMode = mergeMode === 'fastMkv' || mergeMode === 'smartMkv';
  const outputExt = isMkvMode && !convertToMp4 ? 'mkv' : 'mp4';

  const [selectedPartCount, setSelectedPartCount] = useState('3');
  const [customPartCount, setCustomPartCount] = useState('');
  const [partCountError, setPartCountError] = useState<string | null>(null);

  const [selectedDuration, setSelectedDuration] = useState('3600');
  const [customDuration, setCustomDuration] = useState('');

  // Sync split config with store when settings change
  useEffect(() => {
    const splitMode = splitConfig.mode;
    if (splitMode === 'count') {
      const pCount = customPartCount ? parseInt(customPartCount, 10) : parseInt(selectedPartCount, 10);
      const validCount = isNaN(pCount) || pCount < 2 || pCount > 1000 ? 3 : pCount;
      setSplitConfig({
        mode: 'count',
        partCount: validCount,
      });
    } else if (splitMode === 'duration') {
      const durationSecs = customDuration ? parseInt(customDuration, 10) : parseInt(selectedDuration, 10);
      const validDuration = isNaN(durationSecs) || durationSecs <= 0 ? 3600 : durationSecs;
      setSplitConfig({
        mode: 'duration',
        maxDurationPerPart: validDuration,
      });
    } else if (splitMode === 'folder') {
      setSplitConfig({
        mode: 'folder',
      });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedPartCount, customPartCount, selectedDuration, customDuration, splitConfig.mode]);

  const handleSplitModeChange = (mode: 'none' | 'count' | 'duration' | 'folder') => {
    // Preserve existing subtitleMode when switching modes
    const currentSubMode = splitConfig.subtitleMode;
    if (mode === 'none') {
      setSplitConfig({ mode: 'none', subtitleMode: currentSubMode });
    } else if (mode === 'count') {
      const pCount = customPartCount ? parseInt(customPartCount, 10) : parseInt(selectedPartCount, 10);
      const validCount = isNaN(pCount) || pCount < 2 || pCount > 1000 ? 3 : pCount;
      setSplitConfig({
        mode: 'count',
        partCount: validCount,
        subtitleMode: currentSubMode,
      });
    } else if (mode === 'duration') {
      const durationSecs = customDuration ? parseInt(customDuration, 10) : parseInt(selectedDuration, 10);
      const validDuration = isNaN(durationSecs) || durationSecs <= 0 ? 3600 : durationSecs;
      setSplitConfig({
        mode: 'duration',
        maxDurationPerPart: validDuration,
        subtitleMode: currentSubMode,
      });
} else if (mode === 'folder') {
      setSplitConfig({
        mode: 'folder',
        subtitleMode: currentSubMode,
        folderSplitMode: splitConfig.folderSplitMode || 'single',
      });
    }
  };

  const handleFolderSplitModeChange = (newMode: 'single' | 'parts') => {
    const pCount = customPartCount ? parseInt(customPartCount, 10) : parseInt(selectedPartCount, 10);
    const validCount = isNaN(pCount) || pCount < 2 || pCount > 1000 ? 3 : pCount;
    setSplitConfig({
      ...splitConfig,
      folderSplitMode: newMode,
      partCount: newMode === 'parts' ? validCount : undefined,
    });
  };

  // Helper to extract top-level subfolders for preview
  const getFolderGroupsPreview = () => {
    const groups: string[] = [];
    let currentGroup: string | null = null;
    
    // Find common parent path
    const paths = entries.map(e => e.path);
    if (paths.length === 0) return [];
    
    const findCommonParent = (allPaths: string[]) => {
      const first = allPaths[0].replace(/\\/g, '/');
      const parts = first.split('/');
      let commonParts = parts.slice(0, -1);
      
      for (let i = 1; i < allPaths.length; i++) {
        const current = allPaths[i].replace(/\\/g, '/').split('/');
        const tempCommon: string[] = [];
        for (let j = 0; j < Math.min(commonParts.length, current.length - 1); j++) {
          if (commonParts[j] === current[j]) {
            tempCommon.push(commonParts[j]);
          } else {
            break;
          }
        }
        commonParts = tempCommon;
      }
      return commonParts.join('/');
    };

    const commonParent = findCommonParent(paths);

    entries.forEach((entry) => {
      const pathNorm = entry.path.replace(/\\/g, '/');
      const relPath = commonParent && pathNorm.startsWith(commonParent)
        ? pathNorm.substring(commonParent.length).replace(/^\//, '')
        : entry.relativePath?.replace(/\\/g, '/').replace(/^\//, '') || '';
      
      const parts = relPath.split('/');
      const folderName = parts.length > 1 ? parts[0] : 'Root';

      if (folderName !== currentGroup) {
        currentGroup = folderName;
        groups.push(folderName);
      }
    });

    return groups;
  };

  // Helper to extract top-level subfolders with video counts for folder parts mode
  const getFolderGroupsWithCounts = (): { name: string; count: number }[] => {
    const groups: { name: string; count: number }[] = [];
    let currentGroup: string | null = null;
    let currentCount = 0;

    // Find common parent path
    const paths = entries.map(e => e.path);
    if (paths.length === 0) return [];

    const findCommonParent = (allPaths: string[]) => {
      const first = allPaths[0].replace(/\\/g, '/');
      const parts = first.split('/');
      let commonParts = parts.slice(0, -1);

      for (let i = 1; i < allPaths.length; i++) {
        const current = allPaths[i].replace(/\\/g, '/').split('/');
        const tempCommon: string[] = [];
        for (let j = 0; j < Math.min(commonParts.length, current.length - 1); j++) {
          if (commonParts[j] === current[j]) {
            tempCommon.push(commonParts[j]);
          } else {
            break;
          }
        }
        commonParts = tempCommon;
      }
      return commonParts.join('/');
    };

    const commonParent = findCommonParent(paths);

    entries.forEach((entry) => {
      const pathNorm = entry.path.replace(/\\/g, '/');
      const relPath = commonParent && pathNorm.startsWith(commonParent)
        ? pathNorm.substring(commonParent.length).replace(/^\//, '')
        : entry.relativePath?.replace(/\\/g, '/').replace(/^\//, '') || '';

      const parts = relPath.split('/');
      const folderName = parts.length > 1 ? parts[0] : 'Root';

      if (folderName !== currentGroup) {
        if (currentGroup !== null) {
          groups.push({ name: currentGroup, count: currentCount });
        }
        currentGroup = folderName;
        currentCount = 1;
      } else {
        currentCount++;
      }
    });

    if (currentGroup !== null) {
      groups.push({ name: currentGroup, count: currentCount });
    }

    return groups;
  };

  // ── Naming preview ──────────────────────────────────────────────────────
  // Uses the same validateNamingTemplate engine as MergeNamingSection,
  // ensuring preview filenames match actual backend output exactly.
  const [splitNamingPreview, setSplitNamingPreview] = useState<NamingValidation | null>(null);

  const updateSplitNaming = useCallback(async () => {
    const baseName = outputFilename || 'merged_output';
    if (splitConfig.mode === 'none') {
      setSplitNamingPreview(null);
      return;
    }

    const now = new Date();
    const dateStr = now.toISOString().split('T')[0];
    const timeStr = `${String(now.getHours()).padStart(2, '0')}-${String(now.getMinutes()).padStart(2, '0')}-${String(now.getSeconds()).padStart(2, '0')}`;

    // Compute part segments matching backend compute_part_boundaries logic
    const segments: Array<{
      filename: string;
      extension: string;
      index: number;
      startTime: number;
      endTime: number;
      duration: number;
      folder?: string;
      partLabel?: string;
    }> = [];

    if (splitConfig.mode === 'count' && splitConfig.partCount && splitConfig.partCount > 0) {
      const requested = splitConfig.partCount;
      const partCount = Math.min(requested, entries.length);
      const filesPerPart = Math.ceil(entries.length / partCount);
      for (let i = 0; i < partCount; i++) {
        const startIdx = i * filesPerPart;
        const endIdx = Math.min((i + 1) * filesPerPart, entries.length);
        const partDuration = entries
          .slice(startIdx, endIdx)
          .reduce((sum, e) => sum + (e.mediaInfo?.duration ?? 0), 0);
        const partStart = entries
          .slice(0, startIdx)
          .reduce((sum, e) => sum + (e.mediaInfo?.duration ?? 0), 0);
        segments.push({
          filename: baseName,
          extension: outputExt,
          index: i + 1,
          startTime: partStart,
          endTime: partStart + partDuration,
          duration: partDuration,
          partLabel: `Part_${i + 1}`,
        });
      }
    } else if (splitConfig.mode === 'duration') {
      const maxDur = splitConfig.maxDurationPerPart ?? 3600;
      const partCount = Math.ceil(totalDur / maxDur);
      for (let i = 0; i < partCount; i++) {
        const partStart = i * maxDur;
        const partEnd = Math.min((i + 1) * maxDur, totalDur);
        segments.push({
          filename: baseName,
          extension: outputExt,
          index: i + 1,
          startTime: partStart,
          endTime: partEnd,
          duration: partEnd - partStart,
          partLabel: `Part_${i + 1}`,
        });
      }
    } else if (splitConfig.mode === 'folder') {
      const groups = getFolderGroupsPreview();
      groups.forEach((folder, i) => {
        // Use folder name as default filename for Folder mode (matches backend behavior)
        const folderFilename = folder.replace(/ /g, '_');
        segments.push({
          filename: folderFilename,
          extension: outputExt,
          index: i + 1,
          startTime: 0,
          endTime: 0,
          duration: 0,
          folder: folderFilename,
          partLabel: `Part_${i + 1}`,
        });
      });
    }

    if (segments.length === 0) {
      setSplitNamingPreview(null);
      return;
    }

    try {
      const result = await tauriCommands.validateNamingTemplate(
        namingConfig.template,
        namingConfig,
        segments.map((s) => ({
          filename: s.filename,
          extension: s.extension,
          index: s.index,
          startTime: s.startTime,
          endTime: s.endTime,
          duration: s.duration,
          date: dateStr,
          time: timeStr,
          folder: s.folder,
          partLabel: s.partLabel,
          prefix: namingConfig.prefix,
          suffix: namingConfig.suffix,
          chapter: undefined,
          resolution: undefined,
          width: undefined,
          height: undefined,
          originalNum: undefined,
          playlist: undefined,
          playlistIndex: undefined,
          videoCount: undefined,
          totalDuration: undefined,
          lang: undefined,
          langName: undefined,
        })),
        Math.min(segments.length, 10),
      );
      setSplitNamingPreview(result);
    } catch {
      setSplitNamingPreview(null);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [splitConfig, namingConfig, outputFilename, entries, totalDur]);

  // Recompute naming preview whenever split config or naming config changes
  useEffect(() => {
    const timer = setTimeout(updateSplitNaming, 150);
    return () => clearTimeout(timer);
  }, [updateSplitNaming]);

  // Get resolved filenames from naming validation (batched preview)
  const resolvedNames = splitNamingPreview?.batchPreview ?? [];

  // Fallback: generate default names if validation hasn't completed or failed
  const getPreviewName = (index: number, folderSuffix?: string): string => {
    if (index < resolvedNames.length && resolvedNames[index]) {
      return resolvedNames[index];
    }
    const ext = `.${outputExt}`;
    const base = outputFilename || 'merged_output';
    if (folderSuffix) {
      return `${base}_part${index + 1}_${folderSuffix}${ext}`;
    }
    return `${base}_part${index + 1}${ext}`;
  };

  return (
    <div className="bg-bg-elevated border border-border rounded-xl p-4">
      <button
        onClick={() => toggleSection('split')}
        className="flex items-center justify-between w-full"
        aria-expanded={splitExpanded}
        aria-label="Toggle split output settings"
      >
        <div className="flex items-center gap-2">
          <SplitSquareHorizontal className="w-4 h-4 text-text-muted" />
          <p className="text-xs font-semibold text-text-secondary">Split Output</p>
          {splitConfig.mode !== 'none' && (
            <Badge variant="info">
              {splitConfig.mode === 'count'
                ? `${splitConfig.partCount} parts`
                : splitConfig.mode === 'folder'
                ? 'Split by Folder'
                : `${formatDuration(splitConfig.maxDurationPerPart ?? 0)} per part`}
            </Badge>
          )}
        </div>
        {splitExpanded ? (
          <ChevronUp className="w-4 h-4 text-text-muted" />
        ) : (
          <ChevronDown className="w-4 h-4 text-text-muted" />
        )}
      </button>
 
      <AnimatePresence>
        {splitExpanded && (
          <motion.div
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: 'auto' }}
            exit={{ opacity: 0, height: 0 }}
            className="overflow-hidden"
          >
            <div className="mt-3 space-y-3">
              {/* Split mode selection */}
              <div>
                <p className="text-2xs font-medium text-text-muted mb-1.5">Split Mode</p>
                <div className="flex gap-2">
                  {SPLIT_MODES.map((mode) => (
                    <button
                      key={mode.value}
                      onClick={() => handleSplitModeChange(mode.value as 'none' | 'count' | 'duration' | 'folder')}
                      title={
                        mode.value === 'count'
                          ? 'Files are distributed evenly across N parts. Part durations may vary depending on source file lengths.'
                          : mode.value === 'duration'
                          ? 'Each part is limited to a fixed maximum duration. The number of parts is calculated from your total playlist length.'
                          : mode.value === 'folder'
                          ? 'Each top-level subfolder becomes one output part.'
                          : undefined
                      }
                      className={cn(
                        'flex-1 px-3 py-2 rounded-lg border text-xs font-medium transition-all',
                        splitConfig.mode === mode.value
                          ? 'bg-accent-muted border-accent-500/40 text-accent-400'
                          : 'bg-bg-overlay border-border hover:border-border-strong text-text-secondary'
                      )}
                      aria-pressed={splitConfig.mode === mode.value}
                    >
                      {mode.label}
                    </button>
                  ))}
                </div>
              </div>

              {/* Folder Output Mode selector */}
              {splitConfig.mode === 'folder' && (
                <div className="border-t border-border/60 pt-3 mt-3">
                  <p className="text-2xs font-medium text-text-muted mb-1.5">Group Output</p>
                  <div className="flex gap-2">
                    {FOLDER_OUTPUT_MODES.map((opt) => (
                      <button
                        key={opt.value}
                        onClick={() => handleFolderSplitModeChange(opt.value as 'single' | 'parts')}
                        className={cn(
                          'flex-1 px-3 py-2 rounded-lg border text-xs font-medium transition-all',
                          (splitConfig.folderSplitMode || 'single') === opt.value
                            ? 'bg-accent-muted border-accent-500/40 text-accent-400'
                            : 'bg-bg-overlay border-border hover:border-border-strong text-text-secondary'
                        )}
                        aria-pressed={(splitConfig.folderSplitMode || 'single') === opt.value}
                      >
                        {opt.label}
                      </button>
                    ))}
                  </div>
                </div>
              )}

              {/* Part count selector for folder with parts */}
              {splitConfig.mode === 'folder' && splitConfig.folderSplitMode === 'parts' && (
                <div>
                  <p className="text-2xs font-medium text-text-muted mb-1.5">Parts Per Folder</p>
                  <div className="flex flex-wrap gap-2">
                    {PART_COUNT_PRESETS.map((preset) => (
                      <button
                        key={preset.value}
                        onClick={() => {
                          setSelectedPartCount(preset.value);
                          setCustomPartCount('');
                          setPartCountError(null);
                          const pCount = parseInt(preset.value, 10);
                          setSplitConfig({
                            ...splitConfig,
                            folderSplitMode: 'parts',
                            partCount: pCount,
                          });
                        }}
                        className={cn(
                          'px-3 py-1.5 rounded-lg border text-xs transition-all',
                          selectedPartCount === preset.value && !customPartCount
                            ? 'bg-accent-muted border-accent-500/40 text-accent-400'
                            : 'bg-bg-overlay border-border hover:border-border-strong text-text-secondary'
                        )}
                        aria-pressed={selectedPartCount === preset.value && !customPartCount}
                      >
                        {preset.label}
                      </button>
                    ))}
                  </div>
                  <div className="mt-2 flex flex-col gap-1">
                    <div className="flex items-center gap-2">
                      <span className="text-2xs text-text-muted">Custom:</span>
                      <input
                        type="number"
                        min={2}
                        max={1000}
                        value={customPartCount}
                        onChange={(e) => {
                          const valStr = e.target.value;
                          setCustomPartCount(valStr);
                          if (valStr.trim() === '') {
                            setPartCountError(null);
                            return;
                          }
                          const val = parseInt(valStr, 10);
                          if (isNaN(val)) {
                            setPartCountError('Must be a number');
                          } else if (val < 2 || val > 1000) {
                            setPartCountError('Must be between 2 and 1000');
                          } else {
                            setPartCountError(null);
                            setSplitConfig({
                              ...splitConfig,
                              folderSplitMode: 'parts',
                              partCount: val,
                            });
                          }
                        }}
                        placeholder="Parts"
                        className={cn(
                          'w-24 h-7 px-2 text-xs bg-bg-overlay border rounded',
                          'text-text-primary focus:outline-none focus:ring-1 focus:ring-accent-500/40',
                          partCountError ? 'border-danger focus:border-danger/60' : 'border-border focus:border-accent-500/60'
                        )}
                        aria-label="Custom number of parts per folder"
                      />
                      <span className="text-2xs text-text-muted">parts</span>
                    </div>
                    {partCountError && (
                      <p className="text-2xs text-danger font-medium mt-0.5">{partCountError}</p>
                    )}
                  </div>
                </div>
              )}

              {/* Subtitle mode selector — per-part handling (overrides global subtitle mode above) */}
              {splitConfig.mode !== 'none' && (
                <div className="border-t border-border/60 pt-3 mt-3">
                  <p className="text-2xs font-medium text-text-muted mb-1.5">Per-Part Subtitles</p>
                  <div className="flex gap-2">
                    <button
                      onClick={() => setSplitConfig({ ...splitConfig, subtitleMode: 'embed' })}
                      className={cn(
                        'flex-1 px-3 py-2 rounded-lg border text-xs font-medium transition-all',
                        (splitConfig.subtitleMode === 'embed' || !splitConfig.subtitleMode)
                          ? 'bg-accent-muted border-accent-500/40 text-accent-400'
                          : 'bg-bg-overlay border-border hover:border-border-strong text-text-secondary'
                      )}
                      aria-pressed={splitConfig.subtitleMode === 'embed' || !splitConfig.subtitleMode}
                      title="Embed subtitles into each split part's video output. Overrides the global subtitle handling mode above."
                    >
                      Embed Per Part
                    </button>
                    <button
                      onClick={() => setSplitConfig({ ...splitConfig, subtitleMode: 'exportSrt' })}
                      className={cn(
                        'flex-1 px-3 py-2 rounded-lg border text-xs font-medium transition-all',
                        splitConfig.subtitleMode === 'exportSrt'
                          ? 'bg-accent-muted border-accent-500/40 text-accent-400'
                          : 'bg-bg-overlay border-border hover:border-border-strong text-text-secondary'
                      )}
                      aria-pressed={splitConfig.subtitleMode === 'exportSrt'}
                      title="Each split part gets its own .srt subtitle file alongside the video."
                    >
                      Export .srt Per Part
                    </button>
                    <button
                      onClick={() => setSplitConfig({ ...splitConfig, subtitleMode: 'ignore' })}
                      className={cn(
                        'flex-1 px-3 py-2 rounded-lg border text-xs font-medium transition-all',
                        splitConfig.subtitleMode === 'ignore'
                          ? 'bg-accent-muted border-accent-500/40 text-accent-400'
                          : 'bg-bg-overlay border-border hover:border-border-strong text-text-secondary'
                      )}
                      aria-pressed={splitConfig.subtitleMode === 'ignore'}
                      title="Skip subtitles in the split output entirely. No subtitle tracks or SRT files are produced."
                    >
                      Skip
                    </button>
                  </div>
                  <p className="text-[9px] text-text-muted leading-tight mt-1.5">
                    Controls how subtitles are handled per split part (overrides the global Subtitle mode above).
                  </p>
                </div>
              )}
 
              {/* Part count selector */}
              {splitConfig.mode === 'count' && (
                <div>
                  <p className="text-2xs font-medium text-text-muted mb-1.5">Number of Parts</p>
                  <div className="flex flex-wrap gap-2">
                    {PART_COUNT_PRESETS.map((preset) => (
                      <button
                        key={preset.value}
                        onClick={() => {
                          setSelectedPartCount(preset.value);
                          setCustomPartCount('');
                          setPartCountError(null);
                        }}
                        className={cn(
                          'px-3 py-1.5 rounded-lg border text-xs transition-all',
                          selectedPartCount === preset.value && !customPartCount
                            ? 'bg-accent-muted border-accent-500/40 text-accent-400'
                            : 'bg-bg-overlay border-border hover:border-border-strong text-text-secondary'
                        )}
                        aria-pressed={selectedPartCount === preset.value && !customPartCount}
                      >
                        {preset.label}
                      </button>
                    ))}
                  </div>
                  <div className="mt-2 flex flex-col gap-1">
                    <div className="flex items-center gap-2">
                      <span className="text-2xs text-text-muted">Custom:</span>
                      <input
                        type="number"
                        min={2}
                        max={1000}
                        value={customPartCount}
                        onChange={(e) => {
                          const valStr = e.target.value;
                          setCustomPartCount(valStr);
                          if (valStr.trim() === '') {
                            setPartCountError(null);
                            return;
                          }
                          const val = parseInt(valStr, 10);
                          if (isNaN(val)) {
                            setPartCountError('Must be a number');
                          } else if (val < 2 || val > 1000) {
                            setPartCountError('Must be between 2 and 1000');
                          } else if (val.toString() !== valStr.trim()) {
                            setPartCountError('Must be a whole number');
                          } else {
                            setPartCountError(null);
                          }
                        }}
                        placeholder="Parts"
                        className={cn(
                          'w-24 h-7 px-2 text-xs bg-bg-overlay border rounded',
                          'text-text-primary focus:outline-none focus:ring-1 focus:ring-accent-500/40',
                          partCountError ? 'border-danger focus:border-danger/60' : 'border-border focus:border-accent-500/60'
                        )}
                        aria-label="Custom number of parts"
                      />
                      <span className="text-2xs text-text-muted">parts</span>
                    </div>
                    {partCountError && (
                      <p className="text-2xs text-danger font-medium mt-0.5">{partCountError}</p>
                    )}
                    {!partCountError && customPartCount && totalDur > 0 && (
                      <>
                        {totalDur / (parseInt(customPartCount, 10) || 1) < 10 && (
                          <p className="text-2xs text-warning font-medium">
                            ⚠️ Segments will be extremely short (~{(totalDur / parseInt(customPartCount, 10)).toFixed(1)}s per part)
                          </p>
                        )}
                      </>
                    )}
                  </div>
                </div>
              )}
 
              {/* Duration selector */}
              {splitConfig.mode === 'duration' && (
                <div>
                  <p className="text-2xs font-medium text-text-muted mb-1.5">Max Duration Per Part</p>
                  <div className="flex flex-wrap gap-2">
                    {DURATION_PRESETS.map((preset) => (
                      <button
                        key={preset.value}
                        onClick={() => {
                          setSelectedDuration(preset.value);
                          setCustomDuration('');
                        }}
                        className={cn(
                          'px-3 py-1.5 rounded-lg border text-xs transition-all',
                          selectedDuration === preset.value && !customDuration
                            ? 'bg-accent-muted border-accent-500/40 text-accent-400'
                            : 'bg-bg-overlay border-border hover:border-border-strong text-text-secondary'
                        )}
                        aria-pressed={selectedDuration === preset.value && !customDuration}
                      >
                        {preset.label}
                      </button>
                    ))}
                  </div>
                  <div className="mt-2 flex items-center gap-2">
                    <span className="text-2xs text-text-muted">Custom:</span>
                    <input
                      type="number"
                      value={customDuration}
                      onChange={(e) => setCustomDuration(e.target.value)}
                      placeholder="Seconds"
                      className={cn(
                        'w-24 h-7 px-2 text-xs bg-bg-overlay border rounded',
                        'text-text-primary focus:outline-none focus:ring-1 focus:ring-accent-500/40',
                        'border-border focus:border-accent-500/60'
                      )}
                      aria-label="Custom duration in seconds"
                    />
                    <span className="text-2xs text-text-muted">seconds</span>
                  </div>
                </div>
              )}
 
              {/* Split preview */}
              {splitConfig.mode !== 'none' && totalDur > 0 && (
                <div className="bg-bg-overlay rounded-lg p-3 border border-border">
                  <p className="text-2xs text-text-muted mb-2">Output files will be named:</p>
                  <div className="space-y-1">
                    {splitConfig.mode === 'count' && splitConfig.partCount && splitConfig.partCount > 0 && entries.length > 0 && (
                      <>
                        {(() => {
                          const requested = splitConfig.partCount ?? 0;
                          const partCount = Math.min(requested, entries.length);
                          const filesPerPart = Math.ceil(entries.length / partCount);
                          const displayCount = Math.min(partCount, 10);
                          const items = [];
                          for (let i = 0; i < displayCount; i++) {
                            const startIdx = i * filesPerPart;
                            const endIdx = Math.min((i + 1) * filesPerPart, entries.length);
                            const fileRange = startIdx === endIdx - 1
                              ? `File ${startIdx + 1}`
                              : `Files ${startIdx + 1}–${endIdx}`;
                            const partDuration = entries
                              .slice(startIdx, endIdx)
                              .reduce((sum, e) => sum + (e.mediaInfo?.duration ?? 0), 0);
                            const durationLabel = partDuration > 0
                              ? `~${formatDurationShort(partDuration)}`
                              : 'duration unknown';
                            items.push(
                              <div key={i} className="flex flex-col gap-0.5">
                                <p className="text-xs text-text-secondary font-mono">
                                  {getPreviewName(i)}
                                </p>
                                <p className="text-2xs text-text-muted pl-2">
                                  {fileRange} · {durationLabel}
                                </p>
                              </div>
                            );
                          }
                          return items;
                        })()}
                        {splitConfig.partCount > 10 && (
                          <p className="text-2xs text-text-muted">
                            ...and {splitConfig.partCount - 10} more parts
                          </p>
                        )}
                        {splitConfig.partCount > entries.length && (
                          <p className="text-2xs text-warning mt-1">
                            ⚠️ {splitConfig.partCount} parts requested but only {entries.length} files available — some parts will be empty
                          </p>
                        )}
                      </>
                    )}
                    {splitConfig.mode === 'duration' && (
                      <>
                        {(() => {
                          const maxDur = customDuration ? parseInt(customDuration) : parseInt(selectedDuration);
                          if (maxDur <= 0) return null;
                          const partCount = Math.ceil(totalDur / maxDur);
                          return (
                            <>
                              {Array.from({ length: Math.min(partCount, 10) }, (_, i) => (
                                <p key={i} className="text-xs text-text-secondary font-mono">
                                  {getPreviewName(i)}
                                </p>
                              ))}
                              {partCount > 10 && (
                                <p className="text-2xs text-text-muted">
                                  ...and {partCount - 10} more parts
                                </p>
                              )}
                              <p className="text-2xs text-text-muted mt-1">
                                Estimated {partCount} parts based on {formatDuration(totalDur)} total duration
                              </p>
                            </>
                          );
                        })()}
                      </>
                    )}
{splitConfig.mode === 'folder' && (
                      <>
                        {(() => {
                          const groupsWithCounts = getFolderGroupsWithCounts();
                          const requestedParts = splitConfig.partCount || 2;
                          const folderPartCounts = groupsWithCounts.map(g => Math.min(requestedParts, g.count));
                          const totalOutputs = folderPartCounts.reduce((a, b) => a + b, 0);
                          const exceedsLimit = totalOutputs > MAX_OUTPUT_PARTS;
                          const displayGroups = groupsWithCounts.slice(0, 10);
                          const displayParts = folderPartCounts.slice(0, 10);

                          if (splitConfig.folderSplitMode === 'parts') {
                            // Show per-folder parts preview
                            return (
                              <>
                                {exceedsLimit && (
                                  <p className="text-2xs text-danger font-medium mt-1">
                                    ⚠️ This will create {totalOutputs} output files (max: {MAX_OUTPUT_PARTS})
                                  </p>
                                )}
                                {displayGroups.map((folder, i) => {
                                  const parts = displayParts[i];
                                  const displayPartCount = Math.min(parts, 4);
                                  const folderItems = [];
                                  for (let p = 1; p <= displayPartCount; p++) {
                                    folderItems.push(
                                      <p key={p} className="text-xs text-text-secondary font-mono pl-2">
                                        {folder.name.replace(/ /g, '_')}_part{p}.mkv
                                      </p>
                                    );
                                  }
                                  if (parts > 4) {
                                    folderItems.push(
                                      <p key="more" className="text-2xs text-text-muted pl-2">
                                        ...and {parts - 4} more
                                      </p>
                                    );
                                  }
                                  return (
                                    <div key={i} className="mb-1">
                                      <p className="text-2xs font-medium text-text-secondary">
                                        {folder.name} ({folder.count} videos → {parts} parts)
                                      </p>
                                      {folderItems}
                                    </div>
                                  );
                                })}
                                {groupsWithCounts.length > 10 && (
                                  <p className="text-2xs text-text-muted">
                                    ...and {groupsWithCounts.length - 10} more folders
                                  </p>
                                )}
                                <p className="text-2xs text-text-muted mt-1">
                                  Total outputs: {totalOutputs}
                                </p>
                              </>
                            );
                          }

                          // Single mode (existing behavior)
                          return (
                            <>
                              {groupsWithCounts.slice(0, 10).map((folder, i) => (
                                <p key={i} className="text-xs text-text-secondary font-mono">
                                  {getPreviewName(i, folder.name.replace(/ /g, '_'))}
                                </p>
                              ))}
                              {groupsWithCounts.length > 10 && (
                                <p className="text-2xs text-text-muted">
                                  ...and {groupsWithCounts.length - 10} more parts
                                </p>
                              )}
                              <p className="text-2xs text-text-muted mt-1 font-sans">
                                Detected {groupsWithCounts.length} subfolder group{groupsWithCounts.length > 1 ? 's' : ''} based on import paths
                              </p>
                            </>
                          );
                        })()}
                      </>
                    )}
                  </div>
                </div>
              )}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
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
