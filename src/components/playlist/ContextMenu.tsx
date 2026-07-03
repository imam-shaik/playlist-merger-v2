import React, { useEffect, useRef } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { Pencil, Copy, Trash2, MoveUp, MoveDown, ArrowUpToLine, ArrowDownToLine } from 'lucide-react';
import { cn } from '@/utils/cn';
import { usePlaylistStore } from '@/store/playlistStore';

interface ContextMenuProps {
  x: number;
  y: number;
  targetId: string;
  onClose: () => void;
  onRename: (id: string) => void;
}

interface MenuItem {
  label: string;
  icon: React.ReactNode;
  action: () => void;
  danger?: boolean;
  separator?: boolean;
}

export function PlaylistContextMenu({ x, y, targetId, onClose, onRename }: ContextMenuProps) {
  const selectedIds = usePlaylistStore((s) => s.selectedIds);
  const entries = usePlaylistStore((s) => s.entries);
  const duplicateEntry = usePlaylistStore((s) => s.duplicateEntry);
  const moveSelectedTo = usePlaylistStore((s) => s.moveSelectedTo);
  const reorderEntry = usePlaylistStore((s) => s.reorderEntry);
  const removeEntries = usePlaylistStore((s) => s.removeEntries);
  const menuRef = useRef<HTMLDivElement>(null);

  const isMultiSelect = selectedIds.size > 1 && selectedIds.has(targetId);
  const affectedIds = isMultiSelect ? [...selectedIds] : [targetId];
  const affectedCount = affectedIds.length;

  const idx = entries.findIndex(e => e.id === targetId);

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

  // Adjust position to stay inside viewport
  const menuWidth = 200;
  const menuHeight = 280;
  const adjustedX = Math.min(x, window.innerWidth - menuWidth - 8);
  const adjustedY = Math.min(y, window.innerHeight - menuHeight - 8);

  const items: MenuItem[] = [
    ...(!isMultiSelect ? [{
      label: 'Rename',
      icon: <Pencil className="w-3.5 h-3.5" />,
      action: () => { onRename(targetId); onClose(); },
    }] : []),
    {
      label: `Duplicate${affectedCount > 1 ? ` (${affectedCount})` : ''}`,
      icon: <Copy className="w-3.5 h-3.5" />,
      action: () => {
        affectedIds.forEach(id => duplicateEntry(id));
        onClose();
      },
    },
    {
      label: 'Move to Top',
      icon: <ArrowUpToLine className="w-3.5 h-3.5" />,
      action: () => {
        moveSelectedTo(0);
        onClose();
      },
    },
    {
      label: 'Move to Bottom',
      icon: <ArrowDownToLine className="w-3.5 h-3.5" />,
      action: () => {
        moveSelectedTo(entries.length);
        onClose();
      },
    },
    ...(!isMultiSelect && idx > 0 ? [{
      label: 'Move Up',
      icon: <MoveUp className="w-3.5 h-3.5" />,
      action: () => {
        reorderEntry(idx, idx - 1);
        onClose();
      },
    }] : []),
    ...(!isMultiSelect && idx < entries.length - 1 ? [{
      label: 'Move Down',
      icon: <MoveDown className="w-3.5 h-3.5" />,
      action: () => {
        reorderEntry(idx, idx + 1);
        onClose();
      },
    }] : []),
    {
      label: `Remove${affectedCount > 1 ? ` (${affectedCount})` : ''}`,
      icon: <Trash2 className="w-3.5 h-3.5" />,
      action: () => {
        removeEntries(affectedIds);
        onClose();
      },
      danger: true,
      separator: true,
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
            {item.separator && (
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
