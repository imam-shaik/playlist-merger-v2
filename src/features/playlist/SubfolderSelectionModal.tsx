import React, { useState, useMemo } from 'react';
import { motion } from 'framer-motion';
import { Folder, Search, X, Check, FolderOpen } from 'lucide-react';
import { useFolderSelectStore } from '@/store/folderSelectStore';

export function SubfolderSelectionModal() {
  const renderCount = React.useRef(0);
  renderCount.current++;
  const { isOpen, rootPath, subfolders, selectedPaths, toggleFolder, selectAll, selectNone, confirm, cancel } =
    useFolderSelectStore();
  const [search, setSearch] = useState('');

  console.log('[FOLDER_AUDIT] MODAL_RENDER - render#:', renderCount.current, 'isOpen:', isOpen, 'subfolders.length:', subfolders.length, 'rootPath:', rootPath);

  const filteredSubfolders = useMemo(() => {
    if (!search.trim()) return subfolders;
    const q = search.toLowerCase();
    return subfolders.filter((s) => s.name.toLowerCase().includes(q));
  }, [subfolders, search]);

  const totalSelected = selectedPaths.size;
  const totalVideos = subfolders.reduce((sum, s) => sum + s.videoCount, 0);
  const selectedVideos = subfolders
    .filter((s) => selectedPaths.has(s.path))
    .reduce((sum, s) => sum + s.videoCount, 0);

  if (!isOpen) return null;

  const rootName = rootPath?.split(/[/\\]/).pop() ?? 'Folder';

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/70 backdrop-blur-sm p-4">
      <div className="absolute inset-0" onClick={cancel} />
      <motion.div
        initial={{ opacity: 0, scale: 0.95, y: 10 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        transition={{ duration: 0.2, ease: [0.16, 1, 0.3, 1] }}
        className="relative z-10 bg-bg-elevated border border-border shadow-modal rounded-xl w-full max-w-lg overflow-hidden flex flex-col"
        style={{ maxHeight: '80vh' }}
      >
        <div className="flex items-center justify-between px-5 py-4 border-b border-border/80 shrink-0">
          <div className="flex items-center gap-2 min-w-0">
            <FolderOpen className="w-4 h-4 text-accent-400 shrink-0" />
            <h2 className="text-sm font-semibold text-text-primary truncate">
              Select Folders to Include
            </h2>
          </div>
          <button
            onClick={cancel}
            className="p-1 rounded-lg text-text-muted hover:text-text-primary hover:bg-bg-overlay transition-all shrink-0 ml-2"
            aria-label="Close"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="px-5 py-3 border-b border-border/80 shrink-0">
          <div className="flex items-center gap-2 text-xs text-text-secondary mb-2">
            <span className="font-medium text-text-primary">{rootName}</span>
            <span>/</span>
            <span>
              {totalSelected} of {subfolders.length} folders selected
            </span>
            {totalVideos > 0 && (
              <span className="text-text-muted">
                ({selectedVideos} of {totalVideos} videos)
              </span>
            )}
          </div>
          <div className="relative">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text-muted" />
            <input
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search folders..."
              className="w-full pl-9 pr-4 py-2 bg-bg-surface border border-border rounded-lg text-sm text-text-primary placeholder:text-text-muted focus:outline-none focus:border-accent-500 focus:ring-1 focus:ring-accent-500/30"
            />
            {search && (
              <button
                onClick={() => setSearch('')}
                className="absolute right-3 top-1/2 -translate-y-1/2 p-0.5 rounded text-text-muted hover:text-text-primary"
              >
                <X className="w-3 h-3" />
              </button>
            )}
          </div>
        </div>

        <div className="flex-1 overflow-y-auto px-3 py-2">
          {filteredSubfolders.length === 0 ? (
            <div className="py-8 text-center text-sm text-text-muted">
              {search ? 'No folders match your search' : 'No subfolders found'}
            </div>
          ) : (
            <div className="flex flex-col gap-1">
              {filteredSubfolders.map((folder) => {
                const isSelected = selectedPaths.has(folder.path);
                return (
                  <button
                    key={folder.path}
                    onClick={() => toggleFolder(folder.path)}
                    className={`flex items-center gap-3 px-3 py-2.5 rounded-lg text-left transition-all ${
                      isSelected
                        ? 'bg-accent-500/10 border border-accent-500/30'
                        : 'bg-bg-surface/30 border border-transparent hover:bg-bg-surface/50'
                    }`}
                  >
                    <div
                      className={`w-5 h-5 rounded border-2 flex items-center justify-center shrink-0 transition-colors ${
                        isSelected
                          ? 'bg-accent-500 border-accent-500'
                          : 'border-border bg-bg-elevated'
                      }`}
                    >
                      {isSelected && <Check className="w-3 h-3 text-white" />}
                    </div>
                    <Folder className={`w-4 h-4 shrink-0 ${isSelected ? 'text-accent-400' : 'text-text-muted'}`} />
                    <div className="flex-1 min-w-0">
                      <p className="text-sm font-medium text-text-primary truncate">{folder.name}</p>
                      {folder.videoCount > 0 && (
                        <p className="text-xs text-text-muted">
                          {folder.videoCount} video{folder.videoCount !== 1 ? 's' : ''}
                          {folder.hasSubfolders && ' (contains subfolders)'}
                        </p>
                      )}
                    </div>
                  </button>
                );
              })}
            </div>
          )}
        </div>

        <div className="px-5 py-4 border-t border-border/80 bg-bg-surface/30 shrink-0">
          <div className="flex items-center justify-between">
            <div className="flex gap-2">
              <button
                onClick={selectAll}
                className="px-3 py-1.5 text-xs font-medium text-text-secondary hover:text-text-primary hover:bg-bg-overlay rounded-lg transition-all"
              >
                Select All
              </button>
              <button
                onClick={selectNone}
                className="px-3 py-1.5 text-xs font-medium text-text-secondary hover:text-text-primary hover:bg-bg-overlay rounded-lg transition-all"
              >
                Select None
              </button>
            </div>
            <div className="flex gap-2">
              <button
                onClick={cancel}
                className="px-4 py-2 text-sm font-medium text-text-secondary hover:text-text-primary hover:bg-bg-overlay rounded-lg transition-all"
              >
                Cancel
              </button>
              <button
                onClick={confirm}
                disabled={selectedPaths.size === 0}
                className="px-4 py-2 text-sm font-medium bg-accent-500 text-white rounded-lg hover:bg-accent-600 transition-all disabled:opacity-50 disabled:cursor-not-allowed"
              >
                Continue
              </button>
            </div>
          </div>
        </div>
      </motion.div>
    </div>
  );
}