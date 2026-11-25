import React, { useId } from 'react';
import { SelectField, SelectFieldProps } from './SelectField';

interface FormFieldProps {
  label: string;
  htmlFor: string;
  hint?: string;
  error?: string;
  required?: boolean;
  children: React.ReactNode;
  className?: string;
}

export function FormField({
  label,
  htmlFor,
  hint,
  error,
  required,
  children,
  className = '',
}: FormFieldProps) {
  return (
    <div className={`mb-5 max-w-md ${className}`}>
      <label
        htmlFor={htmlFor}
        className="block mb-1.5 text-sm font-medium text-text-secondary"
      >
        {label}
        {required ? (
          <span aria-hidden="true" className="ms-1 text-danger">
            *
          </span>
        ) : null}
      </label>
      {hint ? (
        <p className="mb-1.5 text-xs text-text-muted leading-normal">{hint}</p>
      ) : null}
      {children}
      {error ? (
        <p role="alert" className="mt-1.5 text-xs font-medium text-danger">
          {error}
        </p>
      ) : null}
    </div>
  );
}

const INPUT_BASE =
  'tss-input w-full min-h-10 px-3 py-2 text-sm bg-surface text-text border border-border-strong rounded-[var(--radius-DEFAULT)] shadow-[var(--shadow-inner)] placeholder:text-text-muted/70 transition-colors';

export const Input = React.forwardRef<
  HTMLInputElement,
  React.InputHTMLAttributes<HTMLInputElement>
>(function Input({ className = '', ...props }, ref) {
  return <input ref={ref} className={`${INPUT_BASE} ${className}`} {...props} />;
});

export function Select({ className = '', children, ...props }: SelectFieldProps) {
  return (
    <SelectField className={className} {...props}>
      {children}
    </SelectField>
  );
}

export const Textarea = React.forwardRef<
  HTMLTextAreaElement,
  React.TextareaHTMLAttributes<HTMLTextAreaElement>
>(function Textarea({ className = '', ...props }, ref) {
  return <textarea ref={ref} className={`${INPUT_BASE} min-h-24 ${className}`} {...props} />;
});

export function CheckboxField({
  id,
  label,
  description,
  checked,
  onChange,
  className = '',
}: {
  id?: string;
  label: string;
  description?: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  className?: string;
}) {
  const autoId = useId();
  const resolvedId = id ?? autoId;
  return (
    <label
      htmlFor={resolvedId}
      className={`inline-flex items-start gap-2.5 min-h-11 py-1.5 cursor-pointer select-none ${className}`}
    >
      <input
        id={resolvedId}
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        className="tss-checkbox mt-0.5"
      />
      <span className="min-w-0">
        <span className="block text-sm font-medium text-text leading-snug">{label}</span>
        {description ? (
          <span className="block mt-1 text-xs text-text-muted leading-normal">{description}</span>
        ) : null}
      </span>
    </label>
  );
}

export interface ChoiceOption<T extends string> {
  value: T;
  title: string;
  tag?: string;
  description: string;
}

export function ChoiceCardGroup<T extends string>({
  name,
  label,
  value,
  options,
  onChange,
  columnsClass = 'grid-cols-1 sm:grid-cols-3',
}: {
  name: string;
  label: string;
  value: T;
  options: ChoiceOption<T>[];
  onChange: (val: T) => void;
  columnsClass?: string;
}) {
  const legendId = useId();
  return (
    <fieldset className="mb-5" aria-labelledby={legendId}>
      <legend id={legendId} className="block mb-2 text-sm font-medium text-text-secondary">
        {label}
      </legend>
      <div className={`grid ${columnsClass} gap-2.5`}>
        {options.map((opt) => {
          const checked = opt.value === value;
          return (
            <label key={opt.value} className="tss-choice-card p-3 flex items-start gap-2.5">
              <input
                type="radio"
                name={name}
                value={opt.value}
                checked={checked}
                onChange={() => onChange(opt.value)}
                className="tss-checkbox mt-0.5"
              />
              <span className="min-w-0 flex-1">
                <span className="flex items-baseline justify-between gap-2">
                  <span className="text-sm font-semibold text-text leading-tight">
                    {opt.title}
                  </span>
                  {opt.tag ? (
                    <span className="font-mono text-[11px] text-text-muted shrink-0">
                      {opt.tag}
                    </span>
                  ) : null}
                </span>
                <span className="mt-1 block text-xs text-text-muted leading-normal">
                  {opt.description}
                </span>
              </span>
            </label>
          );
        })}
      </div>
    </fieldset>
  );
}

export function useFieldId(prefix: string): string {
  const id = useId();
  return `${prefix}-${id}`;
}
