import React from 'react';
import { motion } from 'framer-motion';
import { Film, FolderOpen, FilePlus2, Clock, Zap } from 'lucide-react';
import { cn } from '@/utils/cn';
import { Button } from '@/components/ui/Button';
import { useAppStore } from '@/store/appStore';
import { usePlaylistStore } from '@/store/playlistStore';
import { useDropZone } from '@/hooks/useDropZone';
import { useFileImport } from '@/hooks/useFileImport';
import { tauriCommands } from '@/tauri/commands';
import { formatDuration, formatBytes } from '@/utils';

const FEATURE_LIST = [
  'Lossless stream copy — zero quality loss',
  'Drag & reorder unlimited clips',
  'FFprobe compatibility analysis',
  'Virtualized list — handles 1000+ files',
  'Thumbnail preview with cache',
];

export function HomeScreen() {
  const appStore = useAppStore();
  const playlistStore = usePlaylistStore();
  const settings = appStore.settings;
  const { pickAndAddFiles, pickAndAddFolder, handleNativeDrop } = useFileImport();
  const [isPickingFiles, setIsPickingFiles] = React.useState(false);
  const [isPickingFolder, setIsPickingFolder] = React.useState(false);

  const handleAddFiles = React.useCallback(async () => {
    if (isPickingFiles) return;
    setIsPickingFiles(true);
    try {
      await pickAndAddFiles();
    } finally {
      setIsPickingFiles(false);
    }
  }, [pickAndAddFiles, isPickingFiles]);

  const handleAddFolder = React.useCallback(async () => {
    if (isPickingFolder) return;
    setIsPickingFolder(true);
    try {
      await pickAndAddFolder();
    } finally {
      setIsPickingFolder(false);
    }
  }, [pickAndAddFolder, isPickingFolder]);

  const { isDragOver, dropHandlers } = useDropZone({
    onDropFiles: handleNativeDrop,
  });

  const recentExports = settings?.recentExports ?? [];

  return (
    <div className="flex flex-col h-full overflow-y-auto">
      {/* Hero drop zone */}
      <div className="flex-1 flex flex-col items-center justify-center p-8 gap-6">

        {/* Drop zone */}
        <motion.div
          {...dropHandlers}
          initial={{ opacity: 0, y: 12 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.4, ease: [0.16, 1, 0.3, 1] }}
          className={cn(
            'relative w-full max-w-lg rounded-2xl',
            'border-2 border-dashed',
            'flex flex-col items-center justify-center gap-4 py-14 px-8',
            'transition-all duration-200',
            isDragOver
              ? 'border-accent-500 bg-accent-muted shadow-glow scale-[1.01]'
              : 'border-border hover:border-border-strong bg-bg-elevated/40 hover:bg-bg-elevated/60'
          )}
        >
          <div className={cn(
            'w-14 h-14 rounded-2xl flex items-center justify-center',
            'transition-all duration-200',
            isDragOver
              ? 'bg-accent-muted text-accent-400'
              : 'bg-bg-overlay border border-border text-text-muted'
          )}>
            <Film className="w-7 h-7" />
          </div>

          <div className="text-center">
            <p className={cn(
              'text-base font-semibold transition-colors duration-200',
              isDragOver ? 'text-accent-300' : 'text-text-primary'
            )}>
              {isDragOver ? 'Drop to add videos' : 'Drop videos or folders here'}
            </p>
            <p className="text-xs text-text-muted mt-1">
              MP4, MKV, MOV, AVI, WebM and more
            </p>
          </div>

          <div className="flex items-center gap-3">
            <Button
              variant="primary"
              size="md"
              leftIcon={<FilePlus2 className="w-4 h-4" />}
              onClick={handleAddFiles}
              loading={isPickingFiles}
            >
              Select Files
            </Button>
            <Button
              variant="secondary"
              size="md"
              leftIcon={<FolderOpen className="w-4 h-4" />}
              onClick={handleAddFolder}
              loading={isPickingFolder}
            >
              Select Folder
            </Button>
          </div>
        </motion.div>

        {/* Continue editing if playlist exists */}
        {playlistStore.entries.length > 0 && (
          <motion.div
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0.1 }}
            className="w-full max-w-lg"
          >
            <button
              onClick={() => appStore.setScreen('playlist')}
              className={cn(
                'w-full flex items-center justify-between px-4 py-3 rounded-xl',
                'bg-accent-muted border border-accent-500/20',
                'hover:bg-accent-500/15 hover:border-accent-500/30 transition-colors',
                'group'
              )}
            >
              <div className="flex items-center gap-3">
                <div className="w-8 h-8 rounded-lg bg-accent-500/20 flex items-center justify-center">
                  <Zap className="w-4 h-4 text-accent-400" />
                </div>
                <div className="text-left">
                  <p className="text-sm font-medium text-text-primary">
                    Continue — {playlistStore.playlistName}
                  </p>
                  <p className="text-xs text-text-muted">
                    {playlistStore.entries.length} files
                    {' · '}
                    {formatDuration(playlistStore.getTotalDuration())}
                  </p>
                </div>
              </div>
              <span className="text-text-muted group-hover:text-text-secondary text-sm transition-colors">→</span>
            </button>
          </motion.div>
        )}

        {/* Features list */}
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ delay: 0.2, duration: 0.4 }}
          className="flex flex-col gap-1.5"
        >
          {FEATURE_LIST.map((feat, i) => (
            <div key={i} className="flex items-center gap-2 text-xs text-text-muted">
              <div className="w-1 h-1 rounded-full bg-accent-500/60 shrink-0" />
              {feat}
            </div>
          ))}
        </motion.div>
      </div>

      {/* Recent exports */}
      {recentExports.length > 0 && (
        <div className="border-t border-border px-6 py-4">
          <div className="flex items-center gap-2 mb-3">
            <Clock className="w-3.5 h-3.5 text-text-muted" />
            <span className="text-xs font-medium text-text-muted uppercase tracking-wider">
              Recent Exports
            </span>
          </div>
          <div className="flex flex-col gap-1">
            {recentExports.slice(0, 5).map((exp, i) => (
              <button
                key={i}
                onClick={() => tauriCommands.revealInExplorer(exp.path).catch(console.error)}
                className="flex items-center justify-between px-2 py-1.5 rounded-md hover:bg-bg-elevated transition-colors group text-left"
              >
                <span className="text-xs text-text-secondary truncate flex-1 group-hover:text-text-primary">
                  {exp.path.replace(/\\/g, '/').split('/').pop()}
                </span>
                <span className="text-2xs text-text-muted ml-3 shrink-0">
                  {formatBytes(exp.sizeBytes)}
                </span>
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
