import React, { useEffect, useRef } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { Pencil, Trash2, MoveUp, MoveDown, FolderInput, ArrowUpToLine } from 'lucide-react';
import { cn } from '@/utils/cn';
import { usePlaylistStore } from '@/store/playlistStore';

interface FolderContextMenuProps {
  x: number;
  y: number;
  folderId: string;
  onClose: () => void;
  onRename: (folderId: string) => void;
}

interface MenuItem {
  label: string;
  icon: React.ReactNode;
  action: () => void;
  danger?: boolean;
  separator?: boolean;
}

export function FolderContextMenu({ x, y, folderId, onClose, onRename }: FolderContextMenuProps) {
  const folders = usePlaylistStore((s) => s.folders);
  const moveFolderUp = usePlaylistStore((s) => s.moveFolderUp);
  const moveFolderDown = usePlaylistStore((s) => s.moveFolderDown);
  const deleteFolder = usePlaylistStore((s) => s.deleteFolder);
  const selectOnlyFolder = usePlaylistStore((s) => s.selectOnlyFolder);
  const menuRef = useRef<HTMLDivElement>(null);

  const folderIndex = folders.findIndex((f) => f.id === folderId);
  const folder = folders[folderIndex];
  const isFirst = folderIndex === 0;
  const isLast = folderIndex === folders.length - 1;

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose();
      }
    };
    const escHandler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('mousedown', handler);
    document.addEventListener('keydown', escHandler);
    return () => {
      document.removeEventListener('mousedown', handler);
      document.removeEventListener('keydown', escHandler);
    };
  }, [onClose]);

  const menuWidth = 200;
  const menuHeight = 240;
  const adjustedX = Math.min(x, window.innerWidth - menuWidth - 8);
  const adjustedY = Math.min(y, window.innerHeight - menuHeight - 8);

  const items: MenuItem[] = [
    {
      label: 'Rename',
      icon: <Pencil className="w-3.5 h-3.5" />,
      action: () => {
        onRename(folderId);
        onClose();
      },
    },
    {
      label: 'Move to Top',
      icon: <ArrowUpToLine className="w-3.5 h-3.5" />,
      action: () => {
        if (!isFirst) moveFolderUp(folderId);
        onClose();
      },
      separator: true,
    },
    {
      label: 'Move Up',
      icon: <MoveUp className="w-3.5 h-3.5" />,
      action: () => {
        if (!isFirst) moveFolderUp(folderId);
        onClose();
      },
    },
    {
      label: 'Move Down',
      icon: <MoveDown className="w-3.5 h-3.5" />,
      action: () => {
        if (!isLast) moveFolderDown(folderId);
        onClose();
      },
    },
    {
      label: 'Select Only This Folder',
      icon: <FolderInput className="w-3.5 h-3.5" />,
      action: () => {
        selectOnlyFolder(folderId);
        onClose();
      },
      separator: true,
    },
    {
      label: `Delete "${folder?.name || 'Folder'}"`,
      icon: <Trash2 className="w-3.5 h-3.5" />,
      action: () => {
        deleteFolder(folderId);
        onClose();
      },
      danger: true,
    },
  ];

  return (
    <AnimatePresence>
      <motion.div
        ref={menuRef}
        initial={{ opacity: 0, scale: 0.96, y: -4 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        exit={{ opacity: 0, scale: 0.96, y: -4 }}
        transition={{ duration: 0.12, ease: [0.16, 1, 0.3, 1] }}
        className={cn(
          'fixed z-50 w-[200px]',
          'bg-bg-elevated/95 backdrop-blur-md',
          'border border-border rounded-lg shadow-modal',
          'py-1 overflow-hidden'
        )}
        style={{ left: adjustedX, top: adjustedY }}
      >
        {items.map((item, i) => (
          <React.Fragment key={i}>
            {item.separator && i > 0 && (
              <div className="my-1 h-px bg-border mx-2" />
            )}
            <button
              className={cn(
                'w-full flex items-center gap-2.5 px-3 py-1.5',
                'text-xs text-left transition-colors duration-100',
                'hover:bg-bg-overlay',
                item.danger
                  ? 'text-danger hover:bg-danger/10'
                  : 'text-text-secondary hover:text-text-primary'
              )}
              onClick={item.action}
            >
              <span className="shrink-0">{item.icon}</span>
              {item.label}
            </button>
          </React.Fragment>
        ))}
      </motion.div>
    </AnimatePresence>
  );
}