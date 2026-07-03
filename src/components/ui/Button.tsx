import React from 'react';
import { cn } from '@/utils/cn';

export type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'danger' | 'outline';
export type ButtonSize = 'xs' | 'sm' | 'md' | 'lg';

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  loading?: boolean;
  leftIcon?: React.ReactNode;
  rightIcon?: React.ReactNode;
}

const variantStyles: Record<ButtonVariant, string> = {
  primary: [
    'bg-accent-600 hover:bg-accent-500 active:bg-accent-700',
    'text-white font-medium',
    'border border-accent-500/50',
    'shadow-glow-sm hover:shadow-glow',
  ].join(' '),
  secondary: [
    'bg-bg-elevated hover:bg-bg-overlay active:bg-bg-elevated',
    'text-text-primary',
    'border border-border',
    'hover:border-border-strong',
  ].join(' '),
  ghost: [
    'bg-transparent hover:bg-bg-elevated active:bg-bg-surface',
    'text-text-secondary hover:text-text-primary',
  ].join(' '),
  danger: [
    'bg-danger/15 hover:bg-danger/25 active:bg-danger/20',
    'text-danger border border-danger/30 hover:border-danger/50',
  ].join(' '),
  outline: [
    'bg-transparent hover:bg-bg-elevated',
    'text-text-secondary hover:text-text-primary',
    'border border-border hover:border-border-strong',
  ].join(' '),
};

const sizeStyles: Record<ButtonSize, string> = {
  xs: 'h-6 px-2 text-2xs rounded-xs gap-1',
  sm: 'h-7 px-3 text-xs rounded-sm gap-1.5',
  md: 'h-8 px-4 text-sm rounded gap-2',
  lg: 'h-10 px-5 text-sm rounded-md gap-2',
};

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  (
    {
      variant = 'secondary',
      size = 'md',
      loading = false,
      leftIcon,
      rightIcon,
      children,
      className,
      disabled,
      ...props
    },
    ref
  ) => {
    return (
      <button
        ref={ref}
        disabled={disabled || loading}
        className={cn(
          'inline-flex items-center justify-center font-sans',
          'transition-all duration-150',
          'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-500/50',
          'disabled:opacity-40 disabled:cursor-not-allowed disabled:pointer-events-none',
          'select-none',
          variantStyles[variant],
          sizeStyles[size],
          className
        )}
        {...props}
      >
        {loading ? (
          <span className="w-3.5 h-3.5 rounded-full border-2 border-current border-t-transparent animate-spin" />
        ) : (
          leftIcon && <span className="shrink-0">{leftIcon}</span>
        )}
        {children && <span>{children}</span>}
        {rightIcon && !loading && <span className="shrink-0">{rightIcon}</span>}
      </button>
    );
  }
);

Button.displayName = 'Button';
