import { CheckCircle2, Clock, WifiOff } from 'lucide-react';

export type BadgeTone = 'success' | 'warning' | 'danger' | 'neutral' | 'action';

const TAG_TONES: Record<BadgeTone, string> = {
  success: 'bg-success-subtle text-success-text border border-success/20',
  warning: 'bg-warning-subtle text-warning-text border border-warning/25',
  danger: 'bg-danger-subtle text-danger-text border border-danger/20',
  action: 'bg-action-subtle text-action-text border border-action/20',
  neutral: 'bg-surface-sunken text-text-secondary border border-border',
};

const DOT_TONES: Record<BadgeTone, string> = {
  success: 'bg-success',
  warning: 'bg-warning',
  danger: 'bg-danger',
  action: 'bg-action',
  neutral: 'bg-text-muted',
};

export function StatusBadge({
  tone,
  label,
  icon = 'auto',
  variant = 'tag',
}: {
  tone: BadgeTone;
  label: string;
  icon?: 'online' | 'offline' | 'pending' | 'none' | 'auto';
  variant?: 'tag' | 'dot';
}) {
  const resolved =
    icon === 'auto'
      ? tone === 'success'
        ? 'online'
        : tone === 'danger'
          ? 'offline'
          : tone === 'warning'
            ? 'pending'
            : 'none'
      : icon;

  if (variant === 'dot') {
    return (
      <span className="inline-flex items-center gap-1.5 text-xs font-medium text-text-secondary whitespace-nowrap">
        <span
          className={`w-1.5 h-1.5 rounded-full shrink-0 ${DOT_TONES[tone]}`}
          aria-hidden="true"
        />
        {resolved === 'offline' ? (
          <WifiOff className="w-3.5 h-3.5 text-danger shrink-0" aria-hidden="true" />
        ) : resolved === 'pending' ? (
          <Clock className="w-3.5 h-3.5 text-warning shrink-0" aria-hidden="true" />
        ) : null}
        <span className="tss-trim">{label}</span>
      </span>
    );
  }

  return (
    <span
      className={`inline-flex items-center gap-1.5 min-h-5 px-2 py-0.5 rounded-[var(--radius-sm)] text-xs font-medium whitespace-nowrap ${TAG_TONES[tone]}`}
    >
      {resolved === 'online' ? <CheckCircle2 className="w-3 h-3 shrink-0" aria-hidden="true" /> : null}
      {resolved === 'offline' ? <WifiOff className="w-3 h-3 shrink-0" aria-hidden="true" /> : null}
      {resolved === 'pending' ? <Clock className="w-3 h-3 shrink-0" aria-hidden="true" /> : null}
      <span className="tss-trim">{label}</span>
    </span>
  );
}
