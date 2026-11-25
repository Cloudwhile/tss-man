import React, { useCallback, useEffect, useId, useMemo, useRef, useState } from 'react';
import { Check, ChevronDown, ChevronsUpDown } from 'lucide-react';

export interface SelectOption {
  value: string;
  label: string;
  badge?: string;
  meta?: string;
  statusDot?: 'online' | 'connecting' | 'offline';
  disabled?: boolean;
}

export interface SelectFieldProps {
  id?: string;
  name?: string;
  value: string | number;
  options?: SelectOption[];
  children?: React.ReactNode;
  onValueChange?: (value: string) => void;
  onChange?: (event: { target: { value: string } }) => void;
  placeholder?: string;
  size?: 'sm' | 'header' | 'md';
  variant?: 'default' | 'toolbar' | 'breadcrumb';
  ariaLabel?: string;
  disabled?: boolean;
  className?: string;
}

function extractNodeText(node: React.ReactNode): string {
  if (typeof node === 'string' || typeof node === 'number') return String(node);
  if (Array.isArray(node)) return node.map(extractNodeText).join('');
  if (React.isValidElement(node)) {
    const props = node.props as { children?: React.ReactNode };
    return extractNodeText(props.children);
  }
  return '';
}

function parseOptionsFromChildren(children: React.ReactNode): SelectOption[] {
  const list: SelectOption[] = [];
  React.Children.forEach(children, (child) => {
    if (!React.isValidElement(child)) return;
    if (child.type === React.Fragment) {
      const fragProps = child.props as { children?: React.ReactNode };
      list.push(...parseOptionsFromChildren(fragProps.children));
      return;
    }
    const props = child.props as {
      value?: string | number;
      disabled?: boolean;
      children?: React.ReactNode;
    };
    const label = extractNodeText(props.children).trim();
    const val = props.value !== undefined ? String(props.value) : label;
    list.push({
      value: val,
      label: label || val,
      disabled: Boolean(props.disabled),
    });
  });
  return list;
}

const DOT_TONES: Record<'online' | 'connecting' | 'offline', string> = {
  online: 'bg-success',
  connecting: 'bg-warning',
  offline: 'bg-danger',
};

