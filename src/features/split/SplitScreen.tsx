import React from 'react';
import { Scissors, ChevronDown, ChevronRight, FileVideo, Sliders, Eye, HardDrive, Info, Type } from 'lucide-react';
import { motion } from 'framer-motion';
import { SplitInputPanel } from './SplitInputPanel';
import { SplitModeSelector } from './SplitModeSelector';
import { SplitPreview } from './SplitPreview';
import { SplitActionBar } from './SplitActionBar';
import { SplitTaskQueue } from './SplitTaskQueue';
import { NamingPanel } from '@/components/naming/NamingPanel';
import { cn } from '@/utils/cn';
import { useWorkspaceStore } from '@/store/workspaceStore';
import { useSplitStore } from '@/store/splitStore';
import { getFilename } from '@/utils';
import { SPLIT_MODES_DEFINITIONS } from '@/constants';

export function SplitScreen() {
  const sectionsCollapsed = useWorkspaceStore((s) => s.sectionsCollapsed);
  const toggleSection = useWorkspaceStore((s) => s.toggleSection);

  const inputFile = useSplitStore((s) => s.inputFile);
  const splitMode = useSplitStore((s) => s.splitMode);
  const namingConfig = useSplitStore((s) => s.namingConfig);
  const currentPlan = useSplitStore((s) => s.currentPlan);
  const jobs = useSplitStore((s) => s.jobs);

  const activeModeLabel = SPLIT_MODES_DEFINITIONS.find(m => m.value === splitMode)?.label || splitMode;
  const activeNamingLabel = namingConfig.mode;

  return (
    <div className="flex-1 flex flex-col min-h-0 bg-bg-base/10">
      <div className="flex-1 overflow-y-auto custom-scrollbar">
        <div className="w-full p-6 space-y-5">
          {/* Header */}
          <div className="flex items-center gap-3 pb-5 border-b border-border/80">
            <div className="w-9 h-9 rounded-xl bg-accent-500/10 border border-accent-500/20 flex items-center justify-center shadow-glow-sm">
              <Scissors className="w-4.5 h-4.5 text-accent-400" />
            </div>
            <div>
              <h2 className="text-sm font-semibold text-text-primary tracking-wide">Video Split Engine</h2>
              <p className="text-[11px] text-text-muted mt-0.5">
                Lossless frame-accurate division of media files into multiple parts
              </p>
            </div>
          </div>

          {/* Input Section */}
          <div className="glass rounded-xl overflow-hidden shadow-card hover:border-border-strong transition-all duration-300">
            <button
              onClick={() => toggleSection('encoding')}
              className={cn(
                "flex items-center justify-between w-full px-4 py-3.5 text-left transition-colors duration-150 bg-bg-surface/40 hover:bg-bg-surface/70 border-b border-transparent",
                !sectionsCollapsed.encoding && "border-border/60 bg-bg-surface/60"
              )}
            >
              <div className="flex items-center gap-2.5 min-w-0">
                <FileVideo className="w-4 h-4 text-accent-400 shrink-0" />
                <div className="min-w-0">
                  <span className="text-xs font-semibold text-text-secondary uppercase tracking-wider block">
                    Input Source File
                  </span>
                  {inputFile && !sectionsCollapsed.encoding && (
                    <span className="text-[10px] text-text-muted truncate block mt-0.5 max-w-[400px]" title={inputFile}>
                      {getFilename(inputFile)}
                    </span>
                  )}
                </div>
              </div>
              <div className="flex items-center gap-2">
                {sectionsCollapsed.encoding && inputFile && (
                  <span className="text-[9px] bg-accent-500/10 border border-accent-500/20 text-accent-400 px-2 py-0.5 rounded font-mono font-bold uppercase tracking-wider">
                    Selected
                  </span>
                )}
                {sectionsCollapsed.encoding ? (
                  <ChevronRight className="w-4 h-4 text-text-muted" />
                ) : (
                  <ChevronDown className="w-4 h-4 text-text-muted" />
                )}
              </div>
            </button>
            <motion.div
              animate={{
                height: sectionsCollapsed.encoding ? 0 : 'auto',
                opacity: sectionsCollapsed.encoding ? 0 : 1,
              }}
              transition={{ duration: 0.2, ease: 'easeInOut' }}
              className="overflow-hidden"
            >
              <div className="p-4 bg-bg-elevated/10">
                <SplitInputPanel />
              </div>
            </motion.div>
          </div>

          {/* Mode Selection */}
          <div className="glass rounded-xl overflow-hidden shadow-card hover:border-border-strong transition-all duration-300">
            <button
              onClick={() => toggleSection('advanced')}
              className={cn(
                "flex items-center justify-between w-full px-4 py-3.5 text-left transition-colors duration-150 bg-bg-surface/40 hover:bg-bg-surface/70 border-b border-transparent",
                !sectionsCollapsed.advanced && "border-border/60 bg-bg-surface/60"
              )}
            >
              <div className="flex items-center gap-2.5 min-w-0">
                <Sliders className="w-4 h-4 text-accent-400 shrink-0" />
                <div className="min-w-0">
                  <span className="text-xs font-semibold text-text-secondary uppercase tracking-wider block">
                    Split Mode & Rules
                  </span>
                  {!sectionsCollapsed.advanced && (
                    <span className="text-[10px] text-text-muted block mt-0.5">
                      Configure segment creation parameters
                    </span>
                  )}
                </div>
              </div>
              <div className="flex items-center gap-2">
                {sectionsCollapsed.advanced && (
                  <span className="text-[9px] bg-accent-500/15 border border-accent-500/20 text-accent-300 px-2 py-0.5 rounded font-mono font-bold uppercase tracking-wider">
                    {activeModeLabel}
                  </span>
                )}
                {sectionsCollapsed.advanced ? (
                  <ChevronRight className="w-4 h-4 text-text-muted" />
                ) : (
                  <ChevronDown className="w-4 h-4 text-text-muted" />
                )}
              </div>
            </button>
            <motion.div
              animate={{
                height: sectionsCollapsed.advanced ? 0 : 'auto',
                opacity: sectionsCollapsed.advanced ? 0 : 1,
              }}
              transition={{ duration: 0.2, ease: 'easeInOut' }}
              className="overflow-hidden"
            >
              <div className="p-4 bg-bg-elevated/10">
                <SplitModeSelector />
              </div>
            </motion.div>
          </div>

          {/* Output Naming */}
          <div className="glass rounded-xl overflow-hidden shadow-card hover:border-border-strong transition-all duration-300">
            <button
              onClick={() => toggleSection('naming')}
              className={cn(
                "flex items-center justify-between w-full px-4 py-3.5 text-left transition-colors duration-150 bg-bg-surface/40 hover:bg-bg-surface/70 border-b border-transparent",
                !sectionsCollapsed.naming && "border-border/60 bg-bg-surface/60"
              )}
            >
              <div className="flex items-center gap-2.5 min-w-0">
                <Type className="w-4 h-4 text-accent-400 shrink-0" />
                <div className="min-w-0">
                  <span className="text-xs font-semibold text-text-secondary uppercase tracking-wider block">
                    Output Naming
                  </span>
                  {!sectionsCollapsed.naming && (
                    <span className="text-[10px] text-text-muted block mt-0.5">
                      Template-based filename generation for split outputs
                    </span>
                  )}
                </div>
              </div>
              <div className="flex items-center gap-2">
                {sectionsCollapsed.naming && (
                  <span className="text-[9px] bg-accent-500/10 border border-accent-500/20 text-accent-400 px-2 py-0.5 rounded font-mono font-bold uppercase tracking-wider">
                    {activeNamingLabel}
                  </span>
                )}
                {sectionsCollapsed.naming ? (
                  <ChevronRight className="w-4 h-4 text-text-muted" />
                ) : (
                  <ChevronDown className="w-4 h-4 text-text-muted" />
                )}
              </div>
            </button>
            <motion.div
              animate={{
                height: sectionsCollapsed.naming ? 0 : 'auto',
                opacity: sectionsCollapsed.naming ? 0 : 1,
              }}
              transition={{ duration: 0.2, ease: 'easeInOut' }}
              className="overflow-hidden"
            >
              <div className="p-4 bg-bg-elevated/10">
                <NamingPanel />
              </div>
            </motion.div>
          </div>

          {/* Preview Section */}
          <div className="glass rounded-xl overflow-hidden shadow-card hover:border-border-strong transition-all duration-300">
            <button
              onClick={() => toggleSection('summary')}
              className={cn(
                "flex items-center justify-between w-full px-4 py-3.5 text-left transition-colors duration-150 bg-bg-surface/40 hover:bg-bg-surface/70 border-b border-transparent",
                !sectionsCollapsed.summary && "border-border/60 bg-bg-surface/60"
              )}
            >
              <div className="flex items-center gap-2.5 min-w-0">
                <Eye className="w-4 h-4 text-accent-400 shrink-0" />
                <div className="min-w-0">
                  <span className="text-xs font-semibold text-text-secondary uppercase tracking-wider block">
                    Plan Preview
                  </span>
                  {!sectionsCollapsed.summary && (
                    <span className="text-[10px] text-text-muted block mt-0.5">
                      Verify generated timeline segment cuts
                    </span>
                  )}
                </div>
              </div>
              <div className="flex items-center gap-2">
                {sectionsCollapsed.summary && currentPlan && (
                  <span className="text-[9px] bg-success-muted text-success px-2 py-0.5 rounded font-mono font-bold uppercase tracking-wider">
                    {currentPlan.segments.length} segments
                  </span>
                )}
                {sectionsCollapsed.summary ? (
                  <ChevronRight className="w-4 h-4 text-text-muted" />
                ) : (
                  <ChevronDown className="w-4 h-4 text-text-muted" />
                )}
              </div>
            </button>
            <motion.div
              animate={{
                height: sectionsCollapsed.summary ? 0 : 'auto',
                opacity: sectionsCollapsed.summary ? 0 : 1,
              }}
              transition={{ duration: 0.2, ease: 'easeInOut' }}
              className="overflow-hidden"
            >
              <div className="p-4 bg-bg-elevated/10">
                <SplitPreview />
              </div>
            </motion.div>
          </div>

          {/* Action Bar */}
          <div className="glass rounded-xl p-4 flex items-center justify-between gap-4 border border-border shadow-card">
            <div className="flex items-start gap-2.5 min-w-0">
              <Info className="w-4 h-4 text-text-muted shrink-0 mt-0.5" />
              <p className="text-[10px] text-text-muted leading-relaxed">
                Segments are split losslessly when possible (stream copy).<br />
                Falls back to re-encode if stream copy is not supported.
              </p>
            </div>
            <SplitActionBar />
          </div>

          {/* Task Queue */}
          <div className="glass rounded-xl overflow-hidden shadow-card hover:border-border-strong transition-all duration-300">
            <button
              onClick={() => toggleSection('tasks')}
              className={cn(
                "flex items-center justify-between w-full px-4 py-3.5 text-left transition-colors duration-150 bg-bg-surface/40 hover:bg-bg-surface/70 border-b border-transparent",
                !sectionsCollapsed.tasks && "border-border/60 bg-bg-surface/60"
              )}
            >
              <div className="flex items-center gap-2.5 min-w-0">
                <HardDrive className="w-4 h-4 text-accent-400 shrink-0" />
                <div className="min-w-0">
                  <span className="text-xs font-semibold text-text-secondary uppercase tracking-wider block">
                    Split Task Manager
                  </span>
                  {!sectionsCollapsed.tasks && (
                    <span className="text-[10px] text-text-muted block mt-0.5">
                      Monitor active and completed split tasks
                    </span>
                  )}
                </div>
              </div>
              <div className="flex items-center gap-2">
                {sectionsCollapsed.tasks && jobs.length > 0 && (
                  <span className="text-[9px] bg-accent-500/10 border border-accent-500/20 text-accent-400 px-2 py-0.5 rounded font-mono font-bold">
                    {jobs.length}
                  </span>
                )}
                {sectionsCollapsed.tasks ? (
                  <ChevronRight className="w-4 h-4 text-text-muted" />
                ) : (
                  <ChevronDown className="w-4 h-4 text-text-muted" />
                )}
              </div>
            </button>
            <motion.div
              animate={{
                height: sectionsCollapsed.tasks ? 0 : 'auto',
                opacity: sectionsCollapsed.tasks ? 0 : 1,
              }}
              transition={{ duration: 0.2, ease: 'easeInOut' }}
              className="overflow-hidden"
            >
              <div className="p-4 bg-bg-elevated/10">
                <SplitTaskQueue />
              </div>
            </motion.div>
          </div>
        </div>
      </div>
    </div>
  );
}
