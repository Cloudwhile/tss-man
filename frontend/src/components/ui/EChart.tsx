/* Modular Apache ECharts wrapper with live Light/Dark theme tracking, XSS-safe glass tooltip builder, & ResizeObserver auto-fit. */
import { useEffect, useRef, useState } from 'react';
import * as echarts from 'echarts/core';
import { BarChart, LineChart } from 'echarts/charts';
import {
  GridComponent,
  LegendComponent,
  TooltipComponent,
} from 'echarts/components';
import { CanvasRenderer } from 'echarts/renderers';
import type { EChartsCoreOption, EChartsType } from 'echarts/core';

echarts.use([
  LineChart,
  BarChart,
  GridComponent,
  TooltipComponent,
  LegendComponent,
  CanvasRenderer,
]);

export interface ChartPalette {
  isDark: boolean;
  text: string;
  textMuted: string;
  axisLine: string;
  gridLine: string;
  crosshair: string;
  tooltipBg: string;
  tooltipBorder: string;
  tooltipText: string;
  tooltipMuted: string;
  cobalt: string;
  sky: string;
  teal: string;
  amber: string;
}

function escapeHtml(raw: string): string {
  return raw
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

export function buildTimeTooltipFormatter(
  palette: ChartPalette,
  range: '1h' | '24h' | '7d' | '30d',
  units: Record<string, string>,
) {
  return (params: unknown) => {
    const items = Array.isArray(params) ? params : [params];
    if (items.length === 0) return '';
    const first = items[0] as { value?: unknown; axisValue?: unknown };
    const rawTs = Array.isArray(first?.value) ? Number(first.value[0]) : Number(first?.axisValue);
    const d = new Date(rawTs);
    const header = Number.isNaN(d.getTime())
      ? ''
      : range === '7d' || range === '30d'
        ? `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')} ${d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`
        : d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });

    const rows = items
      .map((entry) => {
        const p = entry as {
          seriesName?: string;
          color?: string;
          value?: unknown;
        };
        const name = p.seriesName ?? '';
        const val = Array.isArray(p.value) ? p.value[1] : p.value;
        const unit = units[name] ?? '';
        const dotColor = typeof p.color === 'string' ? p.color : palette.cobalt;
        return `<div style="display:flex;align-items:center;justify-content:space-between;gap:16px;margin-top:4px;font-size:12px;line-height:1.35;">
          <span style="display:inline-flex;align-items:center;gap:6px;color:${palette.tooltipMuted};">
            <span style="width:7px;height:7px;border-radius:999px;background:${escapeHtml(dotColor)};display:inline-block;"></span>
            <span>${escapeHtml(name)}</span>
          </span>
          <span style="font-variant-numeric:tabular-nums;font-weight:600;color:${palette.tooltipText};">${escapeHtml(String(val ?? '—'))}${unit ? ` <span style="font-weight:400;color:${palette.tooltipMuted};font-size:11px;">${escapeHtml(unit)}</span>` : ''}</span>
        </div>`;
      })
      .join('');

    return `<div style="min-width:148px;">
      <div style="font-family:'JetBrains Mono Variable','JetBrains Mono',monospace;font-size:11px;color:${palette.tooltipMuted};padding-bottom:4px;margin-bottom:2px;border-bottom:1px solid ${palette.gridLine};">${escapeHtml(header)}</div>
      ${rows}
    </div>`;
  };
}

function resolveIsDark(): boolean {
  if (typeof document === 'undefined') return false;
  const theme = document.documentElement.dataset.theme;
  if (theme === 'dark') return true;
  if (theme === 'light') return false;
  return window.matchMedia?.('(prefers-color-scheme: dark)').matches ?? false;
}

export function useChartTheme(): ChartPalette {
  const [isDark, setIsDark] = useState<boolean>(resolveIsDark);

  useEffect(() => {
    const sync = () => setIsDark(resolveIsDark());
    const obs = new MutationObserver(sync);
    obs.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-theme'],
    });
    const mq = window.matchMedia?.('(prefers-color-scheme: dark)');
    mq?.addEventListener?.('change', sync);
    return () => {
      obs.disconnect();
      mq?.removeEventListener?.('change', sync);
    };
  }, []);

  return isDark
    ? {
        isDark: true,
        text: '#f2f4f7',
        textMuted: '#8e9bb0',
        axisLine: 'rgba(148, 163, 184, 0.14)',
        gridLine: 'rgba(148, 163, 184, 0.09)',
        crosshair: 'rgba(148, 163, 184, 0.32)',
        tooltipBg: 'rgba(15, 21, 34, 0.94)',
        tooltipBorder: 'rgba(148, 163, 184, 0.22)',
        tooltipText: '#f8fafc',
        tooltipMuted: '#94a3b8',
        cobalt: '#3b82f6',
        sky: '#38bdf8',
        teal: '#10b981',
        amber: '#f59e0b',
      }
    : {
        isDark: false,
        text: '#101828',
        textMuted: '#667085',
        axisLine: '#e5e9f0',
        gridLine: '#eef1f6',
        crosshair: '#98a2b3',
        tooltipBg: 'rgba(255, 255, 255, 0.96)',
        tooltipBorder: '#d0d7e2',
        tooltipText: '#101828',
        tooltipMuted: '#475467',
        cobalt: '#2563eb',
        sky: '#0ea5e9',
        teal: '#0d9488',
        amber: '#d97706',
      };
}

export function EChart({
  option,
  heightClass = 'h-60',
  className = '',
}: {
  option: EChartsCoreOption;
  heightClass?: string;
  className?: string;
}) {
  const hostRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<EChartsType | null>(null);

  useEffect(() => {
    const el = hostRef.current;
    if (!el) return;

    const chart = echarts.init(el, undefined, { renderer: 'canvas' });
    chartRef.current = chart;

    const ro = new ResizeObserver(() => {
      chart.resize();
    });
    ro.observe(el);

    return () => {
      ro.disconnect();
      chart.dispose();
      chartRef.current = null;
    };
  }, []);

  useEffect(() => {
    const chart = chartRef.current;
    if (!chart) return;
    chart.clear();
    chart.setOption(option, true);
  }, [option]);

  return <div ref={hostRef} className={`w-full ${heightClass} ${className}`} />;
}