export function SelectField({
  id,
  name,
  value,
  options,
  children,
  onValueChange,
  onChange,
  placeholder = 'Select…',
  size = 'md',
  variant = 'default',
  ariaLabel,
  disabled = false,
  className = '',
}: SelectFieldProps) {
  const listboxId = useId();
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);

  const resolvedOptions = useMemo(() => {
    if (options) return options;
    return parseOptionsFromChildren(children);
  }, [options, children]);

  const strVal = String(value);
  const activeOption = useMemo(
    () => resolvedOptions.find((o) => o.value === strVal) ?? null,
    [resolvedOptions, strVal],
  );

  const commitValue = useCallback(
    (nextVal: string) => {
      onValueChange?.(nextVal);
      onChange?.({ target: { value: nextVal } });
      (panelRef.current as unknown as { hidePopover?: () => void })?.hidePopover?.();
      triggerRef.current?.focus();
    },
    [onChange, onValueChange],
  );

  useEffect(() => {
    const panel = panelRef.current;
    const trigger = triggerRef.current;
    if (!panel || !trigger) return;

    const handleToggle = (e: Event) => {
      const ev = e as Event & { newState?: string };
      const isOpen = ev.newState === 'open';
      setOpen(isOpen);
      if (!isOpen) return;

      if (typeof CSS === 'undefined' || !CSS.supports('position-area', 'block-end')) {
        const t = trigger.getBoundingClientRect();
        panel.style.minWidth = `${Math.max(216, t.width)}px`;
        const m = panel.getBoundingClientRect();
        const below = t.bottom + m.height + 8 <= window.innerHeight;
        panel.style.top = `${below ? t.bottom + 6 : Math.max(8, t.top - m.height - 6)}px`;
        panel.style.left = `${Math.max(8, Math.min(t.left, window.innerWidth - m.width - 8))}px`;
      }

      requestAnimationFrame(() => {
        const selectedBtn = panel.querySelector<HTMLButtonElement>('[aria-selected="true"]');
        const firstBtn = panel.querySelector<HTMLButtonElement>('button[role="option"]:not(:disabled)');
        (selectedBtn ?? firstBtn)?.focus({ preventScroll: true });
      });
    };

    panel.addEventListener('toggle', handleToggle);
    return () => panel.removeEventListener('toggle', handleToggle);
  }, []);

  const handleTriggerKeyDown = (e: React.KeyboardEvent<HTMLButtonElement>) => {
    if (disabled) return;
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      if (!open) {
        (panelRef.current as unknown as { showPopover?: () => void })?.showPopover?.();
      }
    }
  };

  const handleListKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const panel = panelRef.current;
    if (!panel) return;
    const opts = Array.from(
      panel.querySelectorAll<HTMLButtonElement>('button[role="option"]:not(:disabled)'),
    );
    if (opts.length === 0) return;
    const idx = opts.indexOf(document.activeElement as HTMLButtonElement);

    if (e.key === 'ArrowDown') {
      e.preventDefault();
      opts[(idx + 1) % opts.length].focus();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      opts[(idx - 1 + opts.length) % opts.length].focus();
    } else if (e.key === 'Home') {
      e.preventDefault();
      opts[0].focus();
    } else if (e.key === 'End') {
      e.preventDefault();
      opts[opts.length - 1].focus();
    } else if (e.key === 'Tab') {
      (panel as unknown as { hidePopover?: () => void })?.hidePopover?.();
    }
  };

  const isBreadcrumb = variant === 'breadcrumb';

  const sizeClass = isBreadcrumb
    ? 'min-h-8 px-2.5 py-1 text-sm rounded-[var(--radius-DEFAULT)]'
    : size === 'sm'
      ? 'min-h-8 px-2.5 py-1 text-xs rounded-[var(--radius-sm)]'
      : size === 'header'
        ? 'min-h-9 px-3 py-1.5 text-sm rounded-[var(--radius-DEFAULT)]'
        : 'min-h-10 px-3 py-2 text-sm rounded-[var(--radius-DEFAULT)]';

  const variantClass = isBreadcrumb
    ? open
      ? 'bg-surface-sunken border border-border-strong shadow-[var(--shadow-sm)]'
      : 'bg-transparent hover:bg-surface-sunken/75 border border-transparent hover:border-border'
    : variant === 'toolbar'
      ? 'bg-surface-raised border border-border hover:border-border-strong shadow-[var(--shadow-sm)]'
      : 'bg-surface-raised border border-border-strong shadow-[var(--shadow-inner)] hover:border-text-muted/60';

  return (
    <div
      className={`relative inline-flex min-w-0 ${isBreadcrumb ? 'w-auto max-w-full' : 'w-full'} ${className}`}
    >
      {name ? <input type="hidden" name={name} value={strVal} /> : null}
      <button
        ref={triggerRef}
        id={id}
        type="button"
        role="combobox"
        disabled={disabled}
        aria-label={ariaLabel}
        aria-expanded={open}
        aria-haspopup="listbox"
        aria-controls={listboxId}
        {...({ popovertarget: listboxId } as Record<string, string>)}
        onKeyDown={handleTriggerKeyDown}
        className={`tss-input min-w-0 inline-flex items-center justify-between gap-2 text-left text-text transition-colors duration-150 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed ${
          isBreadcrumb ? 'w-auto max-w-full' : 'w-full'
        } ${sizeClass} ${variantClass}`}
      >
        <span className="flex items-center gap-2 min-w-0 flex-1">
          {activeOption?.statusDot ? (
            <span
              className={`w-2 h-2 rounded-full shrink-0 ${DOT_TONES[activeOption.statusDot]}`}
              aria-hidden="true"
            />
          ) : null}
          {activeOption?.badge ? (
            <span className="font-mono text-[10.5px] font-medium px-1.5 py-0.5 rounded-[var(--radius-sm)] bg-surface-sunken border border-border text-text-secondary shrink-0 leading-none tss-trim">
              {activeOption.badge}
            </span>
          ) : null}
          <span
            className={`truncate ${
              isBreadcrumb ? 'font-semibold tracking-tight' : 'font-medium'
            } ${activeOption ? 'text-text' : 'text-text-muted'}`}
          >
            {activeOption?.label ?? placeholder}
          </span>
        </span>
        {isBreadcrumb ? (
          <ChevronsUpDown
            className={`w-3.5 h-3.5 shrink-0 transition-colors ${
              open ? 'text-action' : 'text-text-muted'
            }`}
            strokeWidth={1.75}
            aria-hidden="true"
          />
        ) : (
          <ChevronDown
            className={`w-4 h-4 shrink-0 text-text-muted transition-transform duration-150 ${
              open ? 'rotate-180 text-action' : ''
            }`}
            strokeWidth={1.75}
            aria-hidden="true"
          />
        )}
      </button>

      <div
        ref={panelRef}
        id={listboxId}
        role="listbox"
        aria-label={ariaLabel}
        {...({ popover: 'auto' } as Record<string, string>)}
        onKeyDown={handleListKeyDown}
        className="tss-select-popover z-[var(--z-popover)] p-1.5 rounded-[var(--radius-lg)] bg-surface-overlay/98 backdrop-blur-md border border-border-strong shadow-[var(--shadow-lg)] text-text"
      >
        {ariaLabel && isBreadcrumb ? (
          <div className="px-2.5 pt-1 pb-1.5 text-[10.5px] font-semibold uppercase tracking-wider text-text-muted border-b border-border/70 mb-1">
            {ariaLabel}
          </div>
        ) : null}
        <div className="flex flex-col gap-0.5">
          {resolvedOptions.map((opt) => {
            const isSelected = opt.value === strVal;
            return (
              <button
                key={opt.value}
                type="button"
                role="option"
                disabled={opt.disabled}
                aria-selected={isSelected}
                onClick={() => commitValue(opt.value)}
                className={`w-full px-2.5 py-2 rounded-[var(--radius-sm)] text-left flex items-center justify-between gap-3 text-sm transition-colors cursor-pointer disabled:opacity-45 disabled:cursor-not-allowed ${
                  isSelected
                    ? 'bg-action-subtle text-action-text font-medium'
                    : 'text-text hover:bg-surface-sunken/80'
                }`}
              >
                <span className="flex items-center gap-2 min-w-0 flex-1">
                  {opt.statusDot ? (
                    <span
                      className={`w-2 h-2 rounded-full shrink-0 ${DOT_TONES[opt.statusDot]}`}
                      aria-hidden="true"
                    />
                  ) : null}
                  {opt.badge ? (
                    <span className="font-mono text-[10.5px] px-1.5 py-0.5 rounded-[var(--radius-sm)] bg-surface-sunken border border-border text-text-secondary shrink-0 leading-none tss-trim">
                      {opt.badge}
                    </span>
                  ) : null}
                  <span className="truncate">{opt.label}</span>
                </span>
                <span className="flex items-center gap-2 shrink-0">
                  {opt.meta ? (
                    <span className="font-mono text-[11px] text-text-muted tabular-nums">
                      {opt.meta}
                    </span>
                  ) : null}
                  <Check
                    className={`w-4 h-4 shrink-0 ${isSelected ? 'text-action opacity-100' : 'opacity-0'}`}
                    strokeWidth={2}
                    aria-hidden="true"
                  />
                </span>
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}
