// ────────────────────────────────────────────────
// WorkspaceLayout — Resizable Multi-Panel Layout
//
// Provides a desktop-native resizable panel layout
// with persistent widths, collapse/expand support.
// Uses react-resizable-panels (Group / Panel / Separator).
// ────────────────────────────────────────────────

import React from 'react';
import { Group, Panel, Separator } from 'react-resizable-panels';
import { cn } from '@/utils/cn';
import { useWorkspaceStore } from '@/store/workspaceStore';

interface WorkspaceLayoutProps {
  /** Left (sidebar) panel — accepts null to hide */
  sidebar: React.ReactNode;
  /** Main content panel */
  main: React.ReactNode;
  /** Right (inspector/merge) panel — accepts null to hide */
  inspector?: React.ReactNode;
  /** Optional class name for the container */
  className?: string;
}

export function WorkspaceLayout({ sidebar, main, inspector, className }: WorkspaceLayoutProps) {
  const mergePanelCollapsed = useWorkspaceStore((s) => s.mergePanelCollapsed);
  const toggleMergePanel = useWorkspaceStore((s) => s.toggleMergePanel);

  return (
    <div className={cn('flex-1 flex min-h-0', className)}>
      <Group id="workspace">
        {/* Sidebar */}
        {sidebar && (
          <>
            <Panel id="sidebar" defaultSize="56px" minSize="56px" maxSize="56px">
              {sidebar}
            </Panel>
            <div className="w-px bg-border shrink-0" />
          </>
        )}

        {/* Main content */}
        <Panel id="main" minSize="30%">
          {main}
        </Panel>

        {/* Inspector (merge panel) */}
        {inspector && !mergePanelCollapsed && (
          <>
            <Separator className="w-px bg-border hover:bg-accent-500/40 transition-colors cursor-col-resize data-[resize-handle-active]:bg-accent-500/60" />
            <Panel id="inspector" defaultSize="320px" minSize="260px" maxSize="450px" collapsible collapsedSize="0px">
              <div className="h-full flex flex-col">
                {/* Collapse button */}
                <div className="flex items-center justify-end px-2 py-1 border-b border-border/40">
                  <button
                    onClick={toggleMergePanel}
                    className="flex items-center gap-1 text-2xs text-text-muted hover:text-text-secondary transition-colors"
                    aria-label="Collapse merge panel"
                    title="Collapse merge panel"
                  >
                    <svg
                      width="12"
                      height="12"
                      viewBox="0 0 12 12"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="1.5"
                      strokeLinecap="round"
                    >
                      <path d="M7 3L4 6L7 9" />
                    </svg>
                    Hide
                  </button>
                </div>
                <div className="flex-1 min-h-0 overflow-hidden">
                  {inspector}
                </div>
              </div>
            </Panel>
          </>
        )}
      </Group>

      {/* When inspector is collapsed, show a toggle button */}
      {inspector && mergePanelCollapsed && (
        <div className="flex items-center border-l border-border">
          <button
            onClick={toggleMergePanel}
            className="flex items-center gap-1 px-2 py-1 text-2xs text-text-muted hover:text-accent-400 hover:bg-accent-muted transition-colors rounded-l"
            aria-label="Show merge panel"
            title="Show merge panel"
          >
            <svg
              width="12"
              height="12"
              viewBox="0 0 12 12"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
            >
              <path d="M5 3L8 6L5 9" />
            </svg>
            Merge
          </button>
        </div>
      )}
    </div>
  );
}
