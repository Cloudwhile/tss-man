import { useCallback, useEffect, useState } from 'react';
import {
  Clock,
  Download,
  FolderArchive,
  GitCompare,
  Minus,
  Plus,
  RotateCcw,
  Trash2,
  Upload,
} from 'lucide-react';
import { useApp } from '../context/AppContext';
import { useI18n } from '../i18n';
import {
  apiRequest,
  BackupPolicyView,
  SnapshotDiffResp,
  SnapshotView,
} from '../lib/api';
import { formatCompactTime, formatTriggerKind } from '../lib/format';
import { AlertBanner } from '../components/ui/AlertBanner';
import { Button } from '../components/ui/Button';
import { TableContainer, Td, Th } from '../components/ui/DataTable';
import { EmptyState } from '../components/ui/EmptyState';
import { FormField, Input, Textarea } from '../components/ui/FormControls';
import { ModalDialog } from '../components/ui/ModalDialog';
import { PopoverMenu } from '../components/ui/PopoverMenu';
import { StatusBadge } from '../components/ui/StatusBadge';

export function SnapshotsPage() {
  const { t, locale } = useI18n();
  const { instances, activeInstanceId, activeSid } = useApp();

  const [snapshots, setSnapshots] = useState<SnapshotView[]>([]);
  const [policies, setPolicies] = useState<BackupPolicyView[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const [showCreate, setShowCreate] = useState(false);
  const [snapName, setSnapName] = useState('');

  const [showImport, setShowImport] = useState(false);
  const [importJson, setImportJson] = useState('');

  const [diffResult, setDiffResult] = useState<SnapshotDiffResp | null>(null);
  const [deployTarget, setDeployTarget] = useState<SnapshotView | null>(null);

  const [showAddPolicy, setShowAddPolicy] = useState(false);
  const [cronExpr, setCronExpr] = useState('0 3 * * *');
  const [retention, setRetention] = useState('7');

  const activeInst = instances.find((i) => i.id === activeInstanceId);
  const canAdmin =
    activeInst?.my_role === 'super_admin' || activeInst?.my_role === 'server_admin';

  const loadData = useCallback(async () => {
    if (!activeInstanceId) return;
    setError(null);
    try {
      const [snaps, pols] = await Promise.all([
        apiRequest<SnapshotView[]>(
          `/instances/${activeInstanceId}/servers/${activeSid}/snapshots`,
        ),
        apiRequest<BackupPolicyView[]>(
          `/instances/${activeInstanceId}/servers/${activeSid}/backup-policies`,
        ),
      ]);
      setSnapshots(snaps);
      setPolicies(pols);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load snapshots');
    }
  }, [activeInstanceId, activeSid]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  if (!activeInstanceId) {
    return (
      <EmptyState
        icon={FolderArchive}
        title={t.workbench.emptyTitle}
        description={t.workbench.emptyDesc}
      />
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-xl sm:text-2xl font-semibold text-text tracking-tight">{t.snapshots.title}</h1>
          <p className="mt-0.5 text-sm text-text-secondary tss-prose">{t.snapshots.subtitle}</p>
        </div>
        {canAdmin && snapshots.length > 0 ? (
          <div className="flex flex-wrap gap-2">
            <Button variant="secondary" onClick={() => setShowImport(true)}>
              <Upload className="w-4 h-4" aria-hidden="true" />
              <span>{t.snapshots.importSnap}</span>
            </Button>
            <Button variant="primary" onClick={() => setShowCreate(true)}>
              <Plus className="w-4 h-4" aria-hidden="true" />
              <span>{t.snapshots.createSnap}</span>
            </Button>
          </div>
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

      {snapshots.length === 0 ? (
        <EmptyState
          icon={FolderArchive}
          title={t.snapshots.title}
          description={t.common.noData}
          action={
            canAdmin ? (
              <>
                <Button variant="secondary" onClick={() => setShowImport(true)}>
                  <Upload className="w-4 h-4" aria-hidden="true" />
                  <span>{t.snapshots.importSnap}</span>
                </Button>
                <Button variant="primary" onClick={() => setShowCreate(true)}>
                  <Plus className="w-4 h-4" aria-hidden="true" />
                  <span>{t.snapshots.createSnap}</span>
                </Button>
              </>
            ) : undefined
          }
        />
      ) : (
        <TableContainer label={t.snapshots.title}>
          <table className="tss-table tss-table-stack">
            <thead>
              <tr>
                <Th>{t.common.name}</Th>
                <Th>{t.common.type}</Th>
                <Th numeric>{t.snapshots.channelsCol}</Th>
                <Th numeric>{t.snapshots.groupsCol}</Th>
                <Th>{t.common.createdAt}</Th>
                <Th numeric>{t.common.actions}</Th>
              </tr>
            </thead>
            <tbody>
              {snapshots.map((s) => {
                const trig = formatTriggerKind(s.trigger_kind, locale);
                return (
                <tr key={s.id}>
                  <Td label={t.common.name}>
                    <div className="font-semibold text-text">{s.name}</div>
                    <div className="text-xs text-text-muted">{s.summary.server_name}</div>
                  </Td>
                  <Td label={t.common.type}>
                    <StatusBadge
                      tone={trig.tone}
                      icon="none"
                      label={trig.label}
                    />
                  </Td>
                  <Td numeric label={t.snapshots.channelsCol}>
                    {s.summary.channel_count}
                  </Td>
                  <Td numeric label={t.snapshots.groupsCol}>
                    {s.summary.group_count}
                  </Td>
                  <Td label={t.common.createdAt}>
                    <span className="font-mono text-xs text-text-muted tabular-nums whitespace-nowrap">
                      {formatCompactTime(s.created_at)}
                    </span>
                  </Td>
                  <Td numeric label={t.common.actions}>
                    <div className="inline-flex items-center gap-1.5 justify-end whitespace-nowrap">
                      <Button
                        size="xs"
                        variant="secondary"
                        onClick={async () => {
                          const diff = await apiRequest<SnapshotDiffResp>(
                            `/instances/${activeInstanceId}/servers/${activeSid}/snapshots/${s.id}/diff?against=live`,
                          );
                          setDiffResult(diff);
                        }}
                      >
                        <GitCompare className="w-3.5 h-3.5" aria-hidden="true" />
                        <span>{t.snapshots.diffBtn}</span>
                      </Button>
                      {canAdmin ? (
                        <>
                          <Button
                            size="xs"
                            variant="secondary"
                            onClick={() => setDeployTarget(s)}
                          >
                            <RotateCcw className="w-3.5 h-3.5" aria-hidden="true" />
                            <span>{t.snapshots.deployBtn}</span>
                          </Button>
                          <PopoverMenu
                            iconOnly
                            triggerLabel={t.common.moreActions}
                            items={[
                              {
                                label: t.snapshots.exportBtn,
                                icon: <Download className="w-3.5 h-3.5" aria-hidden="true" />,
                                onClick: async () => {
                                  const doc = await apiRequest<Record<string, unknown>>(
                                    `/instances/${activeInstanceId}/servers/${activeSid}/snapshots/${s.id}/export`,
                                  );
                                  const blob = new Blob([JSON.stringify(doc, null, 2)], {
                                    type: 'application/json',
                                  });
                                  const url = URL.createObjectURL(blob);
                                  const a = document.createElement('a');
                                  a.href = url;
                                  a.download = `snapshot-${s.id}.tssnap.json`;
                                  a.click();
                                  URL.revokeObjectURL(url);
                                },
                              },
                              {
                                label: t.common.delete,
                                danger: true,
                                dividerBefore: true,
                                icon: <Trash2 className="w-3.5 h-3.5" aria-hidden="true" />,
                                onClick: async () => {
                                  await apiRequest(
                                    `/instances/${activeInstanceId}/servers/${activeSid}/snapshots/${s.id}`,
                                    { method: 'DELETE' },
                                  );
                                  await loadData();
                                },
                              },
                            ]}
                          />
                        </>
                      ) : null}
                    </div>
                  </Td>
                </tr>
                );
              })}
            </tbody>
          </table>
        </TableContainer>
      )}

      {/* Backup Policies */}
      <section className="space-y-3.5">
        <div className="flex items-center justify-between gap-3">
          <h2 className="text-base font-semibold text-text">{t.snapshots.policiesTitle}</h2>
          {canAdmin && policies.length > 0 ? (
            <Button size="sm" variant="secondary" onClick={() => setShowAddPolicy(true)}>
              <Plus className="w-3.5 h-3.5" aria-hidden="true" />
              <span>{t.snapshots.addPolicy}</span>
            </Button>
          ) : null}
        </div>
        {policies.length === 0 ? (
          <EmptyState
            compact
            icon={Clock}
            title={locale === 'zh-CN' ? '尚无定时备份计划' : 'No Backup Schedules Yet'}
            description={t.snapshots.policiesDesc}
            action={
              canAdmin ? (
                <Button size="sm" variant="secondary" onClick={() => setShowAddPolicy(true)}>
                  <Plus className="w-3.5 h-3.5" aria-hidden="true" />
                  <span>{t.snapshots.addPolicy}</span>
                </Button>
              ) : undefined
            }
          />
        ) : (
          <TableContainer label={t.snapshots.policiesTitle}>
            <table className="tss-table tss-table-stack">
              <thead>
                <tr>
                  <Th>{t.snapshots.cronExpr}</Th>
                  <Th numeric>{t.snapshots.retention}</Th>
                  <Th>{t.snapshots.lastRun}</Th>
                  <Th>{t.common.status}</Th>
                  {canAdmin ? <Th numeric>{t.common.actions}</Th> : null}
                </tr>
              </thead>
              <tbody>
                {policies.map((p) => (
                  <tr key={p.id}>
                    <Td label={t.snapshots.cronExpr}>
                      <span className="font-mono text-xs">{p.cron_expr}</span>
                    </Td>
                    <Td numeric label={t.snapshots.retention}>
                      {p.retention_count}
                    </Td>
                    <Td label={t.snapshots.lastRun}>
                      <span className="text-xs text-text-muted tabular-nums">
                        {p.last_run_at ? new Date(p.last_run_at).toLocaleString() : '—'}
                      </span>
                    </Td>
                    <Td label={t.common.status}>
                      <StatusBadge
                        variant="dot"
                        tone={p.enabled ? 'success' : 'neutral'}
                        label={p.enabled ? t.common.enabled : t.common.disabled}
                      />
                    </Td>
                    {canAdmin ? (
                      <Td numeric label={t.common.actions}>
                        <Button
                          size="xs"
                          variant="danger-link"
                          onClick={async () => {
                            await apiRequest(
                              `/instances/${activeInstanceId}/servers/${activeSid}/backup-policies/${p.id}`,
                              { method: 'DELETE' },
                            );
                            await loadData();
                          }}
                        >
                          {t.common.delete}
                        </Button>
                      </Td>
                    ) : null}
                  </tr>
                ))}
              </tbody>
            </table>
          </TableContainer>
        )}
      </section>

      {/* Create Snapshot Modal */}
      <ModalDialog
        open={showCreate}
        onClose={() => setShowCreate(false)}
        title={t.snapshots.createSnap}
      >
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            await apiRequest(`/instances/${activeInstanceId}/servers/${activeSid}/snapshots`, {
              method: 'POST',
              body: JSON.stringify({ name: snapName.trim() || null }),
            });
            setShowCreate(false);
            setSnapName('');
            await loadData();
          }}
        >
          <FormField label={t.common.name} htmlFor="snap-name">
            <Input
              id="snap-name"
              value={snapName}
              onChange={(e) => setSnapName(e.target.value)}
              placeholder="Pre-maintenance backup"
            />
          </FormField>
          <div className="flex justify-end gap-2">
            <Button variant="tertiary" onClick={() => setShowCreate(false)}>
              {t.common.cancel}
            </Button>
            <Button type="submit" variant="primary">
              {t.common.create}
            </Button>
          </div>
        </form>
      </ModalDialog>

      {/* Import Snapshot Modal */}
      <ModalDialog
        open={showImport}
        onClose={() => setShowImport(false)}
        title={t.snapshots.importSnap}
      >
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            const parsed = JSON.parse(importJson);
            await apiRequest(
              `/instances/${activeInstanceId}/servers/${activeSid}/snapshots/import`,
              {
                method: 'POST',
                body: JSON.stringify({
                  name: parsed.name || 'Imported Snapshot',
                  version: parsed.version || '1',
                  data: parsed.data,
                }),
              },
            );
            setShowImport(false);
            setImportJson('');
            await loadData();
          }}
        >
          <FormField label={t.snapshots.importJsonLabel} htmlFor="snap-import-json" required>
            <Textarea
              id="snap-import-json"
              required
              value={importJson}
              onChange={(e) => setImportJson(e.target.value)}
            />
          </FormField>
          <div className="flex justify-end gap-2">
            <Button variant="tertiary" onClick={() => setShowImport(false)}>
              {t.common.cancel}
            </Button>
            <Button type="submit" variant="primary">
              {t.snapshots.importSnap}
            </Button>
          </div>
        </form>
      </ModalDialog>

      {/* Structured Visual Diff Modal */}
      <ModalDialog
        open={diffResult !== null}
        onClose={() => setDiffResult(null)}
        title={`${t.snapshots.diffBtn}: ${diffResult?.snapshot_name ?? ''}`}
        subtitle={`vs ${diffResult?.against_label ?? ''}`}
        maxWidthClass="max-w-2xl"
      >
        {diffResult ? (
          <div className="space-y-5 text-sm">
            <div className="tss-well grid grid-cols-2 divide-x divide-border">
              <div className="p-3.5">
                <div className="text-xs text-text-muted">{t.snapshots.snapLabel}</div>
                <div className="mt-1 font-semibold text-text tabular-nums">
                  {diffResult.snapshot_summary.channel_count} {t.snapshots.channelsCol} ·{' '}
                  {diffResult.snapshot_summary.group_count} {t.snapshots.groupsCol}
                </div>
              </div>
              <div className="p-3.5">
                <div className="text-xs text-text-muted">{diffResult.against_label}</div>
                <div className="mt-1 font-semibold text-text tabular-nums">
                  {diffResult.target_summary.channel_count} {t.snapshots.channelsCol} ·{' '}
                  {diffResult.target_summary.group_count} {t.snapshots.groupsCol}
                </div>
              </div>
            </div>

            <div>
              <h2 className="text-xs font-semibold uppercase tracking-wide text-text-muted mb-2">
                {t.snapshots.addedInLive}
              </h2>
              {diffResult.channels_added_in_target.length === 0 ? (
                <p className="text-text-muted text-xs">{t.common.none}</p>
              ) : (
                <ul className="space-y-1.5 list-none p-0 m-0">
                  {diffResult.channels_added_in_target.map((c) => (
                    <li
                      key={c}
                      className="flex items-center gap-2 px-3 py-1.5 rounded-r-[var(--radius-sm)] border-l-2 border-success bg-success-subtle text-success-text font-mono text-xs"
                    >
                      <Plus className="w-3.5 h-3.5 shrink-0" aria-hidden="true" />
                      <span>{c}</span>
                    </li>
                  ))}
                </ul>
              )}
            </div>

            <div>
              <h2 className="text-xs font-semibold uppercase tracking-wide text-text-muted mb-2">
                {t.snapshots.removedFromLive}
              </h2>
              {diffResult.channels_removed_from_target.length === 0 ? (
                <p className="text-text-muted text-xs">{t.common.none}</p>
              ) : (
                <ul className="space-y-1.5 list-none p-0 m-0">
                  {diffResult.channels_removed_from_target.map((c) => (
                    <li
                      key={c}
                      className="flex items-center gap-2 px-3 py-1.5 rounded-r-[var(--radius-sm)] border-l-2 border-danger bg-danger-subtle text-danger-text font-mono text-xs"
                    >
                      <Minus className="w-3.5 h-3.5 shrink-0" aria-hidden="true" />
                      <span>{c}</span>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </div>
        ) : null}
      </ModalDialog>

      {/* Rollback Confirmation Modal */}
      <ModalDialog
        open={deployTarget !== null}
        onClose={() => setDeployTarget(null)}
        title={t.snapshots.deployConfirmTitle}
      >
        <AlertBanner tone="warning" className="mb-5">
          {t.snapshots.deployConfirmDesc}
        </AlertBanner>
        <div className="flex justify-end gap-3">
          <Button variant="tertiary" autoFocus onClick={() => setDeployTarget(null)}>
            {t.common.cancel}
          </Button>
          <Button
            variant="danger-confirm"
            onClick={async () => {
              if (!deployTarget) return;
              await apiRequest(
                `/instances/${activeInstanceId}/servers/${activeSid}/snapshots/${deployTarget.id}/deploy`,
                { method: 'POST' },
              );
              setDeployTarget(null);
              setNotice(t.snapshots.deployedNotice);
              await loadData();
            }}
          >
            {t.snapshots.deployBtn}
          </Button>
        </div>
      </ModalDialog>

      {/* Add Backup Policy Modal */}
      <ModalDialog
        open={showAddPolicy}
        onClose={() => setShowAddPolicy(false)}
        title={t.snapshots.addPolicy}
      >
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            await apiRequest(
              `/instances/${activeInstanceId}/servers/${activeSid}/backup-policies`,
              {
                method: 'POST',
                body: JSON.stringify({
                  cron_expr: cronExpr.trim(),
                  retention_count: Number(retention) || 7,
                  enabled: true,
                }),
              },
            );
            setShowAddPolicy(false);
            await loadData();
          }}
        >
          <FormField
            label={t.snapshots.cronExpr}
            htmlFor="pol-cron"
            hint={t.snapshots.cronHint}
            required
          >
            <Input
              id="pol-cron"
              required
              className="font-mono"
              value={cronExpr}
              onChange={(e) => setCronExpr(e.target.value)}
            />
          </FormField>
          <FormField label={t.snapshots.retention} htmlFor="pol-ret" required>
            <Input
              id="pol-ret"
              type="number"
              min={1}
              max={100}
              required
              value={retention}
              onChange={(e) => setRetention(e.target.value)}
            />
          </FormField>
          <div className="flex justify-end gap-2">
            <Button variant="tertiary" onClick={() => setShowAddPolicy(false)}>
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
