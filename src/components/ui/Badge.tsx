import React from 'react';
import { cn } from '@/utils/cn';

// ── Badge ────────────────────────────────────────

type BadgeVariant = 'default' | 'success' | 'warning' | 'danger' | 'info' | 'muted';

interface BadgeProps {
  variant?: BadgeVariant;
  children: React.ReactNode;
  className?: string;
  title?: string;
}

const badgeVariants: Record<BadgeVariant, string> = {
  default: 'bg-accent-muted text-accent-300 border-accent-500/20',
  success: 'bg-success/10 text-success border-success/20',
  warning: 'bg-warning/10 text-warning border-warning/20',
  danger: 'bg-danger/10 text-danger border-danger/20',
  info: 'bg-highlight/10 text-highlight border-highlight/20',
  muted: 'bg-bg-elevated text-text-muted border-border',
};

export function Badge({ variant = 'default', children, className, title }: BadgeProps) {
  return (
    <span
      title={title}
      className={cn(
        'inline-flex items-center gap-1 px-1.5 py-0.5',
        'text-2xs font-medium rounded-xs border',
        'select-none',
        badgeVariants[variant],
        className
      )}
    >
      {children}
    </span>
  );
}

// ── Kbd ─────────────────────────────────────────

interface KbdProps {
  children: React.ReactNode;
  className?: string;
}

export function Kbd({ children, className }: KbdProps) {
  return (
    <kbd
      className={cn(
        'inline-flex items-center justify-center',
        'px-1.5 py-0.5 text-2xs font-mono',
        'bg-bg-elevated border border-border rounded-xs',
        'text-text-muted',
        className
      )}
    >
      {children}
    </kbd>
  );
}

// ── Separator ────────────────────────────────────

interface SeparatorProps {
  orientation?: 'horizontal' | 'vertical';
  className?: string;
}

export function Separator({ orientation = 'horizontal', className }: SeparatorProps) {
  return (
    <div
      role="separator"
      className={cn(
        'bg-border',
        orientation === 'horizontal' ? 'h-px w-full' : 'w-px h-full',
        className
      )}
    />
  );
}

// ── EmptyState ───────────────────────────────────

interface EmptyStateProps {
  icon?: React.ReactNode;
  title: string;
  description?: string;
  action?: React.ReactNode;
}

export function EmptyState({ icon, title, description, action }: EmptyStateProps) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 py-16 px-8 text-center">
      {icon && (
        <div className="w-12 h-12 rounded-xl bg-bg-elevated border border-border flex items-center justify-center text-text-muted">
          {icon}
        </div>
      )}
      <div className="flex flex-col gap-1">
        <p className="text-sm font-medium text-text-primary">{title}</p>
        {description && (
          <p className="text-xs text-text-muted max-w-xs">{description}</p>
        )}
      </div>
      {action}
    </div>
  );
}

// ── Spinner ──────────────────────────────────────

interface SpinnerProps {
  size?: 'xs' | 'sm' | 'md' | 'lg';
  className?: string;
}

const spinnerSizes = { xs: 'w-3 h-3', sm: 'w-4 h-4', md: 'w-5 h-5', lg: 'w-6 h-6' };

export function Spinner({ size = 'md', className }: SpinnerProps) {
  return (
    <span
      className={cn(
        spinnerSizes[size],
        'rounded-full border-2 border-current border-t-transparent animate-spin',
        className
      )}
    />
  );
}

// ── Tag (codec/format labels) ────────────────────

interface TagProps {
  children: React.ReactNode;
  className?: string;
}

export function Tag({ children, className }: TagProps) {
  return (
    <span
      className={cn(
        'inline-flex items-center px-1.5 py-0.5',
        'text-2xs font-mono rounded-xs',
        'bg-bg-elevated border border-border text-text-muted',
        className
      )}
    >
      {children}
    </span>
  );
}
