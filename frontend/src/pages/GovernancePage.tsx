import React, { useCallback, useEffect, useState } from 'react';
import { Ban, Plus, ShieldAlert } from 'lucide-react';
import { useApp } from '../context/AppContext';
import { useI18n } from '../i18n';
import {
  apiRequest,
  AuditLogView,
  BanRule,
  ComplaintEntry,
  DbClient,
  TsLogEntry,
} from '../lib/api';
import { formatAuditAction, formatCompactTime } from '../lib/format';
import { AlertBanner } from '../components/ui/AlertBanner';
import { Button } from '../components/ui/Button';
import { CopyButton } from '../components/ui/CopyButton';
import { TableContainer, Td, Th } from '../components/ui/DataTable';
import { EmptyState } from '../components/ui/EmptyState';
import { FormField, Input } from '../components/ui/FormControls';
import { ModalDialog } from '../components/ui/ModalDialog';
import { StatusBadge } from '../components/ui/StatusBadge';
import { TabBar } from '../components/ui/Tabs';

export function GovernancePage() {
  const { t, locale } = useI18n();
  const { instances, activeInstanceId, activeSid } = useApp();

  const [tab, setTab] = useState<'bans' | 'db_clients' | 'complaints' | 'ts_logs' | 'audit'>(
    'bans',
  );
  const [bans, setBans] = useState<BanRule[]>([]);
  const [dbClients, setDbClients] = useState<DbClient[]>([]);
  const [dbSearch, setDbSearch] = useState('');
  const [complaints, setComplaints] = useState<ComplaintEntry[]>([]);
  const [tsLogs, setTsLogs] = useState<TsLogEntry[]>([]);
  const [logLevelFilter, setLogLevelFilter] = useState<'ALL' | 'INFO' | 'WARN' | 'ERR'>('ALL');
  const [auditLogs, setAuditLogs] = useState<AuditLogView[]>([]);
  const [error, setError] = useState<string | null>(null);

  const [showAddBan, setShowAddBan] = useState(false);
  const [banIp, setBanIp] = useState('');
  const [banName, setBanName] = useState('');
  const [banUid, setBanUid] = useState('');
  const [banDuration, setBanDuration] = useState('0');
  const [banReason, setBanReason] = useState('');

  const activeInst = instances.find((i) => i.id === activeInstanceId);
  const canModerate =
    activeInst?.my_role === 'super_admin' ||
    activeInst?.my_role === 'server_admin' ||
    activeInst?.my_role === 'moderator';
  const canAdmin =
    activeInst?.my_role === 'super_admin' || activeInst?.my_role === 'server_admin';

  const loadTab = useCallback(async () => {
    setError(null);
    try {
      if (tab === 'audit') {
        const res = await apiRequest<AuditLogView[]>('/audit-logs?limit=150');
        setAuditLogs(res);
        return;
      }
      if (!activeInstanceId) return;
      if (tab === 'bans') {
        setBans(
          await apiRequest<BanRule[]>(`/instances/${activeInstanceId}/servers/${activeSid}/bans`),
        );
      } else if (tab === 'db_clients') {
        const q = dbSearch.trim() ? `&q=${encodeURIComponent(dbSearch.trim())}` : '';
        setDbClients(
          await apiRequest<DbClient[]>(
            `/instances/${activeInstanceId}/servers/${activeSid}/db-clients?limit=60${q}`,
          ),
        );
      } else if (tab === 'complaints') {
        setComplaints(
          await apiRequest<ComplaintEntry[]>(
            `/instances/${activeInstanceId}/servers/${activeSid}/complaints`,
          ),
        );
      } else if (tab === 'ts_logs') {
        setTsLogs(
          await apiRequest<TsLogEntry[]>(
            `/instances/${activeInstanceId}/servers/${activeSid}/ts-logs?lines=80`,
          ),
        );
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load governance data');
    }
  }, [activeInstanceId, activeSid, dbSearch, tab]);

  useEffect(() => {
    loadTab();
  }, [loadTab]);

  const handleCreateBan = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!activeInstanceId) return;
    await apiRequest(`/instances/${activeInstanceId}/servers/${activeSid}/bans`, {
      method: 'POST',
      body: JSON.stringify({
        ip: banIp.trim() || null,
        name: banName.trim() || null,
        uid: banUid.trim() || null,
        duration_secs: Number(banDuration) || 0,
        reason: banReason.trim() || null,
      }),
    });
    setShowAddBan(false);
    setBanIp('');
    setBanName('');
    setBanUid('');
    setBanReason('');
    await loadTab();
  };

  const filteredTsLogs = tsLogs.filter((l) => {
    if (logLevelFilter === 'ALL') return true;
    return l.level.toUpperCase().includes(logLevelFilter);
  });

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-xl sm:text-2xl font-semibold text-text tracking-tight">{t.governance.title}</h1>
          <p className="mt-0.5 text-sm text-text-secondary tss-prose">{t.governance.subtitle}</p>
        </div>
        {tab === 'bans' && canModerate && activeInstanceId && bans.length > 0 ? (
          <Button variant="primary" onClick={() => setShowAddBan(true)}>
            <Plus className="w-4 h-4" aria-hidden="true" />
            <span>{t.governance.addBan}</span>
          </Button>
        ) : null}
      </div>

      {error ? (
        <AlertBanner tone="danger" onDismiss={() => setError(null)}>
          {error}
        </AlertBanner>
      ) : null}

      <TabBar
        idPrefix="gov"
        ariaLabel={t.governance.title}
        active={tab}
        onChange={setTab}
        items={[
          { id: 'bans', label: t.governance.tabBans },
          { id: 'db_clients', label: t.governance.tabDbClients },
          { id: 'complaints', label: t.governance.tabComplaints },
          { id: 'ts_logs', label: t.governance.tabTsLogs },
          { id: 'audit', label: t.governance.tabAuditLogs },
        ]}
      />

      {tab === 'bans' ? (
        <div role="tabpanel" aria-labelledby="gov-tab-bans">
          {bans.length === 0 ? (
            <EmptyState
              icon={Ban}
              title={t.governance.tabBans}
              description={t.common.noData}
              action={
                canModerate && activeInstanceId ? (
                  <Button variant="primary" onClick={() => setShowAddBan(true)}>
                    <Plus className="w-4 h-4" aria-hidden="true" />
                    <span>{t.governance.addBan}</span>
                  </Button>
                ) : undefined
              }
            />
          ) : (
            <TableContainer label={t.governance.tabBans}>
              <table className="tss-table tss-table-stack">
                <thead>
                  <tr>
                    <Th>ID</Th>
                    <Th>{t.governance.banTarget}</Th>
                    <Th>{t.governance.banReason}</Th>
                    <Th>{t.governance.invoker}</Th>
                    <Th numeric>{t.governance.enforcements}</Th>
                    {canModerate ? <Th numeric>{t.common.actions}</Th> : null}
                  </tr>
                </thead>
                <tbody>
                  {bans.map((b) => (
                    <tr key={b.banid}>
                      <Td label="ID">
                        <span className="font-mono text-xs">#{b.banid}</span>
                      </Td>
                      <Td label={t.governance.banTarget}>
                        <div className="font-mono text-xs space-y-0.5 min-w-0">
                          {b.ip ? <div>IP {b.ip}</div> : null}
                          {b.name ? <div>Name {b.name}</div> : null}
                          {b.uid ? (
                            <div className="break-all sm:truncate sm:max-w-56">UID {b.uid}</div>
                          ) : null}
                        </div>
                      </Td>
                      <Td label={t.governance.banReason}>{b.reason || '—'}</Td>
                      <Td label={t.governance.invoker}>{b.invoker_name}</Td>
                      <Td numeric label={t.governance.enforcements}>
                        {b.enforcements}
                      </Td>
                      {canModerate ? (
                        <Td numeric label={t.common.actions}>
                          <Button
                            size="xs"
                            variant="danger-link"
                            onClick={async () => {
                              await apiRequest(
                                `/instances/${activeInstanceId}/servers/${activeSid}/bans/${b.banid}`,
                                { method: 'DELETE' },
                              );
                              await loadTab();
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
        </div>
      ) : tab === 'db_clients' ? (
        <div role="tabpanel" aria-labelledby="gov-tab-db_clients" className="space-y-4">
          <search className="max-w-md flex gap-2">
            <Input
              type="search"
              placeholder={t.governance.searchClient}
              value={dbSearch}
              onChange={(e) => setDbSearch(e.target.value)}
            />
            <Button variant="secondary" onClick={loadTab}>
              {t.common.search}
            </Button>
          </search>
          <TableContainer label={t.governance.tabDbClients}>
            <table className="tss-table tss-table-stack">
              <thead>
                <tr>
                  <Th>DB ID</Th>
                  <Th>{t.common.name}</Th>
                  <Th>UID</Th>
                  <Th>{t.governance.lastIp}</Th>
                  <Th numeric>{t.governance.totalLogins}</Th>
                  {canAdmin ? <Th numeric>{t.common.actions}</Th> : null}
                </tr>
              </thead>
              <tbody>
                {dbClients.map((c) => {
                  const uidId = `db-client-uid-${c.cldbid}`;
                  return (
                    <tr key={c.cldbid}>
                      <Td label="DB ID">
                        <span className="font-mono text-xs">#{c.cldbid}</span>
                      </Td>
                      <Td label={t.common.name}>
                        <span className="font-medium text-text">{c.nickname}</span>
                      </Td>
                      <Td label="UID">
                        <div className="inline-flex items-center gap-1.5">
                          <code id={uidId} className="font-mono text-xs text-text-muted">
                            {c.uid}
                          </code>
                          <CopyButton targetId={uidId} />
                        </div>
                      </Td>
                      <Td label={t.governance.lastIp}>
                        <span className="font-mono text-xs">{c.last_ip}</span>
                      </Td>
                      <Td numeric label={t.governance.totalLogins}>
                        {c.total_connections}
                      </Td>
                      {canAdmin ? (
                        <Td numeric label={t.common.actions}>
                          <Button
                            size="xs"
                            variant="danger-link"
                            onClick={async () => {
                              await apiRequest(
                                `/instances/${activeInstanceId}/servers/${activeSid}/db-clients/${c.cldbid}`,
                                { method: 'DELETE' },
                              );
                              await loadTab();
                            }}
                          >
                            {t.common.delete}
                          </Button>
                        </Td>
                      ) : null}
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </TableContainer>
        </div>
      ) : tab === 'complaints' ? (
        <div role="tabpanel" aria-labelledby="gov-tab-complaints">
          {complaints.length === 0 ? (
            <EmptyState
              icon={ShieldAlert}
              title={t.governance.tabComplaints}
              description={t.common.noData}
            />
          ) : (
            <TableContainer label={t.governance.tabComplaints}>
              <table className="tss-table tss-table-stack">
                <thead>
                  <tr>
                    <Th>{t.governance.targetUser}</Th>
                    <Th>{t.governance.reporter}</Th>
                    <Th>{t.common.message}</Th>
                    {canModerate ? <Th numeric>{t.common.actions}</Th> : null}
                  </tr>
                </thead>
                <tbody>
                  {complaints.map((cp) => (
                    <tr key={`${cp.tcldbid}-${cp.fcldbid}`}>
                      <Td label={t.governance.targetUser}>{cp.tname}</Td>
                      <Td label={t.governance.reporter}>{cp.fname}</Td>
                      <Td label={t.common.message}>{cp.message}</Td>
                      {canModerate ? (
                        <Td numeric label={t.common.actions}>
                          <Button
                            size="xs"
                            variant="danger-link"
                            onClick={async () => {
                              await apiRequest(
                                `/instances/${activeInstanceId}/servers/${activeSid}/complaints/${cp.tcldbid}/${cp.fcldbid}`,
                                { method: 'DELETE' },
                              );
                              await loadTab();
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
        </div>
      ) : tab === 'ts_logs' ? (
        <div role="tabpanel" aria-labelledby="gov-tab-ts_logs" className="space-y-4">
          <div className="tss-well p-1 inline-flex items-center gap-0.5">
            {(['ALL', 'INFO', 'WARN', 'ERR'] as const).map((lvl) => (
              <button
                key={lvl}
                type="button"
                aria-pressed={logLevelFilter === lvl}
                onClick={() => setLogLevelFilter(lvl)}
                className={`min-h-7 px-3 py-1 text-xs rounded-[var(--radius-sm)] cursor-pointer ${
                  logLevelFilter === lvl
                    ? 'bg-surface-raised text-text font-semibold border border-border-strong/80 shadow-[var(--shadow-sm)]'
                    : 'text-text-secondary hover:text-text font-medium border border-transparent'
                }`}
              >
                {lvl === 'ALL' ? t.governance.filterAllLevels : lvl}
              </button>
            ))}
          </div>

          <TableContainer label={t.governance.tabTsLogs}>
            <table className="tss-table tss-table-stack">
              <thead>
                <tr>
                  <Th>{t.common.time}</Th>
                  <Th>{t.governance.level}</Th>
                  <Th>{t.governance.channel}</Th>
                  <Th>{t.common.message}</Th>
                </tr>
              </thead>
              <tbody>
                {filteredTsLogs.map((l, idx) => (
                  <tr key={idx}>
                    <Td label={t.common.time}>
                      <span className="font-mono text-xs text-text-muted whitespace-nowrap">
                        {l.timestamp}
                      </span>
                    </Td>
                    <Td label={t.governance.level}>
                      <StatusBadge
                        variant="dot"
                        tone={
                          l.level.includes('ERR')
                            ? 'danger'
                            : l.level.includes('WARN')
                              ? 'warning'
                              : 'neutral'
                        }
                        icon="none"
                        label={l.level}
                      />
                    </Td>
                    <Td label={t.governance.channel}>{l.channel}</Td>
                    <Td label={t.common.message}>
                      <span className="font-mono text-xs">{l.message}</span>
                    </Td>
                  </tr>
                ))}
              </tbody>
            </table>
          </TableContainer>
        </div>
      ) : (
        <div role="tabpanel" aria-labelledby="gov-tab-audit">
          <TableContainer label={t.governance.tabAuditLogs}>
            <table className="tss-table tss-table-stack">
              <thead>
                <tr>
                  <Th>{t.common.time}</Th>
                  <Th>{t.governance.actor}</Th>
                  <Th>{t.governance.action}</Th>
                  <Th>{t.governance.targetObj}</Th>
                  <Th>IP</Th>
                </tr>
              </thead>
              <tbody>
                {auditLogs.map((a) => {
                  const badge = formatAuditAction(a.action, locale);
                  return (
                    <tr key={a.id}>
                      <Td label={t.common.time}>
                        <span className="font-mono text-xs text-text-muted tabular-nums whitespace-nowrap">
                          {formatCompactTime(a.created_at)}
                        </span>
                      </Td>
                      <Td label={t.governance.actor}>
                        <span className="font-semibold text-text whitespace-nowrap">
                          {a.actor_username}
                        </span>
                      </Td>
                      <Td label={t.governance.action}>
                        <div className="inline-flex items-center gap-2">
                          <StatusBadge tone={badge.tone} icon="none" label={badge.label} />
                          <span className="font-mono text-[11px] text-text-muted">{a.action}</span>
                        </div>
                      </Td>
                      <Td label={t.governance.targetObj}>
                        <span className="font-mono text-xs">{a.target}</span>
                      </Td>
                      <Td label="IP">
                        <span className="font-mono text-xs text-text-muted">{a.ip_addr}</span>
                      </Td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </TableContainer>
        </div>
      )}

      {/* Add Ban Modal */}
      <ModalDialog
        open={showAddBan}
        onClose={() => setShowAddBan(false)}
        title={t.governance.addBan}
      >
        <form onSubmit={handleCreateBan}>
          <FormField label="IP" htmlFor="ban-ip">
            <Input
              id="ban-ip"
              value={banIp}
              onChange={(e) => setBanIp(e.target.value)}
              placeholder="203.0.113.10"
            />
          </FormField>
          <FormField label={t.governance.banNameRegex} htmlFor="ban-name">
            <Input
              id="ban-name"
              value={banName}
              onChange={(e) => setBanName(e.target.value)}
            />
          </FormField>
          <FormField label="UID" htmlFor="ban-uid">
            <Input
              id="ban-uid"
              value={banUid}
              onChange={(e) => setBanUid(e.target.value)}
            />
          </FormField>
          <FormField label={t.governance.banDuration} htmlFor="ban-dur">
            <Input
              id="ban-dur"
              type="number"
              min={0}
              value={banDuration}
              onChange={(e) => setBanDuration(e.target.value)}
            />
          </FormField>
          <FormField label={t.governance.banReason} htmlFor="ban-reason">
            <Input
              id="ban-reason"
              value={banReason}
              onChange={(e) => setBanReason(e.target.value)}
            />
          </FormField>
          <div className="flex justify-end gap-2">
            <Button variant="tertiary" onClick={() => setShowAddBan(false)}>
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
