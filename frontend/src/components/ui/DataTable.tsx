import React, { useId } from 'react';

export function TableContainer({
  label,
  children,
  className = '',
}: {
  label: string;
  children: React.ReactNode;
  className?: string;
}) {
  const id = useId();
  return (
    <div
      tabIndex={0}
      role="region"
      aria-labelledby={id}
      className={`tss-table-wrapper tss-frame overflow-hidden ${className}`}
    >
      <span id={id} className="visually-hidden">
        {label}
      </span>
      {children}
    </div>
  );
}

export function Th({
  numeric = false,
  children,
  className = '',
}: {
  numeric?: boolean;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <th
      scope="col"
      className={`px-4 py-2.5 text-[11px] font-semibold uppercase tracking-wider text-text-muted bg-surface-sunken/55 border-b border-border whitespace-nowrap ${
        numeric ? 'text-right' : 'text-left'
      } ${className}`}
    >
      {children}
    </th>
  );
}

export function Td({
  numeric = false,
  label,
  children,
  className = '',
}: {
  numeric?: boolean;
  label?: string;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <td
      data-label={label}
      className={`px-4 py-3 text-sm text-text border-b border-border/60 [tr:last-child_&]:border-b-0 transition-colors [tr:hover_&]:bg-surface-sunken/35 ${
        numeric ? 'text-right tabular-nums' : 'text-left'
      } ${className}`}
    >
      {children}
    </td>
  );
}
