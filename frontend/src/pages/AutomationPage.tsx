import React, { useCallback, useEffect, useState } from 'react';
import { BellRing, Bot, Play, Plus, Send, Trash2 } from 'lucide-react';
import { useApp } from '../context/AppContext';
import { Dict, useI18n } from '../i18n';
import { apiRequest, AutomationRuleView, ChannelNode, WebhookView } from '../lib/api';
import { AlertBanner } from '../components/ui/AlertBanner';
import { Button } from '../components/ui/Button';
import { TableContainer, Td, Th } from '../components/ui/DataTable';
import { EmptyState } from '../components/ui/EmptyState';
import {
  ChoiceCardGroup,
  FormField,
  Input,
  Select,
  Textarea,
} from '../components/ui/FormControls';
import { ModalDialog } from '../components/ui/ModalDialog';
import { StatusBadge } from '../components/ui/StatusBadge';

function formatRuleSummary(r: AutomationRuleView, t: Dict): string {
  const cfg = r.config ?? {};
  if (r.rule_kind === 'afk_mover') {
    return t.automation.summaryAfk
      .replace('{secs}', String(cfg.max_idle_secs ?? 300))
      .replace('{cid}', String(cfg.target_channel_id ?? 1));
  }
  if (r.rule_kind === 'timed_broadcast') {
    return t.automation.summaryBroadcast
      .replace('{cron}', String(cfg.cron_expr ?? ''))
      .replace('{msg}', String(cfg.message ?? ''));
  }
  if (r.rule_kind === 'alert_packet_loss') {
    return t.automation.summaryLoss.replace('{pct}', String(cfg.threshold_pct ?? 5));
  }
  return t.automation.summaryOffline.replace('{cd}', String(cfg.cooldown_secs ?? 300));
}

function formatRuleLabel(kind: AutomationRuleView['rule_kind'], t: Dict): string {
  switch (kind) {
    case 'afk_mover':
      return t.automation.ruleAfk;
    case 'timed_broadcast':
      return t.automation.ruleBroadcast;
    case 'alert_packet_loss':
      return t.automation.rulePacketLoss;
    default:
      return t.automation.ruleOffline;
  }
}

function formatRunNotice(res: Record<string, unknown>, t: Dict): string {
  const kind = String(res.rule_kind ?? '');
  if (kind === 'afk_mover') {
    const count = Number(res.moved_count ?? 0);
    const names = Array.isArray(res.moved_clients) ? res.moved_clients.join(', ') : '';
    return count > 0
      ? t.automation.execAfkMoved.replace('{count}', String(count)).replace('{names}', names)
      : t.automation.execAfkNone;
  }
  if (kind === 'timed_broadcast') {
    return t.automation.execBroadcast;
  }
  return res.dispatched ? t.automation.execAlertDispatched : t.automation.execAlertIdle;
}

