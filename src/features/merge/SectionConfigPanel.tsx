import React, { useState, useEffect, useCallback } from 'react';
import { motion } from 'framer-motion';
import { 
  Layers, ChevronDown, ChevronUp, FolderOpen, 
  Clock, HardDrive, SplitSquareHorizontal, Play
} from 'lucide-react';
import { cn } from '@/utils/cn';
import { Button } from '@/components/ui/Button';
import { Input } from '@/components/ui/Input';
import { Badge } from '@/components/ui/Badge';
import { useSectionStore } from '@/store/sectionStore';
import { usePlaylistStore } from '@/store/playlistStore';
import { useMergeStore } from '@/store/mergeStore';
import { formatDuration } from '@/utils';
import { sectionCommands } from '@/tauri/commands';
import type { FolderForMerge, PartitionMethod } from '@/types';

interface SectionConfigPanelProps {
  onStartMerge: (config: SectionMergeConfig) => void;
  onCancel: () => void;
}

export interface SectionMergeConfig {
  method: PartitionMethod;
  nameTemplate: string;
  outputSubfolder: string;
  outputBaseDir: string;
  baseMergeMode: string;
  normalizeAudio: boolean;
}

export function SectionConfigPanel({ onStartMerge, onCancel }: SectionConfigPanelProps) {
  const {
    methodType,
    sectionCount,
    maxDurationSecs,
    maxSizeBytes,
    nameTemplate,
    sectionPlans,
    isCalculating,
    folderExclusions,
    setMethodType,
    setSectionCount,
    setMaxDurationSecs,
    setMaxSizeBytes,
    setNameTemplate,
    setSectionPlans,
    setIsCalculating,
    getPartitionMethod,
    getActiveFolders,
  } = useSectionStore();

  const entries = usePlaylistStore((s) => s.entries);
  const outputPath = useMergeStore((s) => s.outputPath || '');

  const [isOpen, setIsOpen] = useState(true);

  const totalDuration = entries.reduce((sum, e) => sum + (e.mediaInfo?.duration || 0), 0);
  const totalVideos = entries.length;
  const totalSize = entries.reduce((sum, e) => sum + (e.mediaInfo?.size || 0), 0);

  const folders: FolderForMerge[] = React.useMemo(() => {
    const folderMap = new Map<string, FolderForMerge>();
    
    for (const entry of entries) {
      const folderPath = entry.parentFolder || 'Uncategorized';
      const folder = folderMap.get(folderPath);
      
      if (folder) {
        folder.videoCount += 1;
        folder.durationSecs += entry.mediaInfo?.duration || 0;
        folder.sizeBytes += entry.mediaInfo?.size || 0;
        folder.filePaths.push(entry.path);
      } else {
        folderMap.set(folderPath, {
          name: folderPath.split(/[/\\]/).pop() || folderPath,
          path: folderPath,
          videoCount: 1,
          durationSecs: entry.mediaInfo?.duration || 0,
          sizeBytes: entry.mediaInfo?.size || 0,
          filePaths: [entry.path],
          includeInSections: folderExclusions[folderPath] !== false,
        });
      }
    }
    
    return Array.from(folderMap.values());
  }, [entries, folderExclusions]);

  const activeFolders = getActiveFolders(folders);

  const calculatePreview = useCallback(async () => {
    if (activeFolders.length === 0) return;
    
    setIsCalculating(true);
    try {
      const method = getPartitionMethod();
      const plans = await sectionCommands.computeSectionPreview(
        activeFolders,
        method,
        nameTemplate,
      );
      setSectionPlans(plans);
    } catch (error) {
      console.error('Failed to compute section preview:', error);
    } finally {
      setIsCalculating(false);
    }
  }, [activeFolders, getPartitionMethod, nameTemplate, setSectionPlans, setIsCalculating]);

  useEffect(() => {
    const debounce = setTimeout(() => {
      calculatePreview();
    }, 300);
    return () => clearTimeout(debounce);
  }, [calculatePreview, methodType, sectionCount, maxDurationSecs, maxSizeBytes, nameTemplate]);

  const handleQuickSplit = (count: number) => {
    setMethodType('sectionCount');
    setSectionCount(count);
  };

  const handleStartMerge = () => {
    if (sectionPlans.length === 0) return;
    
    const config: SectionMergeConfig = {
      method: getPartitionMethod(),
      nameTemplate,
      outputSubfolder: 'Sections',
      outputBaseDir: outputPath || '.',
      baseMergeMode: 'smartMkv',
      normalizeAudio: true,
    };
    
    onStartMerge(config);
  };

  const formatSize = (bytes: number) => {
    const gb = bytes / 1e9;
    if (gb >= 1) return `${gb.toFixed(1)} GB`;
    const mb = bytes / 1e6;
    return `${mb.toFixed(0)} MB`;
  };

  const formatDurationShort = (secs: number) => {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    return h > 0 ? `${h}h ${m}m` : `${m}m`;
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <motion.div
        initial={{ opacity: 0, scale: 0.95 }}
        animate={{ opacity: 1, scale: 1 }}
        className="bg-white dark:bg-zinc-900 rounded-xl shadow-2xl w-full max-w-2xl max-h-[90vh] overflow-hidden flex flex-col"
      >
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-zinc-200 dark:border-zinc-700">
          <div className="flex items-center gap-3">
            <div className="p-2 bg-blue-100 dark:bg-blue-900/30 rounded-lg">
              <Layers className="w-5 h-5 text-blue-600 dark:text-blue-400" />
            </div>
            <div>
              <h2 className="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
                Merge by Sections
              </h2>
              <p className="text-sm text-zinc-500 dark:text-zinc-400">
                Split your playlist into multiple output files
              </p>
            </div>
          </div>
          <button
            onClick={() => setIsOpen(!isOpen)}
            className="p-1 hover:bg-zinc-100 dark:hover:bg-zinc-800 rounded"
          >
            {isOpen ? <ChevronUp className="w-5 h-5" /> : <ChevronDown className="w-5 h-5" />}
          </button>
        </div>

        {/* Content */}
        {isOpen && (
          <div className="flex-1 overflow-y-auto p-6 space-y-6">
            {/* Quick Actions */}
            <div className="flex items-center gap-2">
              <span className="text-sm text-zinc-500 dark:text-zinc-400">Quick Split:</span>
              {[2, 3, 4, 5].map((n) => (
                <Button
                  key={n}
                  variant="outline"
                  size="sm"
                  onClick={() => handleQuickSplit(n)}
                  className={cn(
                    sectionCount === n && methodType === 'sectionCount' &&
                    'border-blue-500 bg-blue-50 dark:bg-blue-900/20'
                  )}
                >
                  Split Into {n}
                </Button>
              ))}
            </div>

            {/* Partition Method */}
            <div className="space-y-3">
              <label className="text-sm font-medium text-zinc-700 dark:text-zinc-300">
                Partition Method
              </label>
              <div className="grid grid-cols-3 gap-3">
                <MethodCard
                  icon={<SplitSquareHorizontal className="w-4 h-4" />}
                  label="By Sections"
                  description="Divide into N equal parts"
                  selected={methodType === 'sectionCount'}
                  onClick={() => setMethodType('sectionCount')}
                />
                <MethodCard
                  icon={<Clock className="w-4 h-4" />}
                  label="By Duration"
                  description="Max duration per part"
                  selected={methodType === 'maxDuration'}
                  onClick={() => setMethodType('maxDuration')}
                />
                <MethodCard
                  icon={<HardDrive className="w-4 h-4" />}
                  label="By Size"
                  description="Max size per part"
                  selected={methodType === 'maxSize'}
                  onClick={() => setMethodType('maxSize')}
                />
              </div>
            </div>

            {/* Method-specific inputs */}
            {methodType === 'sectionCount' && (
              <div className="space-y-2">
                <label className="text-sm font-medium text-zinc-700 dark:text-zinc-300">
                  Number of Sections
                </label>
                <Input
                  type="number"
                  min={2}
                  max={50}
                  value={sectionCount}
                  onChange={(e) => setSectionCount(parseInt(e.target.value, 10) || 2)}
                  className="w-32"
                />
              </div>
            )}

            {methodType === 'maxDuration' && (
              <div className="space-y-2">
                <label className="text-sm font-medium text-zinc-700 dark:text-zinc-300">
                  Maximum Duration (hours)
                </label>
                <Input
                  type="number"
                  min={1}
                  max={12}
                  value={maxDurationSecs / 3600}
                  onChange={(e) => setMaxDurationSecs((parseFloat(e.target.value) || 1) * 3600)}
                  className="w-32"
                />
              </div>
            )}

            {methodType === 'maxSize' && (
              <div className="space-y-2">
                <label className="text-sm font-medium text-zinc-700 dark:text-zinc-300">
                  Maximum Size (GB)
                </label>
                <Input
                  type="number"
                  min={1}
                  max={100}
                  value={maxSizeBytes / 1e9}
                  onChange={(e) => setMaxSizeBytes((parseFloat(e.target.value) || 1) * 1e9)}
                  className="w-32"
                />
              </div>
            )}

            {/* Name Template */}
            <div className="space-y-2">
              <label className="text-sm font-medium text-zinc-700 dark:text-zinc-300">
                Output Name Template
              </label>
              <Input
                value={nameTemplate}
                onChange={(e) => setNameTemplate(e.target.value)}
                placeholder="Course_Part_{N}"
                className="w-full"
              />
              <p className="text-xs text-zinc-500">
                Tokens: {'{N}'} = section number, {'{TOTAL}'} = total sections, {'{DURATION}'} = duration
              </p>
            </div>

            {/* Preview */}
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <label className="text-sm font-medium text-zinc-700 dark:text-zinc-300">
                  Section Preview
                </label>
                {isCalculating && (
                  <Badge variant="default" className="animate-pulse">
                    Calculating...
                  </Badge>
                )}
              </div>

              {sectionPlans.length > 0 && (
                <div className="border border-zinc-200 dark:border-zinc-700 rounded-lg overflow-hidden">
                  <table className="w-full text-sm">
                    <thead className="bg-zinc-50 dark:bg-zinc-800">
                      <tr>
                        <th className="px-3 py-2 text-left font-medium">Section</th>
                        <th className="px-3 py-2 text-left font-medium">Folders</th>
                        <th className="px-3 py-2 text-left font-medium">Videos</th>
                        <th className="px-3 py-2 text-left font-medium">Duration</th>
                        <th className="px-3 py-2 text-left font-medium">Est. Size</th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-zinc-100 dark:divide-zinc-800">
                      {sectionPlans.map((plan) => (
                        <tr key={plan.sectionIndex}>
                          <td className="px-3 py-2 font-medium">{plan.outputName}</td>
                          <td className="px-3 py-2 text-zinc-600 dark:text-zinc-400">
                            {plan.boundary.folderNames.slice(0, 2).join(', ')}
                            {plan.boundary.folderNames.length > 2 && ` +${plan.boundary.folderNames.length - 2}`}
                          </td>
                          <td className="px-3 py-2">{plan.boundary.videoCount}</td>
                          <td className="px-3 py-2">{formatDurationShort(plan.boundary.durationSecs)}</td>
                          <td className="px-3 py-2">{formatSize(plan.boundary.estimatedSizeBytes)}</td>
                        </tr>
                      ))}
                    </tbody>
                    <tfoot className="bg-zinc-50 dark:bg-zinc-800">
                      <tr>
                        <td className="px-3 py-2 font-medium">Total</td>
                        <td className="px-3 py-2">{activeFolders.length}</td>
                        <td className="px-3 py-2">{totalVideos}</td>
                        <td className="px-3 py-2">{formatDuration(totalDuration)}</td>
                        <td className="px-3 py-2">{formatSize(totalSize)}</td>
                      </tr>
                    </tfoot>
                  </table>
                </div>
              )}

              {sectionPlans.length === 0 && !isCalculating && (
                <div className="text-center py-8 text-zinc-500">
                  <FolderOpen className="w-8 h-8 mx-auto mb-2 opacity-50" />
                  <p>Add folders to see section preview</p>
                </div>
              )}
            </div>

            {/* Output Location */}
            <div className="text-sm text-zinc-500">
              Output: <span className="font-mono text-zinc-700 dark:text-zinc-300">{outputPath || '.'}/Sections/</span>
            </div>
          </div>
        )}

        {/* Footer */}
        <div className="flex items-center justify-end gap-3 px-6 py-4 border-t border-zinc-200 dark:border-zinc-700 bg-zinc-50 dark:bg-zinc-800">
          <Button variant="outline" onClick={onCancel}>
            Cancel
          </Button>
          <Button
            onClick={handleStartMerge}
            disabled={sectionPlans.length === 0 || isCalculating}
          >
            <Play className="w-4 h-4 mr-2" />
            Start Section Merge
          </Button>
        </div>
      </motion.div>
    </div>
  );
}

function MethodCard({
  icon,
  label,
  description,
  selected,
  onClick,
}: {
  icon: React.ReactNode;
  label: string;
  description: string;
  selected: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        'flex flex-col items-start p-3 rounded-lg border-2 transition-colors text-left',
        selected
          ? 'border-blue-500 bg-blue-50 dark:bg-blue-900/20'
          : 'border-zinc-200 dark:border-zinc-700 hover:border-zinc-300 dark:hover:border-zinc-600'
      )}
    >
      <div className={cn(
        'mb-2',
        selected ? 'text-blue-600 dark:text-blue-400' : 'text-zinc-400'
      )}>
        {icon}
      </div>
      <div className="font-medium text-sm text-zinc-900 dark:text-zinc-100">{label}</div>
      <div className="text-xs text-zinc-500">{description}</div>
    </button>
  );
}