import React from 'react';
import { FileVideo, FolderOpen } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { useSplitStore } from '@/store/splitStore';
import { formatDuration, formatBytes } from '@/utils';

export function SplitInputPanel() {
  const inputFile = useSplitStore((s) => s.inputFile);
  const inputDuration = useSplitStore((s) => s.inputDuration);
  const inputSizeBytes = useSplitStore((s) => s.inputSizeBytes);
  const setInputFile = useSplitStore((s) => s.setInputFile);
  const setOutputDir = useSplitStore((s) => s.setOutputDir);
  const outputDir = useSplitStore((s) => s.outputDir);

  const handleSelectFile = async () => {
    const { openVideoFilesDialog } = await import('@/tauri/commands');
    const files = await openVideoFilesDialog();
    if (files.length > 0) {
      setInputFile(files[0]);
      // Auto-probe the file
      try {
        const { tauriCommands } = await import('@/tauri/commands');
        const info = await tauriCommands.probeVideo(files[0]);
        useSplitStore.getState().setInputDuration(info.duration);
        useSplitStore.getState().setInputSizeBytes(info.size);
      } catch { /* probe failed, keep duration as 0 */ }
    }
  };

  const handleSelectOutputDir = async () => {
    const { openFolderDialog } = await import('@/tauri/commands');
    const dir = await openFolderDialog();
    if (dir) setOutputDir(dir);
  };

  return (
    <div className="space-y-4">
      {/* Input file area */}
      <div className="space-y-1.5">
        <label className="block text-xs font-medium text-text-secondary">
          Input Video File
        </label>
        {!inputFile ? (
          <div
            onClick={handleSelectFile}
            className="border border-dashed border-border hover:border-accent-500/50 bg-bg-surface/20 hover:bg-bg-surface/40 cursor-pointer rounded-xl p-6 transition-all flex flex-col items-center justify-center gap-2.5 text-center group shadow-inner"
          >
            <div className="w-10 h-10 rounded-full bg-accent-500/10 border border-accent-500/20 flex items-center justify-center group-hover:scale-110 transition-transform">
              <FileVideo className="w-5 h-5 text-accent-400 group-hover:animate-pulse" />
            </div>
            <div className="space-y-0.5">
              <p className="text-xs font-semibold text-text-primary">Select Input Video File</p>
              <p className="text-[10px] text-text-muted">Click here to browse your computer for a video to split</p>
            </div>
            <Button variant="secondary" size="sm" className="mt-1" onClick={(e) => { e.stopPropagation(); handleSelectFile(); }}>
              Browse File
            </Button>
          </div>
        ) : (
          <div className="p-4 rounded-xl border border-border/80 bg-bg-surface/30 flex flex-col gap-3 shadow-sm">
            <div className="flex items-center justify-between gap-3 min-w-0">
              <div className="flex items-center gap-2.5 min-w-0">
                <div className="w-8 h-8 rounded-lg bg-accent-500/10 border border-accent-500/20 flex items-center justify-center shrink-0">
                  <FileVideo className="w-4 h-4 text-accent-400" />
                </div>
                <div className="min-w-0">
                  <p className="text-xs font-semibold text-text-primary truncate" title={inputFile}>
                    {inputFile.replace(/\\/g, '/').split('/').pop() || 'video'}
                  </p>
                  <p className="text-[9px] text-text-muted truncate mt-0.5" title={inputFile}>
                    {inputFile}
                  </p>
                </div>
              </div>
              <Button variant="secondary" size="sm" className="shrink-0" onClick={handleSelectFile}>
                Change
              </Button>
            </div>

            <div className="grid grid-cols-2 gap-2 border-t border-border/40 pt-3">
              <div className="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-bg-base/30 border border-border/30">
                <span className="text-[10px] text-text-muted">Duration:</span>
                <span className="text-[11px] font-mono font-semibold text-text-secondary">
                  {formatDuration(inputDuration)}
                </span>
              </div>
              <div className="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-bg-base/30 border border-border/30">
                <span className="text-[10px] text-text-muted">File Size:</span>
                <span className="text-[11px] font-mono font-semibold text-text-secondary">
                  {formatBytes(inputSizeBytes)}
                </span>
              </div>
            </div>
          </div>
        )}
      </div>

      {/* Output directory area */}
      <div className="space-y-1.5">
        <label className="block text-xs font-medium text-text-secondary">
          Output Directory
        </label>
        <div className="flex gap-2">
          <div className="flex-1 flex items-center gap-2 px-3 py-2 rounded-lg bg-bg-elevated/40 border border-border hover:border-border-strong text-sm text-text-secondary truncate transition-colors duration-150">
            <FolderOpen className="w-4 h-4 shrink-0 text-text-muted" />
            <span className="truncate" title={outputDir || 'Same as input file'}>
              {outputDir || 'Same as input file'}
            </span>
          </div>
          <Button variant="secondary" size="sm" onClick={handleSelectOutputDir}>
            Browse
          </Button>
        </div>
      </div>
    </div>
  );
}
