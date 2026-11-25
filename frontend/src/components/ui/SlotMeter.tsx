import React from 'react';

export function SlotMeter({
  used,
  max,
  label,
  showCounts = false,
  className = '',
}: {
  used: number;
  max: number;
  label: string;
  showCounts?: boolean;
  className?: string;
}) {
  const safeMax = Math.max(1, max);
  const clampedUsed = Math.max(0, Math.min(used, safeMax));
  const pct = (clampedUsed / safeMax) * 100;
  const fillColor =
    pct >= 98
      ? 'var(--color-danger)'
      : pct >= 85
        ? 'var(--color-warning)'
        : 'var(--color-action)';

  return (
    <div className={`space-y-1 ${className}`}>
      {showCounts ? (
        <div className="flex items-baseline justify-between gap-2 text-xs tabular-nums">
          <span className="font-medium text-text">
            {used} <span className="text-text-muted">/ {max}</span>
          </span>
          <span className="text-text-muted">{Math.round(pct)}%</span>
        </div>
      ) : null}
      <progress
        className="tss-progress"
        value={clampedUsed}
        max={safeMax}
        aria-label={label}
        style={{ '--progress-bar-fill': fillColor } as React.CSSProperties}
      />
    </div>
  );
}
