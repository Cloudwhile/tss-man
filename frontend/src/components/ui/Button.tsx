import React from 'react';

export type ButtonVariant = 'primary' | 'secondary' | 'tertiary' | 'danger-link' | 'danger-confirm';
export type ButtonSize = 'xs' | 'sm' | 'md';

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
}

const VARIANT_CLASSES: Record<ButtonVariant, string> = {
  primary:
    'tss-btn-primary bg-action text-on-action hover:bg-action-hover border border-transparent',
  secondary:
    'bg-surface-raised text-text border border-border-strong hover:bg-surface shadow-[var(--shadow-sm)] active:shadow-none',
  tertiary:
    'bg-transparent text-text-secondary hover:text-text hover:bg-surface-sunken/40 border border-transparent',
  'danger-link':
    'bg-transparent text-danger hover:bg-danger-subtle/50 border border-transparent',
  'danger-confirm':
    'bg-danger text-on-action hover:opacity-90 shadow-[var(--shadow-sm)] active:shadow-none border border-transparent',
};

const SIZE_CLASSES: Record<ButtonSize, string> = {
  xs: 'px-2.5 py-1 text-xs min-h-8',
  sm: 'px-3 py-1.5 text-sm min-h-9',
  md: 'px-4 py-2 text-sm min-h-10',
};

export function Button({
  variant = 'secondary',
  size = 'md',
  type = 'button',
  className = '',
  disabled,
  children,
  ...rest
}: ButtonProps) {
  return (
    <button
      type={type}
      disabled={disabled}
      className={`inline-flex items-center justify-center gap-2 rounded-[var(--radius-DEFAULT)] font-medium transition-colors duration-150 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed whitespace-nowrap ${VARIANT_CLASSES[variant]} ${SIZE_CLASSES[size]} ${className}`}
      {...rest}
    >
      {children}
    </button>
  );
}
