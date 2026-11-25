import { useEffect, useRef, useState } from 'react';
import { Check, Copy } from 'lucide-react';
import { useI18n } from '../../i18n';

const buttonHandlers = new WeakMap<HTMLButtonElement, (text: string) => void>();
const timers = new WeakMap<HTMLButtonElement, number>();
let globalRegistered = false;
let lastAnnouncement = '';

async function performCopy(button: HTMLButtonElement, target: HTMLElement | null) {
  if (!target) return;
  const text =
    target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement
      ? target.value
      : target.innerText.trim();
  if (!text) return;

  try {
    await navigator.clipboard.writeText(text);
    buttonHandlers.get(button)?.('ok');
  } catch {
    buttonHandlers.get(button)?.('fail');
  }
}

function ensureGlobalCopyListeners() {
  if (globalRegistered || typeof document === 'undefined') return;
  globalRegistered = true;

  if ('CommandEvent' in window) {
    document.addEventListener(
      'command',
      (event: Event) => {
        const ce = event as Event & { command?: string; source?: Element | null };
        if (
          ce.command === '--copy' &&
          ce.source instanceof HTMLButtonElement &&
          ce.target instanceof HTMLElement
        ) {
          void performCopy(ce.source, ce.target);
        }
      },
      true,
    );
  } else {
    document.addEventListener('click', (event: MouseEvent) => {
      if (event.defaultPrevented) return;
      const el = event.target instanceof Element ? event.target.closest('button[command="--copy"]') : null;
      if (!(el instanceof HTMLButtonElement)) return;
      const targetId = el.getAttribute('commandfor');
      if (!targetId) return;
      const target = document.getElementById(targetId);
      if (target) void performCopy(el, target);
    });
  }
}

export function CopyButton({
  targetId,
  ariaLabel,
  compact = true,
  className = '',
}: {
  targetId: string;
  ariaLabel?: string;
  compact?: boolean;
  className?: string;
}) {
  const { t } = useI18n();
  const btnRef = useRef<HTMLButtonElement>(null);
  const [status, setStatus] = useState<'idle' | 'ok' | 'fail'>('idle');
  const [announcement, setAnnouncement] = useState('');

  useEffect(() => {
    ensureGlobalCopyListeners();
    const btn = btnRef.current;
    if (!btn) return;

    buttonHandlers.set(btn, (outcome) => {
      const prevTimer = timers.get(btn);
      if (prevTimer) window.clearTimeout(prevTimer);

      const nextState = outcome === 'ok' ? 'ok' : 'fail';
      setStatus(nextState);
      let msg = nextState === 'ok' ? t.common.copied : t.common.copyFailed;
      if (msg === lastAnnouncement) msg += '\u00A0';
      lastAnnouncement = msg;
      setAnnouncement('');
      requestAnimationFrame(() => setAnnouncement(msg));

      timers.set(
        btn,
        window.setTimeout(() => {
          setStatus('idle');
          setAnnouncement('');
        }, 1800),
      );
    });

    return () => {
      const prevTimer = timers.get(btn);
      if (prevTimer) window.clearTimeout(prevTimer);
      buttonHandlers.delete(btn);
    };
  }, [t.common.copied, t.common.copyFailed]);

  return (
    <span className={`inline-flex items-center gap-1.5 relative ${className}`}>
      <button
        ref={btnRef}
        type="button"
        {...({ command: '--copy', commandfor: targetId } as Record<string, string>)}
        aria-label={compact ? (ariaLabel ?? t.common.copy) : ariaLabel}
        title={ariaLabel ?? t.common.copy}
        data-copied={status === 'ok' ? '' : undefined}
        data-failed={status === 'fail' ? '' : undefined}
        className={`inline-flex items-center justify-center gap-1.5 rounded-[var(--radius-sm)] border border-border text-text-muted hover:text-text hover:bg-surface cursor-pointer transition-colors ${
          compact ? 'min-w-7 min-h-7 px-1.5 text-xs' : 'min-h-8 px-2.5 py-1 text-xs font-medium bg-surface-raised border-border-strong'
        }`}
      >
        {status === 'ok' ? (
          <Check className="w-3.5 h-3.5 text-success" aria-hidden="true" />
        ) : (
          <Copy className="w-3.5 h-3.5" aria-hidden="true" />
        )}
        {!compact ? <span>{t.common.copy}</span> : null}
      </button>
      <span
        role="status"
        className={`text-[11px] font-medium whitespace-nowrap transition-opacity duration-150 ${
          status === 'ok'
            ? 'opacity-100 text-success-text'
            : status === 'fail'
              ? 'opacity-100 text-danger-text'
              : 'opacity-0 pointer-events-none select-none w-0 overflow-hidden'
        }`}
      >
        {announcement}
      </span>
    </span>
  );
}
