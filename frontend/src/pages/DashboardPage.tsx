import { useCallback, useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  Activity,
  ArrowRight,
  ArrowUpDown,
  Gauge,
  Server,
  ShieldCheck,
  Users,
} from 'lucide-react';
import * as echarts from 'echarts/core';
import type { EChartsCoreOption } from 'echarts/core';
import { useApp } from '../context/AppContext';
import { useI18n } from '../i18n';
import {
  apiRequest,
  AuditLogView,
  LiveTreeState,
  MetricPoint,
} from '../lib/api';
import {
  formatAuditAction,
  formatBytesPerSec,
  formatCompactTime,
  formatDuration,
  splitThroughputParts,
} from '../lib/format';
import { Button } from '../components/ui/Button';
import { buildTimeTooltipFormatter, EChart, useChartTheme } from '../components/ui/EChart';
import { EmptyState } from '../components/ui/EmptyState';
import { SlotMeter } from '../components/ui/SlotMeter';
import { StatusBadge } from '../components/ui/StatusBadge';

const CHART_FONT = "'Inter Variable', Inter, system-ui, sans-serif";

function formatTimeTick(val: number, range: '1h' | '24h' | '7d' | '30d'): string {
  const d = new Date(val);
  if (range === '7d' || range === '30d') {
    return `${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
  }
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

export function DashboardPage() {
  const { t, locale } = useI18n();
  const navigate = useNavigate();
  const { instances, activeInstanceId, activeSid, subscribeWs } = useApp();
  const chartTheme = useChartTheme();

  const [range, setRange] = useState<'1h' | '24h' | '7d' | '30d'>('1h');
  const [tree, setTree] = useState<LiveTreeState | null>(null);
  const [metrics, setMetrics] = useState<MetricPoint[]>([]);
  const [audits, setAudits] = useState<AuditLogView[]>([]);

  const loadData = useCallback(async () => {
    if (!activeInstanceId) return;
    try {
      const [treeRes, metricRes, instAudits, globalAudits] = await Promise.all([
        apiRequest<LiveTreeState>(`/instances/${activeInstanceId}/servers/${activeSid}/tree`),
        apiRequest<MetricPoint[]>(
          `/instances/${activeInstanceId}/servers/${activeSid}/metrics?range=${range}`,
        ),
        apiRequest<AuditLogView[]>(`/audit-logs?limit=6&instance_id=${activeInstanceId}`),
        apiRequest<AuditLogView[]>('/audit-logs?limit=6'),
      ]);
      setTree(treeRes);
      setMetrics(metricRes);
      if (instAudits.length >= 4) {
        setAudits(instAudits.slice(0, 6));
      } else {
        const seen = new Set(instAudits.map((a) => a.id));
        const merged = [...instAudits];
        for (const item of globalAudits) {
          if (!seen.has(item.id)) {
            merged.push(item);
            seen.add(item.id);
          }
          if (merged.length >= 6) break;
        }
        setAudits(merged);
      }
    } catch {}
  }, [activeInstanceId, activeSid, range]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  useEffect(() => {
    return subscribeWs((msg) => {
      if (msg.instance_id === activeInstanceId && msg.virtual_server_id === activeSid) {
        if (msg.event === 'state.delta' && msg.payload) {
          setTree((prev) => ({
            server_info:
              (msg.payload.server_info as LiveTreeState['server_info']) ??
              prev?.server_info ??
              null,
            channels: (msg.payload.channels as LiveTreeState['channels']) ?? prev?.channels ?? [],
            clients: (msg.payload.clients as LiveTreeState['clients']) ?? prev?.clients ?? [],
            updated_at: msg.timestamp,
          }));
        }
      }
    });
  }, [activeInstanceId, activeSid, subscribeWs]);

  const info = tree?.server_info;
  const voiceClients =
    tree?.clients.filter((c) => c.client_type === 0).length ?? info?.clients_online ?? 0;
  const maxClients = Math.max(1, info?.max_clients ?? 64);
  const occupancyPct = Math.min(100, (voiceClients / maxClients) * 100);
  const activeChannelCount = tree?.channels.length ?? info?.channels_online ?? 0;
  const ping = info?.total_ping_ms ?? 0;
  const loss = info?.total_packetloss_pct ?? 0;
  const bwIn = info?.bw_in_bps ?? 0;
  const bwOut = info?.bw_out_bps ?? 0;
  const totalBwParts = splitThroughputParts(bwIn + bwOut);

  const rawSortedPoints = useMemo(() => {
    return metrics
      .map((m) => ({
        ts: new Date(m.recorded_at).getTime(),
        m,
      }))
      .filter((p) => Number.isFinite(p.ts))
      .sort((a, b) => a.ts - b.ts);
  }, [metrics]);

  // Bucket high-frequency live collector ticks cleanly into uniform time slots per range so splines stay smooth
  const sortedPoints = useMemo(() => {
    const bucketMs =
      range === '1h'
        ? 2 * 60_000
        : range === '24h'
          ? 45 * 60_000
          : range === '7d'
            ? 6 * 3_600_000
            : 24 * 3_600_000;

    const groups = new Map<number, typeof rawSortedPoints>();
    for (const item of rawSortedPoints) {
      const key = Math.floor(item.ts / bucketMs) * bucketMs;
      const arr = groups.get(key);
      if (arr) arr.push(item);
      else groups.set(key, [item]);
    }

    return Array.from(groups.entries())
      .sort((a, b) => a[0] - b[0])
      .map(([, items]) => {
        const last = items[items.length - 1];
        const n = items.length;
        return {
          ts: last.ts,
          m: {
            ...last.m,
            clients_online: Math.round(
              items.reduce((s, x) => s + x.m.clients_online, 0) / n,
            ),
            avg_ping_ms: items.reduce((s, x) => s + x.m.avg_ping_ms, 0) / n,
            packet_loss_pct: items.reduce((s, x) => s + x.m.packet_loss_pct, 0) / n,
            bw_in_bps: Math.round(items.reduce((s, x) => s + x.m.bw_in_bps, 0) / n),
            bw_out_bps: Math.round(items.reduce((s, x) => s + x.m.bw_out_bps, 0) / n),
          },
        };
      });
  }, [rawSortedPoints, range]);

  const peakClients = useMemo(
    () => Math.max(voiceClients, ...sortedPoints.map((p) => p.m.clients_online)),
    [sortedPoints, voiceClients],
  );

  const avgRangePing = useMemo(() => {
    if (sortedPoints.length === 0) return ping;
    const sum = sortedPoints.reduce((acc, p) => acc + p.m.avg_ping_ms, 0);
    return sum / sortedPoints.length;
  }, [sortedPoints, ping]);

  const peakLoss = useMemo(
    () => Math.max(loss, ...sortedPoints.map((p) => p.m.packet_loss_pct)),
    [sortedPoints, loss],
  );

  const baseTooltip = useMemo(
    () => ({
      trigger: 'axis' as const,
      backgroundColor: chartTheme.tooltipBg,
      borderColor: chartTheme.tooltipBorder,
      borderWidth: 1,
      padding: [8, 11],
      extraCssText:
        'border-radius: 10px; box-shadow: 0 12px 28px -6px rgba(15, 23, 42, 0.18); backdrop-filter: blur(8px);',
      axisPointer: {
        type: 'line' as const,
        lineStyle: { color: chartTheme.crosshair, type: 'dashed' as const, width: 1 },
      },
    }),
    [chartTheme],
  );

  const clientOption: EChartsCoreOption = useMemo(
    () => ({
      animationDuration: 320,
      textStyle: { fontFamily: CHART_FONT },
      tooltip: {
        ...baseTooltip,
        formatter: buildTimeTooltipFormatter(chartTheme, range, {}),
      },
      legend: { show: false },
      grid: { top: 10, right: 12, bottom: 22, left: 32, containLabel: false },
      xAxis: {
        type: 'time',
        axisLine: { lineStyle: { color: chartTheme.axisLine } },
        axisTick: { show: false },
        splitLine: { show: false },
        axisLabel: {
          color: chartTheme.textMuted,
          fontSize: 11,
          hideOverlap: true,
          formatter: (val: number) => formatTimeTick(val, range),
        },
      },
      yAxis: {
        type: 'value',
        minInterval: 1,
        splitNumber: 4,
        splitLine: { lineStyle: { color: chartTheme.gridLine, type: 'dashed' } },
        axisLabel: { color: chartTheme.textMuted, fontSize: 11 },
      },
      series: [
        {
          name: t.dashboard.kpiOnline,
          type: 'line',
          smooth: 0.32,
          showSymbol: false,
          symbolSize: 5,
          data: sortedPoints.map((p) => [p.ts, p.m.clients_online]),
          lineStyle: { width: 2.2, color: chartTheme.cobalt },
          itemStyle: { color: chartTheme.cobalt },
          areaStyle: {
            color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
              {
                offset: 0,
                color: chartTheme.isDark ? 'rgba(59, 130, 246, 0.28)' : 'rgba(37, 99, 235, 0.16)',
              },
              { offset: 1, color: 'rgba(37, 99, 235, 0.0)' },
            ]),
          },
        },
        {
          name: t.dashboard.channelsCount,
          type: 'line',
          smooth: 0.2,
          showSymbol: false,
          data: sortedPoints.map((p) => [p.ts, p.m.channels_online]),
          lineStyle: { width: 1.5, type: 'dashed', color: chartTheme.sky },
          itemStyle: { color: chartTheme.sky },
        },
      ],
    }),
    [baseTooltip, chartTheme, range, sortedPoints, t.dashboard.channelsCount, t.dashboard.kpiOnline],
  );

  const qualityOption: EChartsCoreOption = useMemo(
    () => ({
      animationDuration: 320,
      textStyle: { fontFamily: CHART_FONT },
      tooltip: {
        ...baseTooltip,
        formatter: buildTimeTooltipFormatter(chartTheme, range, {
          [t.dashboard.avgLatencyShort]: 'ms',
          [t.dashboard.packetLossInline]: '%',
        }),
      },
      legend: { show: false },
      grid: { top: 10, right: 40, bottom: 22, left: 46, containLabel: false },
      xAxis: {
        type: 'time',
        axisLine: { lineStyle: { color: chartTheme.axisLine } },
        axisTick: { show: false },
        splitLine: { show: false },
        axisLabel: {
          color: chartTheme.textMuted,
          fontSize: 11,
          hideOverlap: true,
          formatter: (val: number) => formatTimeTick(val, range),
        },
      },
      yAxis: [
        {
          type: 'value',
          splitNumber: 4,
          splitLine: { lineStyle: { color: chartTheme.gridLine, type: 'dashed' } },
          axisLabel: { color: chartTheme.textMuted, fontSize: 11, formatter: '{value} ms' },
        },
        {
          type: 'value',
          min: 0,
          max: (val: { max: number }) => Math.max(2, Math.ceil(val.max * 1.45)),
          splitNumber: 4,
          splitLine: { show: false },
          axisLabel: { color: chartTheme.textMuted, fontSize: 11, formatter: '{value}%' },
        },
      ],
      series: [
        {
          name: t.dashboard.avgLatencyShort,
          type: 'line',
          smooth: 0.32,
          showSymbol: false,
          symbolSize: 5,
          z: 3,
          data: sortedPoints.map((p) => [p.ts, Number(p.m.avg_ping_ms.toFixed(1))]),
          lineStyle: { width: 2.2, color: chartTheme.teal },
          itemStyle: { color: chartTheme.teal },
          areaStyle: {
            color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
              {
                offset: 0,
                color: chartTheme.isDark ? 'rgba(16, 185, 129, 0.22)' : 'rgba(13, 148, 136, 0.13)',
              },
              { offset: 1, color: 'rgba(16, 185, 129, 0.0)' },
            ]),
          },
        },
        {
          name: t.dashboard.packetLossInline,
          type: 'bar',
          yAxisIndex: 1,
          barMaxWidth: 5,
          z: 2,
          data: sortedPoints.map((p) => [p.ts, Number(p.m.packet_loss_pct.toFixed(2))]),
          itemStyle: {
            color: chartTheme.amber,
            borderRadius: [2, 2, 0, 0],
            opacity: chartTheme.isDark ? 0.45 : 0.38,
          },
          emphasis: {
            itemStyle: { opacity: 0.9 },
          },
        },
      ],
    }),
    [
      baseTooltip,
      chartTheme,
      range,
      sortedPoints,
      t.dashboard.avgLatencyShort,
      t.dashboard.packetLossInline,
    ],
  );

  const bandwidthOption: EChartsCoreOption = useMemo(
    () => ({
      animationDuration: 320,
      textStyle: { fontFamily: CHART_FONT },
      tooltip: {
        ...baseTooltip,
        formatter: buildTimeTooltipFormatter(chartTheme, range, {
          [t.dashboard.outbound]: 'KB/s',
          [t.dashboard.inbound]: 'KB/s',
        }),
      },
      legend: { show: false },
      grid: { top: 10, right: 12, bottom: 22, left: 56, containLabel: false },
      xAxis: {
        type: 'time',
        axisLine: { lineStyle: { color: chartTheme.axisLine } },
        axisTick: { show: false },
        splitLine: { show: false },
        axisLabel: {
          color: chartTheme.textMuted,
          fontSize: 11,
          hideOverlap: true,
          formatter: (val: number) => formatTimeTick(val, range),
        },
      },
      yAxis: {
        type: 'value',
        splitNumber: 4,
        splitLine: { lineStyle: { color: chartTheme.gridLine, type: 'dashed' } },
        axisLabel: { color: chartTheme.textMuted, fontSize: 11, formatter: '{value} KB/s' },
      },
      series: [
        {
          name: t.dashboard.outbound,
          type: 'line',
          smooth: 0.32,
          showSymbol: false,
          data: sortedPoints.map((p) => [p.ts, Number((p.m.bw_out_bps / 1024).toFixed(1))]),
          lineStyle: { width: 2.1, color: chartTheme.cobalt },
          itemStyle: { color: chartTheme.cobalt },
          areaStyle: {
            color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
              {
                offset: 0,
                color: chartTheme.isDark ? 'rgba(59, 130, 246, 0.24)' : 'rgba(37, 99, 235, 0.14)',
              },
              { offset: 1, color: 'rgba(37, 99, 235, 0.0)' },
            ]),
          },
        },
        {
          name: t.dashboard.inbound,
          type: 'line',
          smooth: 0.32,
          showSymbol: false,
          data: sortedPoints.map((p) => [p.ts, Number((p.m.bw_in_bps / 1024).toFixed(1))]),
          lineStyle: { width: 1.8, color: chartTheme.sky },
          itemStyle: { color: chartTheme.sky },
        },
      ],
    }),
    [baseTooltip, chartTheme, range, sortedPoints, t.dashboard.inbound, t.dashboard.outbound],
  );

  if (instances.length === 0) {
    return (
      <EmptyState
        icon={Server}
        title={t.dashboard.emptyTitle}
        description={t.dashboard.emptyDesc}
        action={
          <Button variant="primary" onClick={() => navigate('/instances')}>
            {t.dashboard.emptyCta}
          </Button>
        }
      />
    );
  }

  return (
    <div className="space-y-5">
      {/* Header & Inset Range Switcher */}
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-xl sm:text-2xl font-semibold text-text tracking-tight">
            {t.dashboard.title}
          </h1>
          <p className="mt-0.5 text-sm text-text-secondary tss-prose">{t.dashboard.subtitle}</p>
        </div>

        <div
          role="group"
          aria-label="Time Range"
          className="tss-well p-1 inline-flex items-center gap-0.5"
        >
          {(['1h', '24h', '7d', '30d'] as const).map((r) => (
            <button
              key={r}
              type="button"
              aria-pressed={range === r}
              onClick={() => setRange(r)}
              className={`min-h-7 px-3 py-1 text-xs rounded-[var(--radius-sm)] cursor-pointer ${
                range === r
                  ? 'bg-surface-raised text-text font-semibold border border-border-strong/80 shadow-[var(--shadow-sm)]'
                  : 'text-text-secondary hover:text-text font-medium border border-transparent'
              }`}
            >
              {r === '1h'
                ? t.dashboard.range1h
                : r === '24h'
                  ? t.dashboard.range24h
                  : r === '7d'
                    ? t.dashboard.range7d
                    : t.dashboard.range30d}
            </button>
          ))}
        </div>
      </div>

      {/* 4 Unified Telemetry KPI Cards (identical numeral scale, baseline alignment §4.4, zero cliché icon boxes) */}
      <section
        aria-label="Key Telemetry"
        className="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-4 gap-4"
      >
        {/* KPI 1: Online Voice Clients */}
        <div className="tss-panel p-4 sm:p-5 flex flex-col justify-between gap-3">
          <div className="flex items-center justify-between gap-2">
            <span className="text-xs font-medium text-text-muted">{t.dashboard.kpiOnline}</span>
            <Users className="w-4 h-4 text-text-muted shrink-0" strokeWidth={1.75} aria-hidden="true" />
          </div>
          <div className="flex items-baseline justify-between gap-2">
            <div className="flex items-baseline gap-1.5">
              <span className="text-[1.625rem] font-semibold text-text tabular-nums leading-none tracking-tight">
                {voiceClients}
              </span>
              <span className="text-xs font-medium text-text-muted tabular-nums">
                / {info?.max_clients ?? 0}
              </span>
            </div>
            <span className="font-mono text-[11px] px-1.5 py-0.5 rounded-[var(--radius-sm)] bg-action-subtle text-action-text border border-action/15 tabular-nums leading-none tss-trim">
              {occupancyPct.toFixed(0)}%
            </span>
          </div>
          <div className="space-y-1.5">
            <SlotMeter
              used={voiceClients}
              max={maxClients}
              label={t.dashboard.utilization}
            />
            <div className="flex items-center justify-between text-[11px] text-text-muted tabular-nums">
              <span>
                {t.dashboard.peakClients} {peakClients}
              </span>
              <span>
                {activeChannelCount} {t.dashboard.channelsCount}
              </span>
            </div>
          </div>
        </div>

        {/* KPI 2: Average Network Latency */}
        <div className="tss-panel p-4 sm:p-5 flex flex-col justify-between gap-3">
          <div className="flex items-center justify-between gap-2">
            <span className="text-xs font-medium text-text-muted">{t.dashboard.kpiPing}</span>
            <Gauge className="w-4 h-4 text-text-muted shrink-0" strokeWidth={1.75} aria-hidden="true" />
          </div>
          <div className="flex items-baseline gap-1.5">
            <span className="text-[1.625rem] font-semibold text-text tabular-nums leading-none tracking-tight">
              {ping.toFixed(1)}
            </span>
            <span className="text-xs font-medium text-text-muted">ms</span>
          </div>
          <div className="pt-1 border-t border-border/60 flex items-center justify-between gap-2">
            <StatusBadge
              variant="dot"
              tone={ping < 50 ? 'success' : ping < 120 ? 'warning' : 'danger'}
              label={
                ping < 50
                  ? t.dashboard.latencyHealthy
                  : ping < 120
                    ? t.dashboard.latencyElevated
                    : t.dashboard.latencyHigh
              }
            />
            <span className="font-mono text-[11px] text-text-muted tabular-nums">
              Ø {avgRangePing.toFixed(1)} ms
            </span>
          </div>
        </div>

        {/* KPI 3: Packet Loss Rate */}
        <div className="tss-panel p-4 sm:p-5 flex flex-col justify-between gap-3">
          <div className="flex items-center justify-between gap-2">
            <span className="text-xs font-medium text-text-muted">{t.dashboard.kpiLoss}</span>
            <Activity className="w-4 h-4 text-text-muted shrink-0" strokeWidth={1.75} aria-hidden="true" />
          </div>
          <div className="flex items-baseline gap-1.5">
            <span className="text-[1.625rem] font-semibold text-text tabular-nums leading-none tracking-tight">
              {loss.toFixed(2)}
            </span>
            <span className="text-xs font-medium text-text-muted">%</span>
          </div>
          <div className="pt-1 border-t border-border/60 flex items-center justify-between gap-2">
            <StatusBadge
              variant="dot"
              tone={loss < 1.5 ? 'success' : 'warning'}
              label={loss < 1.5 ? t.dashboard.lossHealthy : t.dashboard.lossWarn}
            />
            <span className="font-mono text-[11px] text-text-muted tabular-nums">
              MAX {peakLoss.toFixed(2)}%
            </span>
          </div>
        </div>

        {/* KPI 4: Combined Voice Throughput */}
        <div className="tss-panel p-4 sm:p-5 flex flex-col justify-between gap-3">
          <div className="flex items-center justify-between gap-2">
            <span className="text-xs font-medium text-text-muted">{t.dashboard.kpiBandwidth}</span>
            <ArrowUpDown className="w-4 h-4 text-text-muted shrink-0" strokeWidth={1.75} aria-hidden="true" />
          </div>
          <div className="flex items-baseline gap-1.5">
            <span className="text-[1.625rem] font-semibold text-text tabular-nums leading-none tracking-tight">
              {totalBwParts.num}
            </span>
            <span className="text-xs font-medium text-text-muted">{totalBwParts.unit}</span>
          </div>
          <div className="pt-1 border-t border-border/60 flex items-center justify-between gap-2 font-mono text-[11px] text-text-muted tabular-nums">
            <span className="inline-flex items-center gap-1">
              <span className="text-sky-500" aria-hidden="true">↓</span>
              <span>{formatBytesPerSec(bwIn)}</span>
            </span>
            <span className="inline-flex items-center gap-1">
              <span className="text-action" aria-hidden="true">↑</span>
              <span>{formatBytesPerSec(bwOut)}</span>
            </span>
          </div>
        </div>
      </section>

      {/* Primary Apache ECharts Row */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <section className="tss-panel p-5">
          <div className="flex flex-wrap items-center justify-between gap-2 mb-3">
            <div>
              <h2 className="text-sm font-semibold text-text">{t.dashboard.chartClients}</h2>
            </div>
            <div className="flex flex-wrap items-center gap-3 text-xs text-text-secondary">
              <span className="inline-flex items-center gap-1.5">
                <span
                  className="w-2 h-2 rounded-full shrink-0"
                  style={{ backgroundColor: chartTheme.cobalt }}
                  aria-hidden="true"
                />
                <span>{t.dashboard.kpiOnline}</span>
              </span>
              <span className="inline-flex items-center gap-1.5">
                <span
                  className="w-2.5 h-0 border-t-2 border-dashed shrink-0"
                  style={{ borderColor: chartTheme.sky }}
                  aria-hidden="true"
                />
                <span>{t.dashboard.channelsCount}</span>
              </span>
            </div>
          </div>
          <EChart option={clientOption} heightClass="h-56" />
        </section>

        <section className="tss-panel p-5">
          <div className="flex flex-wrap items-center justify-between gap-2 mb-3">
            <div>
              <h2 className="text-sm font-semibold text-text">{t.dashboard.chartQuality}</h2>
            </div>
            <div className="flex flex-wrap items-center gap-3 text-xs text-text-secondary">
              <span className="inline-flex items-center gap-1.5">
                <span
                  className="w-2 h-2 rounded-full shrink-0"
                  style={{ backgroundColor: chartTheme.teal }}
                  aria-hidden="true"
                />
                <span>{t.dashboard.avgLatencyShort}</span>
              </span>
              <span className="inline-flex items-center gap-1.5">
                <span
                  className="w-2 h-2.5 rounded-[2px] opacity-70 shrink-0"
                  style={{ backgroundColor: chartTheme.amber }}
                  aria-hidden="true"
                />
                <span>{t.dashboard.packetLossInline}</span>
              </span>
            </div>
          </div>
          <EChart option={qualityOption} heightClass="h-56" />
        </section>
      </div>

      {/* Bandwidth EChart + Server Summary & Recent Audit Stream */}
      <div className="grid grid-cols-1 lg:grid-cols-12 gap-4">
        <section className="lg:col-span-7 tss-panel p-5 flex flex-col justify-between gap-4">
          <div>
            <div className="flex flex-wrap items-center justify-between gap-2 mb-3">
              <h2 className="text-sm font-semibold text-text">{t.dashboard.chartBandwidth}</h2>
              <div className="flex flex-wrap items-center gap-3 text-xs text-text-secondary">
                <span className="inline-flex items-center gap-1.5">
                  <span
                    className="w-2 h-2 rounded-full shrink-0"
                    style={{ backgroundColor: chartTheme.cobalt }}
                    aria-hidden="true"
                  />
                  <span>{t.dashboard.outbound}</span>
                </span>
                <span className="inline-flex items-center gap-1.5">
                  <span
                    className="w-2 h-2 rounded-full shrink-0"
                    style={{ backgroundColor: chartTheme.sky }}
                    aria-hidden="true"
                  />
                  <span>{t.dashboard.inbound}</span>
                </span>
              </div>
            </div>
            <EChart option={bandwidthOption} heightClass="h-48" />
          </div>

          <dl className="pt-3.5 border-t border-border/70 grid grid-cols-2 sm:grid-cols-4 gap-3 text-xs">
            <div className="min-w-0">
              <dt className="text-text-muted">{t.common.name}</dt>
              <dd className="mt-1 font-medium text-text truncate" title={info?.name}>
                {info?.name ?? '—'}
              </dd>
            </div>
            <div>
              <dt className="text-text-muted">{t.dashboard.uptime}</dt>
              <dd className="mt-1 font-medium text-text tabular-nums">
                {info ? formatDuration(info.uptime_secs) : '—'}
              </dd>
            </div>
            <div>
              <dt className="text-text-muted">{t.dashboard.channelsCount}</dt>
              <dd className="mt-1 font-medium text-text tabular-nums">{activeChannelCount}</dd>
            </div>
            <div className="min-w-0">
              <dt className="text-text-muted">{t.dashboard.version}</dt>
              <dd className="mt-1 font-mono text-text-secondary truncate" title={info?.version}>
                {info?.version ?? '—'}
              </dd>
            </div>
          </dl>
        </section>

        <section className="lg:col-span-5 tss-panel p-5 flex flex-col">
          <div className="flex items-center justify-between gap-2 pb-3 mb-2 border-b border-border/70">
            <h2 className="text-sm font-semibold text-text">{t.dashboard.recentAudit}</h2>
            <Button variant="tertiary" size="xs" onClick={() => navigate('/governance')}>
              <span>{t.nav.governance}</span>
              <ArrowRight className="w-3.5 h-3.5" aria-hidden="true" />
            </Button>
          </div>
          {audits.length === 0 ? (
            <div className="flex-1 flex items-center justify-center py-4">
              <EmptyState
                compact
                icon={ShieldCheck}
                title={t.dashboard.recentAudit}
                description={t.common.noData}
              />
            </div>
          ) : (
            <ul className="divide-y divide-border/60 flex-1">
              {audits.map((a) => {
                const badge = formatAuditAction(a.action, locale);
                return (
                  <li
                    key={a.id}
                    className="py-2.5 flex items-center justify-between gap-3 text-sm"
                  >
                    <div className="flex items-center gap-2.5 min-w-0">
                      <span
                        aria-hidden="true"
                        className="w-7 h-7 rounded-[var(--radius-DEFAULT)] bg-surface-sunken border border-border text-[11px] font-mono font-semibold text-text-secondary grid place-items-center shrink-0 uppercase"
                      >
                        {a.actor_username.slice(0, 2)}
                      </span>
                      <div className="min-w-0">
                        <div className="flex flex-wrap items-center gap-1.5">
                          <span className="text-xs font-semibold text-text whitespace-nowrap">
                            {a.actor_username}
                          </span>
                          <StatusBadge
                            tone={badge.tone}
                            icon="none"
                            label={badge.label}
                          />
                        </div>
                        <div className="mt-0.5 text-xs text-text-muted truncate font-mono">
                          {a.target}
                        </div>
                      </div>
                    </div>
                    <span className="font-mono text-[11px] text-text-muted tabular-nums shrink-0">
                      {formatCompactTime(a.created_at)}
                    </span>
                  </li>
                );
              })}
            </ul>
          )}
        </section>
      </div>
    </div>
  );
}
