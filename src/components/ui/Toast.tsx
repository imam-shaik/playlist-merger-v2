// ────────────────────────────────────────────────
// Toast — Queue-based Notification System
//
// Supports multiple simultaneous toasts with
// auto-dismiss, manual dismiss, and stacking.
// ────────────────────────────────────────────────

import React, { useEffect, useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { CheckCircle2, XCircle, AlertTriangle, Info, X, ChevronDown, ChevronUp } from 'lucide-react';
import { cn } from '@/utils/cn';
import { useAppStore, type ToastMessage } from '@/store/appStore';

const ICONS = {
  success: CheckCircle2,
  error: XCircle,
  warning: AlertTriangle,
  info: Info,
};

const COLORS = {
  success: 'border-success/30 bg-success/5 text-success',
  error: 'border-danger/30 bg-danger/5 text-danger',
  warning: 'border-warning/30 bg-warning/5 text-warning',
  info: 'border-accent-500/30 bg-accent-500/5 text-accent-400',
};

function ToastItem({ toast }: { toast: ToastMessage }) {
  const dismissToast = useAppStore((s) => s.dismissToast);
  const Icon = ICONS[toast.type];
  const duration = toast.durationMs ?? 4000;
  const [showDetails, setShowDetails] = useState(false);

  useEffect(() => {
    const timer = setTimeout(() => dismissToast(toast.id), duration);
    return () => clearTimeout(timer);
  }, [toast.id, dismissToast, duration]);

  return (
    <motion.div
      initial={{ opacity: 0, y: 20, scale: 0.95 }}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      exit={{ opacity: 0, y: 8, scale: 0.95 }}
      transition={{ duration: 0.2, ease: [0.16, 1, 0.3, 1] }}
      className={cn(
        'flex items-start gap-3 p-3 pr-2',
        'border rounded-lg shadow-modal backdrop-blur-md',
        'bg-bg-elevated/90',
        'min-w-[280px] max-w-[400px]',
        COLORS[toast.type]
      )}
      role="alert"
    >
      <Icon className="w-4 h-4 mt-0.5 shrink-0" aria-hidden="true" />
      <div className="flex-1 min-w-0">
        <p className="text-sm font-medium text-text-primary">{toast.title}</p>
        {toast.description && (
          <p className="text-xs text-text-muted mt-0.5">{toast.description}</p>
        )}
        {toast.technicalDetails && (
          <>
            <button
              onClick={() => setShowDetails(!showDetails)}
              className="flex items-center gap-1 text-xs text-text-muted/60 hover:text-text-muted mt-1 transition-colors"
            >
              {showDetails ? <ChevronUp className="w-3 h-3" /> : <ChevronDown className="w-3 h-3" />}
              {showDetails ? 'Hide technical details' : 'Show technical details'}
            </button>
            {showDetails && (
              <pre className="mt-1 text-[11px] text-text-muted/50 bg-bg-base/50 rounded p-1.5 overflow-x-auto whitespace-pre-wrap max-h-24 overflow-y-auto">
                {toast.technicalDetails}
              </pre>
            )}
          </>
        )}
      </div>
      <button
        onClick={() => dismissToast(toast.id)}
        className="p-0.5 rounded text-text-muted hover:text-text-secondary transition-colors shrink-0"
        aria-label="Dismiss notification"
      >
        <X className="w-3.5 h-3.5" />
      </button>
    </motion.div>
  );
}

export function ToastContainer() {
  const toasts = useAppStore((s) => s.toasts);

  if (toasts.length === 0) return null;

  return (
    <div
      className="fixed bottom-5 right-5 z-50 flex flex-col gap-2 pointer-events-none"
      aria-live="polite"
      aria-label="Notifications"
    >
      <AnimatePresence mode="popLayout">
        {toasts.map((toast) => (
          <div key={toast.id} className="pointer-events-auto">
            <ToastItem toast={toast} />
          </div>
        ))}
      </AnimatePresence>
    </div>
  );
}
