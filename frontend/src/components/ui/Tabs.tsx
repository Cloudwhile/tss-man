import React, { useRef } from 'react';

export interface TabItem<T extends string> {
  id: T;
  label: string;
  count?: number;
}

export function TabBar<T extends string>({
  ariaLabel,
  items,
  active,
  onChange,
  idPrefix,
  rightSlot,
}: {
  ariaLabel: string;
  items: TabItem<T>[];
  active: T;
  onChange: (id: T) => void;
  idPrefix: string;
  rightSlot?: React.ReactNode;
}) {
  const btnRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const safeAnchor = `--tss-tab-${idPrefix.replace(/[^a-zA-Z0-9_-]/g, '') || 'main'}`;

  const handleKeyDown = (e: React.KeyboardEvent<HTMLButtonElement>, idx: number) => {
    let next = -1;
    if (e.key === 'ArrowRight') next = (idx + 1) % items.length;
    else if (e.key === 'ArrowLeft') next = (idx - 1 + items.length) % items.length;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = items.length - 1;
    if (next < 0) return;

    e.preventDefault();
    const target = items[next];
    onChange(target.id);
    const el = btnRefs.current[next];
    el?.focus();
    el?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  };

  return (
    <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border">
      <div
        role="tablist"
        aria-label={ariaLabel}
        style={{ '--tab-anchor': safeAnchor } as React.CSSProperties}
        className="tss-tablist flex-1 min-w-0"
      >
        {items.map((item, idx) => {
          const isSelected = item.id === active;
          return (
            <button
              key={item.id}
              ref={(el) => {
                btnRefs.current[idx] = el;
              }}
              id={`${idPrefix}-tab-${item.id}`}
              type="button"
              role="tab"
              aria-selected={isSelected}
              tabIndex={isSelected ? 0 : -1}
              onKeyDown={(e) => handleKeyDown(e, idx)}
              onClick={(e) => {
                onChange(item.id);
                e.currentTarget.scrollIntoView({ block: 'nearest', inline: 'nearest' });
              }}
              className="tss-tab"
            >
              <span>{item.label}</span>
              {typeof item.count === 'number' ? (
                <span
                  className={`px-1.5 py-0.5 rounded-[var(--radius-sm)] font-mono text-[11px] tabular-nums leading-none tss-trim ${
                    isSelected
                      ? 'bg-action-subtle text-action-text font-semibold'
                      : 'bg-surface-sunken text-text-muted'
                  }`}
                >
                  {item.count}
                </span>
              ) : null}
            </button>
          );
        })}
      </div>
      {rightSlot ? <div className="pb-1.5 shrink-0">{rightSlot}</div> : null}
    </div>
  );
}
