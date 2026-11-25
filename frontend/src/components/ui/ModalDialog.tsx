import React, { useEffect, useId, useRef } from 'react';
import { X } from 'lucide-react';

interface ModalDialogProps {
  open: boolean;
  onClose: () => void;
  title: string;
  subtitle?: string;
  eyebrow?: string;
  lightDismiss?: boolean;
  maxWidthClass?: string;
  children: React.ReactNode;
}

export function ModalDialog({
  open,
  onClose,
  title,
  subtitle,
  eyebrow,
  lightDismiss = true,
  maxWidthClass = 'max-w-lg',
  children,
}: ModalDialogProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    const dlg = dialogRef.current;
    if (!dlg) return;
    if (open && !dlg.open) {
      dlg.showModal();
    } else if (!open && dlg.open) {
      dlg.close();
    }
  }, [open]);

  useEffect(() => {
    const dlg = dialogRef.current;
    if (!dlg) return;

    const handleClose = () => {
      dlg.returnValue = '';
      onCloseRef.current();
    };
    dlg.addEventListener('close', handleClose);

    // Fallback for browsers without <dialog closedby="any">
    let removeClickFallback: (() => void) | undefined;
    if (lightDismiss && !('closedBy' in HTMLDialogElement.prototype)) {
      const handleClick = (event: MouseEvent) => {
        if (event.target !== dlg) return;
        const rect = dlg.getBoundingClientRect();
        const inside =
          rect.top <= event.clientY &&
          event.clientY <= rect.top + rect.height &&
          rect.left <= event.clientX &&
          event.clientX <= rect.left + rect.width;
        if (!inside) dlg.close();
      };
      dlg.addEventListener('click', handleClick);
      removeClickFallback = () => dlg.removeEventListener('click', handleClick);
    }

    return () => {
      dlg.removeEventListener('close', handleClose);
      removeClickFallback?.();
    };
  }, [lightDismiss]);

  return (
    <dialog
      ref={dialogRef}
      aria-labelledby={titleId}
      {...({ closedby: lightDismiss ? 'any' : 'closerequest' } as Record<string, string>)}
      className={`tss-dialog w-[calc(100vw-2rem)] ${maxWidthClass} p-0 overflow-hidden`}
    >
      {open ? (
        <div className="flex flex-col max-h-[85vh]">
          <div className="flex items-start justify-between gap-4 px-6 py-4 border-b border-border bg-surface/50">
            <div className="min-w-0">
              {eyebrow ? (
                <div className="font-mono text-[11px] uppercase tracking-wide text-text-muted mb-0.5">
                  {eyebrow}
                </div>
              ) : null}
              <h1 id={titleId} className="text-base font-semibold text-text leading-tight">
                {title}
              </h1>
              {subtitle ? (
                <p className="mt-1 text-xs text-text-secondary leading-normal">{subtitle}</p>
              ) : null}
            </div>
            <button
              type="button"
              onClick={() => dialogRef.current?.close()}
              aria-label="Close"
              className="shrink-0 w-8 h-8 inline-flex items-center justify-center rounded-[var(--radius-DEFAULT)] text-text-muted hover:text-text hover:bg-surface-sunken/40 cursor-pointer"
            >
              <X className="w-4 h-4" aria-hidden="true" />
            </button>
          </div>
          <div className="px-6 py-5 overflow-y-auto tss-scroller">{children}</div>
        </div>
      ) : null}
    </dialog>
  );
}
