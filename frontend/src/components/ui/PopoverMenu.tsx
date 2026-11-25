import React, { useEffect, useId, useRef } from 'react';
import { MoreHorizontal } from 'lucide-react';

export interface PopoverItem {
  label: string;
  onClick: () => void;
  danger?: boolean;
  icon?: React.ReactNode;
  dividerBefore?: boolean;
}

export function PopoverMenu({
  triggerLabel,
  triggerIcon,
  iconOnly = false,
  items,
}: {
  triggerLabel: string;
  triggerIcon?: React.ReactNode;
  iconOnly?: boolean;
  items: PopoverItem[];
}) {
  const panelId = useId();
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const panel = panelRef.current;
    const trigger = triggerRef.current;
    if (!panel || !trigger) return;

    const handleToggle = (e: Event) => {
      const ev = e as Event & { newState?: string };
      if (ev.newState !== 'open') return;
      if (typeof CSS !== 'undefined' && CSS.supports('position-area', 'block-end')) return;
      const t = trigger.getBoundingClientRect();
      const m = panel.getBoundingClientRect();
      const below = t.bottom + m.height + 6 <= window.innerHeight;
      panel.style.top = `${below ? t.bottom + 4 : Math.max(8, t.top - m.height - 4)}px`;
      panel.style.left = `${Math.max(8, Math.min(t.right - m.width, window.innerWidth - m.width - 8))}px`;
    };

    panel.addEventListener('toggle', handleToggle);
    return () => panel.removeEventListener('toggle', handleToggle);
  }, []);

  return (
    <div className="relative inline-block">
      <button
        ref={triggerRef}
        type="button"
        {...({ popovertarget: panelId } as Record<string, string>)}
        aria-label={triggerLabel}
        title={iconOnly ? triggerLabel : undefined}
        className={`inline-flex items-center justify-center gap-1.5 min-h-8 text-xs font-medium rounded-[var(--radius-DEFAULT)] border border-border-strong bg-surface-raised text-text-secondary hover:text-text hover:bg-surface cursor-pointer transition-colors ${
          iconOnly ? 'w-8 px-0' : 'px-2.5 py-1.5 min-h-9 text-sm'
        }`}
      >
        {triggerIcon ?? (iconOnly ? <MoreHorizontal className="w-4 h-4" aria-hidden="true" /> : null)}
        {!iconOnly ? <span>{triggerLabel}</span> : null}
      </button>
      <div
        ref={panelRef}
        id={panelId}
        {...({ popover: 'auto' } as Record<string, string>)}
        className="tss-popover z-[var(--z-popover)] min-w-44 p-1.5 rounded-[var(--radius-DEFAULT)] bg-surface-overlay border border-border-strong shadow-[var(--shadow-md)]"
      >
        <div className="flex flex-col gap-0.5">
          {items.map((item, idx) => (
            <React.Fragment key={idx}>
              {item.dividerBefore ? (
                <div className="my-1 border-t border-border" aria-hidden="true" />
              ) : null}
              <button
                type="button"
                onClick={() => {
                  (panelRef.current as unknown as { hidePopover?: () => void })?.hidePopover?.();
                  item.onClick();
                }}
                className={`w-full text-left px-3 py-2 text-sm rounded-[var(--radius-sm)] cursor-pointer transition-colors inline-flex items-center gap-2 ${
                  item.danger
                    ? 'text-danger hover:bg-danger-subtle'
                    : 'text-text hover:bg-surface'
                }`}
              >
                {item.icon ? <span className="shrink-0 opacity-75">{item.icon}</span> : null}
                <span>{item.label}</span>
              </button>
            </React.Fragment>
          ))}
        </div>
      </div>
    </div>
  );
}
