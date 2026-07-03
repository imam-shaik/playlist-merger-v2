// ────────────────────────────────────────────────
// FileHealthBadge — Visual indicator for file health status.
// Shows a small colored badge with tooltip explaining the health state.
// ────────────────────────────────────────────────

import React from 'react';
import { CheckCircle2, AlertTriangle, AlertCircle, XCircle, HelpCircle } from 'lucide-react';
import { cn } from '@/utils/cn';
import type { FileHealthStatus } from '@/types';

interface FileHealthBadgeProps {
  status: FileHealthStatus | null | undefined;
  confidence?: number;
  message?: string;
  technicalDetails?: string;
  /** Show only the colored dot (for compact displays) */
  compact?: boolean;
  className?: string;
}

const STATUS_CONFIG: Record<FileHealthStatus, {
  label: string;
  color: string;
  bg: string;
  border: string;
  icon: React.FC<{ className?: string }>;
  severity: 'success' | 'warning' | 'danger' | 'muted';
}> = {
  healthy: {
    label: 'Healthy',
    color: 'text-success',
    bg: 'bg-success/10',
    border: 'border-success/20',
    icon: CheckCircle2,
    severity: 'success',
  },
  healthy_with_warnings: {
    label: 'Minor Warnings',
    color: 'text-warning',
    bg: 'bg-warning/10',
    border: 'border-warning/20',
    icon: AlertTriangle,
    severity: 'warning',
  },
  seekability_issue: {
    label: 'Seek Issue',
    color: 'text-warning',
    bg: 'bg-warning/10',
    border: 'border-warning/20',
    icon: AlertTriangle,
    severity: 'warning',
  },
  minor_metadata_issue: {
    label: 'Metadata Issue',
    color: 'text-warning',
    bg: 'bg-warning/10',
    border: 'border-warning/20',
    icon: AlertCircle,
    severity: 'warning',
  },
  corrupted: {
    label: 'Corrupted',
    color: 'text-danger',
    bg: 'bg-danger/10',
    border: 'border-danger/20',
    icon: XCircle,
    severity: 'danger',
  },
  unreadable: {
    label: 'Unreadable',
    color: 'text-danger',
    bg: 'bg-danger/10',
    border: 'border-danger/20',
    icon: XCircle,
    severity: 'danger',
  },
} as const;

/** Fallback config for unknown/unrecognized status strings from the backend */
const UNKNOWN_CONFIG = {
  label: 'Unknown',
  color: 'text-text-muted',
  bg: 'bg-bg-overlay',
  border: 'border-border/40',
  icon: HelpCircle,
  severity: 'muted' as const,
};

/** Safe lookup — returns config for recognized statuses, or UNKNOWN_CONFIG for anything unexpected */
function getStatusConfig(status: string) {
  if (status in STATUS_CONFIG) {
    return STATUS_CONFIG[status as FileHealthStatus];
  }
  return UNKNOWN_CONFIG;
}

export function FileHealthBadge({ status, confidence, message, technicalDetails, compact, className }: FileHealthBadgeProps) {
  if (!status) {
    return (
      <span
        className={cn(
          'inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[9px] font-medium',
          UNKNOWN_CONFIG.bg, UNKNOWN_CONFIG.border, UNKNOWN_CONFIG.color,
          className,
        )}
        title="Health not yet checked"
      >
        <UNKNOWN_CONFIG.icon className="w-2.5 h-2.5" />
        {!compact && <span>Unknown</span>}
      </span>
    );
  }

  const cfg = getStatusConfig(status);

  if (compact) {
    return (
      <span
        className={cn(
          'inline-flex items-center justify-center w-3.5 h-3.5 rounded-full border',
          cfg.bg, cfg.border,
          className,
        )}
        title={`${cfg.label}${message ? `: ${message}` : ''}`}
      >
        <cfg.icon className={cn('w-2 h-2', cfg.color)} />
      </span>
    );
  }

  return (
    <span
      className={cn(
        'inline-flex items-center gap-1.5 px-2 py-1 rounded-lg border text-[10px] font-medium',
        cfg.bg, cfg.border, cfg.color,
        className,
      )}
      title={technicalDetails ? `${message}\n\nTechnical: ${technicalDetails}` : message}
    >
      <cfg.icon className="w-3 h-3 shrink-0" />
      <span>{cfg.label}</span>
      {confidence !== undefined && confidence < 100 && (
        <span className="opacity-60 text-[9px]">({confidence}%)</span>
      )}
    </span>
  );
}