export function AutomationPage() {
  const { t, locale } = useI18n();
  const { user, instances, activeInstanceId, activeSid } = useApp();

  const [rules, setRules] = useState<AutomationRuleView[]>([]);
  const [webhooks, setWebhooks] = useState<WebhookView[]>([]);
  const [channels, setChannels] = useState<ChannelNode[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const [showAddRule, setShowAddRule] = useState(false);
  const [ruleName, setRuleName] = useState('');
  const [ruleKind, setRuleKind] = useState<AutomationRuleView['rule_kind']>('afk_mover');
  const [afkChannelId, setAfkChannelId] = useState('6');
  const [afkIdleSecs, setAfkIdleSecs] = useState('300');
  const [afkExcluded, setAfkExcluded] = useState('6');
  const [bcCron, setBcCron] = useState('0 * * * *');
  const [bcMessage, setBcMessage] = useState('');
  const [lossThreshold, setLossThreshold] = useState('5');

  const [showAddWebhook, setShowAddWebhook] = useState(false);
  const [whName, setWhName] = useState('');
  const [whUrl, setWhUrl] = useState('');
  const [whSecret, setWhSecret] = useState('');
  const [whEvents, setWhEvents] = useState('*');

  const isSuper = user?.global_role === 'super_admin';
  const activeInst = instances.find((i) => i.id === activeInstanceId);
  const canAdmin =
    activeInst?.my_role === 'super_admin' || activeInst?.my_role === 'server_admin';

  const loadData = useCallback(async () => {
    setError(null);
    try {
      const promises: Promise<unknown>[] = [];
      if (activeInstanceId) {
        promises.push(
          apiRequest<AutomationRuleView[]>(
            `/instances/${activeInstanceId}/servers/${activeSid}/automation-rules`,
          ).then(setRules),
          apiRequest<{ channels: ChannelNode[] }>(
            `/instances/${activeInstanceId}/servers/${activeSid}/tree`,
          )
            .then((res) => {
              setChannels(res.channels);
              const afk = res.channels.find((c) => c.channel_name.toLowerCase().includes('afk'));
              if (afk) setAfkChannelId(String(afk.cid));
            })
            .catch(() => {}),
        );
      }
      if (isSuper) {
        promises.push(apiRequest<WebhookView[]>('/webhooks').then(setWebhooks));
      }
      await Promise.all(promises);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load automation data');
    }
  }, [activeInstanceId, activeSid, isSuper]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const handleCreateRule = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!activeInstanceId) return;
    let config: Record<string, unknown> = {};
    if (ruleKind === 'afk_mover') {
      const excluded = afkExcluded
        .split(',')
        .map((s) => Number(s.trim()))
        .filter((n) => Number.isFinite(n) && n > 0);
      config = {
        target_channel_id: Number(afkChannelId) || 1,
        max_idle_secs: Number(afkIdleSecs) || 300,
        excluded_group_ids: excluded,
        only_when_muted_or_away: false,
      };
    } else if (ruleKind === 'timed_broadcast') {
      config = { cron_expr: bcCron.trim(), message: bcMessage.trim() };
    } else if (ruleKind === 'alert_packet_loss') {
      config = { threshold_pct: Number(lossThreshold) || 5, cooldown_secs: 300 };
    } else {
      config = { cooldown_secs: 300 };
    }
    await apiRequest(`/instances/${activeInstanceId}/servers/${activeSid}/automation-rules`, {
      method: 'POST',
      body: JSON.stringify({
        name: ruleName.trim(),
        rule_kind: ruleKind,
        config,
        enabled: true,
      }),
    });
    setShowAddRule(false);
    setRuleName('');
    setBcMessage('');
    await loadData();
  };

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-xl sm:text-2xl font-semibold text-text tracking-tight">{t.automation.title}</h1>
          <p className="mt-0.5 text-sm text-text-secondary tss-prose">{t.automation.subtitle}</p>
        </div>
        {canAdmin && activeInstanceId && rules.length > 0 ? (
          <Button variant="primary" onClick={() => setShowAddRule(true)}>
            <Plus className="w-4 h-4" aria-hidden="true" />
            <span>{t.automation.addRule}</span>
          </Button>
        ) : null}
      </div>

      {error ? (
        <AlertBanner tone="danger" onDismiss={() => setError(null)}>
          {error}
        </AlertBanner>
      ) : null}
      {notice ? (
        <AlertBanner tone="success" onDismiss={() => setNotice(null)}>
          {notice}
        </AlertBanner>
      ) : null}

      <section className="space-y-3.5">
        <h2 className="text-base font-semibold text-text">{t.automation.rulesTitle}</h2>
        {rules.length === 0 ? (
          <EmptyState
            icon={Bot}
            title={t.automation.rulesTitle}
            description={t.common.noData}
            action={
              canAdmin && activeInstanceId ? (
                <Button variant="primary" onClick={() => setShowAddRule(true)}>
                  <Plus className="w-4 h-4" aria-hidden="true" />
                  <span>{t.automation.addRule}</span>
                </Button>
              ) : undefined
            }
          />
        ) : (
          <TableContainer label={t.automation.rulesTitle}>
            <table className="tss-table tss-table-stack">
              <thead>
                <tr>
                  <Th>{t.common.name}</Th>
                  <Th>{t.automation.ruleKindLabel}</Th>
                  <Th>{t.automation.lastTriggered}</Th>
                  <Th>{t.common.status}</Th>
                  {canAdmin ? <Th numeric>{t.common.actions}</Th> : null}
                </tr>
              </thead>
              <tbody>
                {rules.map((r) => (
                  <tr key={r.id}>
                    <Td label={t.common.name}>
                      <div className="font-medium text-text">{r.name}</div>
                      <div className="text-xs text-text-muted max-w-md">
                        {formatRuleSummary(r, t)}
                      </div>
                    </Td>
                    <Td label={t.automation.ruleKindLabel}>
                      <StatusBadge
                        tone="action"
                        icon="none"
                        label={formatRuleLabel(r.rule_kind, t)}
                      />
                    </Td>
                    <Td label={t.automation.lastTriggered}>
                      <span className="text-xs text-text-muted tabular-nums whitespace-nowrap">
                        {r.last_triggered_at
                          ? new Date(r.last_triggered_at).toLocaleString()
                          : '—'}
                      </span>
                    </Td>
                    <Td label={t.common.status}>
                      <StatusBadge
                        variant="dot"
                        tone={r.enabled ? 'success' : 'neutral'}
                        label={r.enabled ? t.common.enabled : t.common.disabled}
                      />
                    </Td>
                    {canAdmin ? (
                      <Td numeric label={t.common.actions}>
                        <div className="inline-flex items-center gap-1.5 justify-end">
                          <Button
                            size="xs"
                            variant="secondary"
                            onClick={async () => {
                              const res = await apiRequest<Record<string, unknown>>(
                                `/instances/${activeInstanceId}/servers/${activeSid}/automation-rules/${r.id}/run-now`,
                                { method: 'POST' },
                              );
                              setNotice(formatRunNotice(res, t));
                              await loadData();
                            }}
                          >
                            <Play className="w-3.5 h-3.5" aria-hidden="true" />
                            <span>{t.automation.runNow}</span>
                          </Button>
                          <Button
                            size="xs"
                            variant="danger-link"
                            aria-label={t.common.delete}
                            onClick={async () => {
                              await apiRequest(
                                `/instances/${activeInstanceId}/servers/${activeSid}/automation-rules/${r.id}`,
                                { method: 'DELETE' },
                              );
                              await loadData();
                            }}
                          >
                            <Trash2 className="w-3.5 h-3.5" aria-hidden="true" />
                          </Button>
                        </div>
                      </Td>
                    ) : null}
                  </tr>
                ))}
              </tbody>
            </table>
          </TableContainer>
        )}
      </section>

      {isSuper ? (
        <section className="space-y-3.5">
          <div className="flex items-center justify-between gap-3">
            <h2 className="text-base font-semibold text-text">{t.automation.webhooksTitle}</h2>
            {webhooks.length > 0 ? (
              <Button size="sm" variant="secondary" onClick={() => setShowAddWebhook(true)}>
                <Plus className="w-3.5 h-3.5" aria-hidden="true" />
                <span>{t.automation.addWebhook}</span>
              </Button>
            ) : null}
          </div>
          {webhooks.length === 0 ? (
            <EmptyState
              compact
              icon={BellRing}
              title={locale === 'zh-CN' ? '暂未注册回调端点' : 'No Webhook Endpoints Yet'}
              description={t.automation.webhooksEmptyDesc}
              action={
                <Button size="sm" variant="secondary" onClick={() => setShowAddWebhook(true)}>
                  <Plus className="w-3.5 h-3.5" aria-hidden="true" />
                  <span>{t.automation.addWebhook}</span>
                </Button>
              }
            />
          ) : (
            <TableContainer label={t.automation.webhooksTitle}>
              <table className="tss-table tss-table-stack">
                <thead>
                  <tr>
                    <Th>{t.common.name}</Th>
                    <Th>{t.automation.whUrl}</Th>
                    <Th>{t.automation.whEvents}</Th>
                    <Th>{t.automation.lastStatus}</Th>
                    <Th numeric>{t.common.actions}</Th>
                  </tr>
                </thead>
                <tbody>
                  {webhooks.map((w) => (
                    <tr key={w.id}>
                      <Td label={t.common.name}>{w.name}</Td>
                      <Td label={t.automation.whUrl}>
                        <span className="font-mono text-xs text-text-muted break-all">{w.url}</span>
                      </Td>
                      <Td label={t.automation.whEvents}>
                        <span className="font-mono text-xs">{w.events_csv}</span>
                      </Td>
                      <Td label={t.automation.lastStatus}>{w.last_delivery_status || '—'}</Td>
                      <Td numeric label={t.common.actions}>
                        <div className="inline-flex gap-1.5 justify-end">
                          <Button
                            size="xs"
                            variant="secondary"
                            onClick={async () => {
                              const res = await apiRequest<{ delivery_status: string }>(
                                `/webhooks/${w.id}/test`,
                                { method: 'POST' },
                              );
                              setNotice(
                                t.automation.whTestResult.replace('{status}', res.delivery_status),
                              );
                              await loadData();
                            }}
                          >
                            <Send className="w-3.5 h-3.5" aria-hidden="true" />
                            <span>{t.automation.testWebhook}</span>
                          </Button>
                          <Button
                            size="xs"
                            variant="danger-link"
                            onClick={async () => {
                              await apiRequest(`/webhooks/${w.id}`, { method: 'DELETE' });
                              await loadData();
                            }}
                          >
                            {t.common.delete}
                          </Button>
                        </div>
                      </Td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </TableContainer>
          )}
        </section>
      ) : null}

      {/* Add Automation Rule Modal with ChoiceCardGroup */}
      <ModalDialog
        open={showAddRule}
        onClose={() => setShowAddRule(false)}
        title={t.automation.addRule}
        maxWidthClass="max-w-xl"
      >
        <form onSubmit={handleCreateRule}>
          <FormField label={t.common.name} htmlFor="rule-name" required className="max-w-none">
            <Input
              id="rule-name"
              required
              value={ruleName}
              onChange={(e) => setRuleName(e.target.value)}
            />
          </FormField>

          <ChoiceCardGroup
            name="auto-rule-kind"
            label={t.automation.ruleKindLabel}
            value={ruleKind}
            onChange={setRuleKind}
            columnsClass="grid-cols-1 sm:grid-cols-2"
            options={[
              {
                value: 'afk_mover',
                title: t.automation.ruleAfk,
                description: t.automation.ruleAfkDesc,
              },
              {
                value: 'timed_broadcast',
                title: t.automation.ruleBroadcast,
                description: t.automation.ruleBroadcastDesc,
              },
              {
                value: 'alert_offline',
                title: t.automation.ruleOffline,
                description: t.automation.ruleOfflineDesc,
              },
              {
                value: 'alert_packet_loss',
                title: t.automation.rulePacketLoss,
                description: t.automation.rulePacketLossDesc,
              },
            ]}
          />

          {ruleKind === 'afk_mover' ? (
            <>
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                <FormField label={t.automation.afkTargetChannel} htmlFor="afk-ch" className="max-w-none">
                  <Select
                    id="afk-ch"
                    value={afkChannelId}
                    onChange={(e) => setAfkChannelId(e.target.value)}
                  >
                    {channels.map((c) => (
                      <option key={c.cid} value={c.cid}>
                        #{c.cid} {c.channel_name}
                      </option>
                    ))}
                  </Select>
                </FormField>
                <FormField label={t.automation.afkIdleSecs} htmlFor="afk-secs" className="max-w-none">
                  <Input
                    id="afk-secs"
                    type="number"
                    min={10}
                    value={afkIdleSecs}
                    onChange={(e) => setAfkIdleSecs(e.target.value)}
                  />
                </FormField>
              </div>
              <FormField
                label={t.automation.afkExcludedGroups}
                htmlFor="afk-excl"
                hint={t.automation.afkExcludedHint}
                className="max-w-none"
              >
                <Input
                  id="afk-excl"
                  value={afkExcluded}
                  onChange={(e) => setAfkExcluded(e.target.value)}
                />
              </FormField>
            </>
          ) : null}

          {ruleKind === 'timed_broadcast' ? (
            <>
              <FormField label={t.automation.bcCron} htmlFor="bc-cron" required className="max-w-none">
                <Input
                  id="bc-cron"
                  required
                  className="font-mono"
                  value={bcCron}
                  onChange={(e) => setBcCron(e.target.value)}
                />
              </FormField>
              <FormField label={t.automation.bcMessage} htmlFor="bc-msg" required className="max-w-none">
                <Textarea
                  id="bc-msg"
                  required
                  value={bcMessage}
                  onChange={(e) => setBcMessage(e.target.value)}
                />
              </FormField>
            </>
          ) : null}

          {ruleKind === 'alert_packet_loss' ? (
            <FormField label={t.automation.lossThreshold} htmlFor="loss-pct" required className="max-w-none">
              <Input
                id="loss-pct"
                type="number"
                step="0.1"
                required
                value={lossThreshold}
                onChange={(e) => setLossThreshold(e.target.value)}
              />
            </FormField>
          ) : null}

          <div className="flex justify-end gap-2 pt-2 border-t border-border">
            <Button variant="tertiary" onClick={() => setShowAddRule(false)}>
              {t.common.cancel}
            </Button>
            <Button type="submit" variant="primary">
              {t.common.save}
            </Button>
          </div>
        </form>
      </ModalDialog>

      {/* Add Webhook Modal */}
      <ModalDialog
        open={showAddWebhook}
        onClose={() => setShowAddWebhook(false)}
        title={t.automation.addWebhook}
      >
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            await apiRequest('/webhooks', {
              method: 'POST',
              body: JSON.stringify({
                name: whName.trim(),
                url: whUrl.trim(),
                signing_secret: whSecret.trim() || null,
                events_csv: whEvents.trim() || '*',
                enabled: true,
              }),
            });
            setShowAddWebhook(false);
            setWhName('');
            setWhUrl('');
            setWhSecret('');
            await loadData();
          }}
        >
          <FormField label={t.common.name} htmlFor="wh-name" required>
            <Input
              id="wh-name"
              required
              value={whName}
              onChange={(e) => setWhName(e.target.value)}
            />
          </FormField>
          <FormField label={t.automation.whUrl} htmlFor="wh-url" required>
            <Input
              id="wh-url"
              type="url"
              required
              value={whUrl}
              onChange={(e) => setWhUrl(e.target.value)}
              placeholder="https://hooks.example.com/tss"
            />
          </FormField>
          <FormField label={t.automation.whSecret} htmlFor="wh-sec">
            <Input
              id="wh-sec"
              type="password"
              value={whSecret}
              onChange={(e) => setWhSecret(e.target.value)}
            />
          </FormField>
          <FormField label={t.automation.whEvents} htmlFor="wh-evt">
            <Input
              id="wh-evt"
              value={whEvents}
              onChange={(e) => setWhEvents(e.target.value)}
            />
          </FormField>
          <div className="flex justify-end gap-2">
            <Button variant="tertiary" onClick={() => setShowAddWebhook(false)}>
              {t.common.cancel}
            </Button>
            <Button type="submit" variant="primary">
              {t.common.save}
            </Button>
          </div>
        </form>
      </ModalDialog>
    </div>
  );
}
