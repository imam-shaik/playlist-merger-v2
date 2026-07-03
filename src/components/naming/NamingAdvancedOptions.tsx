import React, { useState } from 'react';
import { ChevronDown, ChevronUp, Download } from 'lucide-react';
import { save } from '@tauri-apps/plugin-dialog';
import { cn } from '@/utils/cn';
import { useSplitStore } from '@/store/splitStore';
import { tauriCommands } from '@/tauri/commands';

export function NamingAdvancedOptions() {
  const [open, setOpen] = useState(false);
  const [exporting, setExporting] = useState(false);
  const namingConfig = useSplitStore((s) => s.namingConfig);
  const setNamingConfig = useSplitStore((s) => s.setNamingConfig);
  const currentPlan = useSplitStore((s) => s.currentPlan);
  const inputFile = useSplitStore((s) => s.inputFile);

  const exportPreviewList = async () => {
    if (!currentPlan || !namingConfig.template.trim()) return;

    setExporting(true);
    try {
      const stem = inputFile
        ? inputFile.replace(/^.*[/\\]/, '').replace(/\.[^.]+$/, '')
        : 'Video';
      const now = new Date();

      const segments = currentPlan.segments.map((seg, i) => ({
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
      }));

      const names = await tauriCommands.previewNamingBatch(
        namingConfig.template,
        namingConfig,
        segments,
      );

      const content = names.map((n, i) => `${String(i + 1).padStart(3, ' ')}. ${n}`).join('\n');
      const header = `Naming Preview — ${currentPlan.segments.length} outputs\nTemplate: ${namingConfig.template}\nGenerated: ${now.toISOString()}\n\n${content}\n`;

      const defaultPath = currentPlan.outputDir || '.';
      const suggested = `${defaultPath}/naming_preview.txt`;

      const path = await save({
        defaultPath: suggested,
        filters: [{ name: 'Text Files', extensions: ['txt'] }],
      });

      if (path) {
        await tauriCommands.writeTextFile(path, header);
      }
    } catch (err) {
      console.error('[Naming] Export failed:', err);
    } finally {
      setExporting(false);
    }
  };

  return (
    <div className="border-t border-border/40 pt-3">
      <button
        onClick={() => setOpen((v) => !v)}
        className="flex items-center gap-1.5 text-[10px] text-text-muted hover:text-text-secondary transition-colors"
      >
        {open ? (
          <ChevronUp className="w-3 h-3" />
        ) : (
          <ChevronDown className="w-3 h-3" />
        )}
        Advanced options
      </button>

      {open && (
        <div className="mt-3 space-y-3">
          {/* Zero padding */}
          <div className="flex items-center gap-3">
            <span className="text-[10px] text-text-muted w-20 shrink-0">Zero Padding</span>
            <div className="flex gap-1">
              {([2, 3, 4] as const).map((p) => (
                <button
                  key={p}
                  onClick={() =>
                    setNamingConfig({ ...namingConfig, zeroPadding: p })
                  }
                  className={cn(
                    'px-2.5 py-1 rounded text-[10px] font-medium border transition-all',
                    namingConfig.zeroPadding === p
                      ? 'bg-accent-500/10 border-accent-500/30 text-accent-300'
                      : 'bg-bg-elevated/20 border-border/80 text-text-muted hover:border-border-strong'
                  )}
                >
                  {p}
                </button>
              ))}
            </div>
            <span className="text-[9px] text-text-muted">digits</span>
          </div>

          {/* Separator */}
          <div className="flex items-center gap-3">
            <span className="text-[10px] text-text-muted w-20 shrink-0">Separator</span>
            <input
              type="text"
              value={namingConfig.separator ?? '_'}
              onChange={(e) =>
                setNamingConfig({ ...namingConfig, separator: e.target.value })
              }
              maxLength={3}
              className="w-16 px-2 py-1 rounded bg-bg-elevated/30 border border-border/80 text-xs text-center font-mono text-text-primary focus:outline-none focus:border-accent-500/50"
            />
            <span className="text-[9px] text-text-muted">default: _</span>
          </div>

          {/* Zero padding preview */}
          <div className="text-[10px] text-text-muted">
            Sequence example:{' '}
            <span className="font-mono text-text-secondary">
              {(() => {
                const p = namingConfig.zeroPadding ?? 3;
                const examples = [1, 10, 100];
                return examples
                  .map((n) => String(n).padStart(p, '0'))
                  .join(', ');
              })()}
            </span>
          </div>

          {/* Export preview list */}
          {currentPlan && (
            <div className="pt-2 border-t border-border/40">
              <button
                onClick={exportPreviewList}
                disabled={exporting}
                className="flex items-center gap-1.5 text-[10px] text-accent-400 hover:text-accent-300 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
              >
                <Download className="w-3 h-3" />
                {exporting ? 'Exporting...' : `Export preview list (${currentPlan.segments.length} files)`}
              </button>
              <p className="text-[9px] text-text-muted mt-0.5">
                Saves all output filenames to naming_preview.txt
              </p>
            </div>
          )}
        </div>
      )}
    </div>
  );
}