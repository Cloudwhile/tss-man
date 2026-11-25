import React, { useCallback, useEffect, useState } from 'react';
import {
  Play,
  Plus,
  RefreshCw,
  Server,
  Square,
  Trash2,
} from 'lucide-react';
import { useApp } from '../context/AppContext';
import { useI18n } from '../i18n';
import {
  apiRequest,
  InstanceView,
  ServerInfoSnapshot,
  VirtualServerSummary,
} from '../lib/api';
import {
  formatInstanceStatus,
  formatProtocolBadge,
  formatRoleLabel,
} from '../lib/format';
import { AlertBanner } from '../components/ui/AlertBanner';
import { Button } from '../components/ui/Button';
import { TableContainer, Td, Th } from '../components/ui/DataTable';
import { EmptyState } from '../components/ui/EmptyState';
import {
  CheckboxField,
  ChoiceCardGroup,
  FormField,
  Input,
  Textarea,
  useFieldId,
} from '../components/ui/FormControls';
import { ModalDialog } from '../components/ui/ModalDialog';
import { PopoverMenu } from '../components/ui/PopoverMenu';
import { SlotMeter } from '../components/ui/SlotMeter';
import { StatusBadge } from '../components/ui/StatusBadge';

export function InstancesPage() {
  const { t, locale } = useI18n();
  const { user, instances, activeInstanceId, setActiveInstanceId, refreshInstances } = useApp();

  const [vservers, setVservers] = useState<VirtualServerSummary[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  // Add Instance Modal
  const [showAddModal, setShowAddModal] = useState(false);
  const [name, setName] = useState('');
  const [protocol, setProtocol] = useState<'raw_tcp' | 'ssh' | 'web_query'>('raw_tcp');
  const [host, setHost] = useState('');
  const [port, setPort] = useState('10011');
  const [username, setUsername] = useState('serveradmin');
  const [secret, setSecret] = useState('');
  const [defaultSid, setDefaultSid] = useState('1');
  const [tlsVerify, setTlsVerify] = useState(true);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<string | null>(null);

  // Edit Virtual Server Modal
  const [editVsId, setEditVsId] = useState<number | null>(null);
  const [vsName, setVsName] = useState('');
  const [vsWelcome, setVsWelcome] = useState('');
  const [vsMaxClients, setVsMaxClients] = useState('32');

  // Create Virtual Server Modal
  const [showCreateVs, setShowCreateVs] = useState(false);
  const [newVsName, setNewVsName] = useState('');
  const [newVsSlots, setNewVsSlots] = useState('32');

  // Delete Instance Confirm Modal
  const [deleteTarget, setDeleteTarget] = useState<InstanceView | null>(null);

  const fName = useFieldId('inst-name');
  const fHost = useFieldId('inst-host');
  const fPort = useFieldId('inst-port');
  const fUser = useFieldId('inst-user');
  const fSecret = useFieldId('inst-secret');
  const fSid = useFieldId('inst-sid');

  const isSuper = user?.global_role === 'super_admin';
  const activeInst = instances.find((i) => i.id === activeInstanceId);
  const canManageVs =
    activeInst?.my_role === 'super_admin' || activeInst?.my_role === 'server_admin';

  const loadVirtualServers = useCallback(async () => {
    if (!activeInstanceId) {
      setVservers([]);
      return;
    }
    try {
      const list = await apiRequest<VirtualServerSummary[]>(
        `/instances/${activeInstanceId}/servers`,
      );
      setVservers(list);
    } catch (err) {
      setVservers([]);
      setError(err instanceof Error ? err.message : 'Failed to list virtual servers');
    }
  }, [activeInstanceId]);

  useEffect(() => {
    loadVirtualServers();
  }, [loadVirtualServers]);

  const handleTestConnection = async () => {
    setTesting(true);
    setTestResult(null);
    setError(null);
    try {
      await apiRequest('/instances/test-connection', {
        method: 'POST',
        body: JSON.stringify({
          protocol,
          host: host.trim(),
          port: Number(port),
          username: username.trim(),
          secret,
          virtual_server_id: Number(defaultSid) || 1,
          tls_verify: tlsVerify,
        }),
      });
      setTestResult(t.instances.testSuccess);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Connection failed');
    } finally {
      setTesting(false);
    }
  };

  const handleCreateInstance = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    try {
      const created = await apiRequest<InstanceView>('/instances', {
        method: 'POST',
        body: JSON.stringify({
          name: name.trim(),
          protocol,
          host: host.trim(),
          port: Number(port),
          username: username.trim(),
          secret,
          default_virtual_server_id: Number(defaultSid) || 1,
          tls_verify: tlsVerify,
          enabled: true,
        }),
      });
      setShowAddModal(false);
      setName('');
      setHost('');
      setSecret('');
      await refreshInstances();
      setActiveInstanceId(created.id);
      setNotice(`${created.name} connected`);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Create instance failed');
    }
  };

  const openEditVs = async (sid: number) => {
    if (!activeInstanceId) return;
    try {
      const info = await apiRequest<ServerInfoSnapshot>(
        `/instances/${activeInstanceId}/servers/${sid}/info`,
      );
      setEditVsId(sid);
      setVsName(info.name);
      setVsWelcome(info.welcome_message);
      setVsMaxClients(String(info.max_clients));
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load virtual server info');
    }
  };

  const handleSaveVs = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!activeInstanceId || editVsId === null) return;
    try {
      await apiRequest(`/instances/${activeInstanceId}/servers/${editVsId}/info`, {
        method: 'PUT',
        body: JSON.stringify({
          name: vsName.trim(),
          welcome_message: vsWelcome,
          max_clients: Number(vsMaxClients) || 32,
        }),
      });
      setEditVsId(null);
      await loadVirtualServers();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to save virtual server');
    }
  };

  const handleCreateVs = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!activeInstanceId) return;
    try {
      await apiRequest(`/instances/${activeInstanceId}/servers`, {
        method: 'POST',
        body: JSON.stringify({
          name: newVsName.trim(),
          max_clients: Number(newVsSlots) || 32,
        }),
      });
      setShowCreateVs(false);
      setNewVsName('');
      await loadVirtualServers();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Create virtual server failed');
    }
  };

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-xl sm:text-2xl font-semibold text-text tracking-tight">{t.instances.title}</h1>
          <p className="mt-0.5 text-sm text-text-secondary tss-prose">{t.instances.subtitle}</p>
        </div>
        {isSuper && instances.length > 0 ? (
          <Button variant="primary" onClick={() => setShowAddModal(true)}>
            <Plus className="w-4 h-4" aria-hidden="true" />
            <span>{t.instances.addInstance}</span>
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

      {instances.length === 0 ? (
        <EmptyState
          icon={Server}
          title={t.instances.emptyTitle}
          description={t.instances.emptyDesc}
          action={
            isSuper ? (
              <Button variant="primary" onClick={() => setShowAddModal(true)}>
                <Plus className="w-4 h-4" aria-hidden="true" />
                <span>{t.instances.addInstance}</span>
              </Button>
            ) : undefined
          }
        />
      ) : (
        <>
          <TableContainer label={t.instances.title}>
            <table className="tss-table tss-table-stack">
              <thead>
                <tr>
                  <Th>{t.common.name}</Th>
                  <Th>{t.instances.host}</Th>
                  <Th>{t.common.status}</Th>
                  <Th>{t.common.role}</Th>
                  <Th numeric>{t.common.actions}</Th>
                </tr>
              </thead>
              <tbody>
                {instances.map((inst) => {
                  const isActive = inst.id === activeInstanceId;
                  const roleMeta = formatRoleLabel(inst.my_role, locale);
                  return (
                  <tr
                    key={inst.id}
                    className={isActive ? 'bg-action-subtle/30' : ''}
                  >
                    <Td
                      label={t.common.name}
                      className={isActive ? 'shadow-[inset_3px_0_0_var(--color-action)]' : ''}
                    >
                      <div className="flex items-center gap-2 min-w-0">
                        <button
                          type="button"
                          onClick={() => setActiveInstanceId(inst.id)}
                          className="font-semibold text-text hover:text-action-text text-left cursor-pointer"
                        >
                          {inst.name}
                        </button>
                      </div>
                    </Td>
                    <Td label={t.instances.host}>
                      <div className="flex items-center gap-2">
                        <span className="font-mono text-[10.5px] font-medium uppercase px-1.5 py-0.5 rounded-[var(--radius-sm)] bg-surface-sunken border border-border text-text-secondary leading-none tss-trim">
                          {formatProtocolBadge(inst.protocol)}
                        </span>
                        <span className="font-mono text-xs text-text-muted tabular-nums">
                          {inst.host}:{inst.port}
                        </span>
                      </div>
                    </Td>
                    <Td label={t.common.status}>
                      <StatusBadge
                        variant="dot"
                        tone={
                          inst.status === 'online'
                            ? 'success'
                            : inst.status === 'connecting'
                              ? 'warning'
                              : 'danger'
                        }
                        label={formatInstanceStatus(inst.status, t)}
                      />
                    </Td>
                    <Td label={t.common.role}>
                      <StatusBadge tone={roleMeta.tone} icon="none" label={roleMeta.label} />
                    </Td>
                    <Td numeric label={t.common.actions}>
                      <div className="inline-flex items-center gap-1.5 justify-end">
                        <Button
                          size="xs"
                          variant="tertiary"
                          onClick={async () => {
                            await apiRequest(`/instances/${inst.id}/reconnect`, {
                              method: 'POST',
                            });
                            await refreshInstances();
                            await loadVirtualServers();
                          }}
                        >
                          <RefreshCw className="w-3.5 h-3.5" aria-hidden="true" />
                          <span>{t.instances.reconnect}</span>
                        </Button>
                        {isSuper ? (
                          <PopoverMenu
                            iconOnly
                            triggerLabel={t.common.moreActions}
                            items={[
                              {
                                label: t.common.delete,
                                danger: true,
                                icon: <Trash2 className="w-3.5 h-3.5" aria-hidden="true" />,
                                onClick: () => setDeleteTarget(inst),
                              },
                            ]}
                          />
                        ) : null}
                      </div>
                    </Td>
                  </tr>
                  );
                })}
              </tbody>
            </table>
          </TableContainer>

          {/* Virtual Servers Section */}
          <section className="space-y-3.5">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div className="flex items-center gap-2">
                <h2 className="text-base font-semibold text-text">
                  {t.instances.virtualServers}
                </h2>
                {activeInst ? (
                  <span className="font-mono text-xs px-2 py-0.5 rounded-[var(--radius-sm)] bg-surface-sunken border border-border text-text-secondary">
                    {activeInst.name}
                  </span>
                ) : null}
              </div>
              {isSuper ? (
                <Button variant="secondary" size="sm" onClick={() => setShowCreateVs(true)}>
                  <Plus className="w-3.5 h-3.5" aria-hidden="true" />
                  <span>{t.instances.createVServer}</span>
                </Button>
              ) : null}
            </div>

            <TableContainer label={t.instances.virtualServers}>
              <table className="tss-table tss-table-stack">
                <thead>
                  <tr>
                    <Th>SID</Th>
                    <Th>{t.common.name}</Th>
                    <Th numeric>{t.instances.port}</Th>
                    <Th>{t.dashboard.kpiOnline}</Th>
                    <Th>{t.common.status}</Th>
                    <Th numeric>{t.common.actions}</Th>
                  </tr>
                </thead>
                <tbody>
                  {vservers.map((vs) => (
                    <tr key={vs.id} className="hover:bg-surface/60">
                      <Td label="SID">
                        <span className="font-mono text-xs tabular-nums">#{vs.id}</span>
                      </Td>
                      <Td label={t.common.name}>
                        <span className="font-medium text-text">{vs.name}</span>
                      </Td>
                      <Td numeric label={t.instances.port}>
                        <span className="font-mono text-xs">{vs.port}</span>
                      </Td>
                      <Td label={t.dashboard.kpiOnline}>
                        <div className="w-36 max-w-full ms-auto sm:ms-0">
                          <SlotMeter
                            used={vs.clients_online}
                            max={vs.max_clients}
                            label={`${vs.name} slots`}
                            showCounts
                          />
                        </div>
                      </Td>
                      <Td label={t.common.status}>
                        <StatusBadge
                          variant="dot"
                          tone={vs.status === 'online' ? 'success' : 'neutral'}
                          label={vs.status === 'online' ? t.common.online : t.common.offline}
                        />
                      </Td>
                      <Td numeric label={t.common.actions}>
                        <div className="inline-flex items-center gap-1.5 justify-end">
                          {canManageVs ? (
                            <>
                              {vs.status !== 'online' ? (
                                <Button
                                  size="xs"
                                  variant="secondary"
                                  onClick={async () => {
                                    await apiRequest(
                                      `/instances/${activeInstanceId}/servers/${vs.id}/start`,
                                      { method: 'POST' },
                                    );
                                    await loadVirtualServers();
                                  }}
                                >
                                  <Play className="w-3.5 h-3.5" aria-hidden="true" />
                                  <span>{t.instances.startVServer}</span>
                                </Button>
                              ) : (
                                <Button
                                  size="xs"
                                  variant="tertiary"
                                  onClick={async () => {
                                    await apiRequest(
                                      `/instances/${activeInstanceId}/servers/${vs.id}/stop`,
                                      { method: 'POST' },
                                    );
                                    await loadVirtualServers();
                                  }}
                                >
                                  <Square className="w-3.5 h-3.5" aria-hidden="true" />
                                  <span>{t.instances.stopVServer}</span>
                                </Button>
                              )}
                              <Button
                                size="xs"
                                variant="secondary"
                                onClick={() => openEditVs(vs.id)}
                              >
                                {t.instances.editVServer}
                              </Button>
                            </>
                          ) : null}
                        </div>
                      </Td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </TableContainer>
          </section>
        </>
      )}

      {/* Add Instance Modal with ChoiceCardGroup */}
      <ModalDialog
        open={showAddModal}
        onClose={() => setShowAddModal(false)}
        title={t.instances.addInstance}
        maxWidthClass="max-w-xl"
      >
        <form onSubmit={handleCreateInstance}>
          {testResult ? (
            <AlertBanner tone="success" className="mb-4" onDismiss={() => setTestResult(null)}>
              {testResult}
            </AlertBanner>
          ) : null}

          <FormField label={t.common.name} htmlFor={fName} required className="max-w-none">
            <Input
              id={fName}
              required
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="EU Frankfurt TS6"
            />
          </FormField>

          <ChoiceCardGroup
            name="inst-protocol"
            label={t.instances.protocol}
            value={protocol}
            onChange={(p) => {
              setProtocol(p);
              setPort(p === 'ssh' ? '10022' : p === 'web_query' ? '10080' : '10011');
            }}
            options={[
              {
                value: 'raw_tcp',
                title: 'Raw TCP',
                tag: '10011',
                description: t.instances.protoRawDesc,
              },
              {
                value: 'ssh',
                title: 'SSH Query',
                tag: '10022',
                description: t.instances.protoSshDesc,
              },
              {
                value: 'web_query',
                title: 'WebQuery',
                tag: 'HTTP(S)',
                description: t.instances.protoWebDesc,
              },
            ]}
          />

          <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
            <FormField label={t.instances.host} htmlFor={fHost} required className="sm:col-span-2 max-w-none">
              <Input
                id={fHost}
                required
                value={host}
                onChange={(e) => setHost(e.target.value)}
                placeholder="127.0.0.1"
              />
            </FormField>
            <FormField label={t.instances.port} htmlFor={fPort} required className="max-w-none">
              <Input
                id={fPort}
                type="number"
                required
                value={port}
                onChange={(e) => setPort(e.target.value)}
              />
            </FormField>
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <FormField label={t.instances.queryUser} htmlFor={fUser} className="max-w-none">
              <Input
                id={fUser}
                value={username}
                onChange={(e) => setUsername(e.target.value)}
              />
            </FormField>

            <FormField label={t.instances.defaultSid} htmlFor={fSid} className="max-w-none">
              <Input
                id={fSid}
                type="number"
                min={1}
                value={defaultSid}
                onChange={(e) => setDefaultSid(e.target.value)}
              />
            </FormField>
          </div>

          <FormField label={t.instances.querySecret} htmlFor={fSecret} required className="max-w-none">
            <Input
              id={fSecret}
              type="password"
              required
              value={secret}
              onChange={(e) => setSecret(e.target.value)}
            />
          </FormField>

          {protocol === 'web_query' ? (
            <div className="mb-5">
              <CheckboxField
                label={t.instances.tlsVerify}
                checked={tlsVerify}
                onChange={setTlsVerify}
              />
            </div>
          ) : null}

          <div className="flex flex-wrap justify-between gap-3 pt-2 border-t border-border">
            <Button variant="secondary" onClick={handleTestConnection} disabled={testing}>
              {testing ? t.common.loading : t.instances.testConn}
            </Button>
            <div className="flex gap-2">
              <Button variant="tertiary" onClick={() => setShowAddModal(false)}>
                {t.common.cancel}
              </Button>
              <Button type="submit" variant="primary">
                {t.common.save}
              </Button>
            </div>
          </div>
        </form>
      </ModalDialog>

      {/* Edit Virtual Server Modal */}
      <ModalDialog
        open={editVsId !== null}
        onClose={() => setEditVsId(null)}
        title={`${t.instances.editVServer} #${editVsId ?? ''}`}
      >
        <form onSubmit={handleSaveVs}>
          <FormField label={t.common.name} htmlFor="vs-edit-name" required>
            <Input
              id="vs-edit-name"
              required
              value={vsName}
              onChange={(e) => setVsName(e.target.value)}
            />
          </FormField>
          <FormField label={t.workbench.slots} htmlFor="vs-edit-slots" required>
            <Input
              id="vs-edit-slots"
              type="number"
              min={1}
              required
              value={vsMaxClients}
              onChange={(e) => setVsMaxClients(e.target.value)}
            />
          </FormField>
          <FormField label={t.instances.welcomeMsg} htmlFor="vs-edit-welcome">
            <Textarea
              id="vs-edit-welcome"
              value={vsWelcome}
              onChange={(e) => setVsWelcome(e.target.value)}
            />
          </FormField>
          <div className="flex justify-end gap-2">
            <Button variant="tertiary" onClick={() => setEditVsId(null)}>
              {t.common.cancel}
            </Button>
            <Button type="submit" variant="primary">
              {t.common.save}
            </Button>
          </div>
        </form>
      </ModalDialog>

      {/* Create Virtual Server Modal */}
      <ModalDialog
        open={showCreateVs}
        onClose={() => setShowCreateVs(false)}
        title={t.instances.createVServer}
      >
        <form onSubmit={handleCreateVs}>
          <FormField label={t.common.name} htmlFor="vs-new-name" required>
            <Input
              id="vs-new-name"
              required
              value={newVsName}
              onChange={(e) => setNewVsName(e.target.value)}
            />
          </FormField>
          <FormField label={t.workbench.slots} htmlFor="vs-new-slots" required>
            <Input
              id="vs-new-slots"
              type="number"
              min={1}
              required
              value={newVsSlots}
              onChange={(e) => setNewVsSlots(e.target.value)}
            />
          </FormField>
          <div className="flex justify-end gap-2">
            <Button variant="tertiary" onClick={() => setShowCreateVs(false)}>
              {t.common.cancel}
            </Button>
            <Button type="submit" variant="primary">
              {t.common.create}
            </Button>
          </div>
        </form>
      </ModalDialog>

      {/* Destructive Delete Instance Modal */}
      <ModalDialog
        open={deleteTarget !== null}
        onClose={() => setDeleteTarget(null)}
        title={`${t.common.delete}: ${deleteTarget?.name ?? ''}`}
      >
        <p className="text-sm text-text-secondary mb-6 leading-normal tss-prose">
          {t.instances.deleteWarn}
        </p>
        <div className="flex justify-end gap-3">
          <Button variant="tertiary" autoFocus onClick={() => setDeleteTarget(null)}>
            {t.common.cancel}
          </Button>
          <Button
            variant="danger-confirm"
            onClick={async () => {
              if (!deleteTarget) return;
              await apiRequest(`/instances/${deleteTarget.id}`, { method: 'DELETE' });
              setDeleteTarget(null);
              await refreshInstances();
            }}
          >
            {t.common.delete}
          </Button>
        </div>
      </ModalDialog>
    </div>
  );
}
