import React from 'react';
import { motion } from 'framer-motion';
import { ListVideo, Zap, Repeat, Scissors, Settings, AlertCircle } from 'lucide-react';
import { message } from '@tauri-apps/plugin-dialog';
import { cn } from '@/utils/cn';
import { useAppStore } from '@/store/appStore';
import { useMergeStore } from '@/store/mergeStore';
import { usePlaylistStore } from '@/store/playlistStore';
import type { AppScreen } from '@/types';

interface NavItem {
  id: AppScreen;
  icon: React.ReactNode;
  label: string;
  badge?: string | number;
}

export function Sidebar() {
  const screen = useAppStore((s) => s.screen);
  const setScreen = useAppStore((s) => s.setScreen);
  const ffmpegMissing = useAppStore((s) => s.ffmpegMissing);
  const activeJob = useMergeStore((s) => s.activeJob);
  const entryCount = usePlaylistStore((s) => s.entries.length);

  const mergeIsActive = activeJob ? ['writing', 'preparing', 'probing', 'validating', 'normalizing', 'finalizing'].includes(activeJob.progress.phase) : false;

  const navItems: NavItem[] = [
    {
      id: 'playlist',
      icon: <ListVideo className="w-4 h-4" />,
      label: 'Playlist',
      badge: entryCount > 0 ? entryCount : undefined,
    },
    {
      id: 'merge',
      icon: <Zap className="w-4 h-4" />,
      label: 'Merge',
      badge: mergeIsActive ? '●' : undefined,
    },
    {
      id: 'repeat',
      icon: <Repeat className="w-4 h-4" />,
      label: 'Repeat',
    },
    {
      id: 'split',
      icon: <Scissors className="w-4 h-4" />,
      label: 'Split',
    },
    {
      id: 'settings',
      icon: <Settings className="w-4 h-4" />,
      label: 'Settings',
    },
  ];

  return (
    <nav className="flex flex-col w-14 h-full bg-bg-surface border-r border-border py-3 items-center gap-1">

      {navItems.map((item) => {
        const isActive = screen === item.id;
        const handleNav = async () => {
          if (isActive) return;
          // Warn when leaving merge screen during active merge
          if (screen === 'merge' && mergeIsActive && item.id !== 'merge') {
            const confirmed = await message(
              'A merge is in progress. Leaving this screen will hide the merge progress. The merge will continue in the background.',
              { title: 'Merge in Progress', kind: 'warning' }
            );
            if (!confirmed) return;
          }
          setScreen(item.id);
        };
        return (
          <button
            key={item.id}
            onClick={handleNav}
            title={item.label}
            className={cn(
              'relative w-10 h-10 rounded-lg flex flex-col items-center justify-center gap-0.5',
              'transition-all duration-150 group',
              'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-500/50',
              isActive
                ? 'bg-accent-muted text-accent-400'
                : 'text-text-muted hover:text-text-secondary hover:bg-bg-elevated'
            )}
          >
            {isActive && (
              <motion.div
                layoutId="nav-indicator"
                className="absolute left-0 w-0.5 h-5 bg-accent-500 rounded-r-full"
                transition={{ type: 'spring', stiffness: 400, damping: 30 }}
              />
            )}

            <span className={cn(
              'transition-colors',
              isActive ? 'text-accent-400' : 'text-text-muted group-hover:text-text-secondary'
            )}>
              {item.icon}
            </span>

            {item.badge !== undefined && (
              <span className={cn(
                'text-2xs font-medium leading-none',
                item.badge === '●' ? 'text-success animate-pulse' : 'text-text-muted',
                isActive && 'text-accent-400'
              )}>
                {item.badge}
              </span>
            )}

            {/* Tooltip */}
            <div className={cn(
              'absolute left-full ml-2 px-2 py-1 rounded-md text-xs font-medium',
              'bg-bg-elevated border border-border shadow-modal text-text-primary',
              'opacity-0 group-hover:opacity-100 pointer-events-none',
              'transition-opacity duration-150 whitespace-nowrap z-50'
            )}>
              {item.label}
            </div>
          </button>
        );
      })}

      {/* FFmpeg missing warning */}
      {ffmpegMissing && (
        <div className="mt-auto w-10 h-10 rounded-lg flex items-center justify-center text-warning" title="FFmpeg not found — check Settings">
          <AlertCircle className="w-4 h-4" />
        </div>
      )}
    </nav>
  );
}
