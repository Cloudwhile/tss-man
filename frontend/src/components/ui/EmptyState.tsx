import React from 'react';

export function EmptyState({
  icon: Icon,
  title,
  description,
  action,
  compact = false,
}: {
  icon: React.ComponentType<{ className?: string }>;
  title: string;
  description: string;
  action?: React.ReactNode;
  compact?: boolean;
}) {
  return (
    <div
      className={`rounded-[var(--radius-lg)] border border-dashed border-border-strong bg-surface-sunken/35 px-6 text-center ${
        compact ? 'py-8' : 'py-14'
      }`}
    >
      <div className="w-11 h-11 mx-auto rounded-[var(--radius-DEFAULT)] bg-surface-raised border border-border shadow-[var(--shadow-sm)] text-action grid place-items-center">
        <Icon className="w-5 h-5" aria-hidden="true" />
      </div>
      <h2 className="mt-3.5 text-sm font-semibold text-text">{title}</h2>
      <p className="mt-1 text-xs sm:text-sm text-text-muted max-w-[46ch] mx-auto leading-relaxed tss-prose">
        {description}
      </p>
      {action ? <div className="mt-4 flex flex-wrap justify-center gap-2.5">{action}</div> : null}
    </div>
  );
}
