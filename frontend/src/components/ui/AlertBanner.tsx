import React from 'react';
import { AlertCircle, CheckCircle2, Info, TriangleAlert, X } from 'lucide-react';

export type AlertTone = 'danger' | 'warning' | 'success' | 'info';

const TONE_CLASSES: Record<AlertTone, string> = {
  danger: 'border-danger bg-danger-subtle text-danger-text',
  warning: 'border-warning bg-warning-subtle text-warning-text',
  success: 'border-success bg-success-subtle text-success-text',
  info: 'border-action bg-action-subtle text-action-text',
};

const TONE_ICONS: Record<AlertTone, React.ComponentType<{ className?: string }>> = {
  danger: AlertCircle,
  warning: TriangleAlert,
  success: CheckCircle2,
  info: Info,
};

export function AlertBanner({
  tone = 'info',
  title,
  onDismiss,
  children,
  className = '',
}: {
  tone?: AlertTone;
  title?: string;
  onDismiss?: () => void;
  children: React.ReactNode;
  className?: string;
}) {
  const Icon = TONE_ICONS[tone];
  return (
    <div
      role={tone === 'danger' ? 'alert' : 'status'}
      className={`border-l-4 rounded-r-[var(--radius-DEFAULT)] px-4 py-3 flex items-start gap-3 ${TONE_CLASSES[tone]} ${className}`}
    >
      <Icon className="w-5 h-5 shrink-0 mt-0.5" aria-hidden="true" />
      <div className="min-w-0 flex-1 text-sm leading-normal">
        {title ? <p className="font-semibold">{title}</p> : null}
        <div>{children}</div>
      </div>
      {onDismiss ? (
        <button
          type="button"
          onClick={onDismiss}
          aria-label="Dismiss"
          className="shrink-0 w-7 h-7 inline-flex items-center justify-center rounded-[var(--radius-sm)] opacity-75 hover:opacity-100 cursor-pointer"
        >
          <X className="w-4 h-4" aria-hidden="true" />
        </button>
      ) : null}
    </div>
  );
}
