import { useCallback, useEffect, useState } from 'react';
import { KeyRound, Plus, Shield, Trash2 } from 'lucide-react';
import { useApp } from '../context/AppContext';
import { useI18n } from '../i18n';
import {
  apiRequest,
  GroupMember,
  PermissionEntry,
  PrivilegeKey,
  ServerGroup,
} from '../lib/api';
import { AlertBanner } from '../components/ui/AlertBanner';
import { Button } from '../components/ui/Button';
import { CopyButton } from '../components/ui/CopyButton';
import { TableContainer, Td, Th } from '../components/ui/DataTable';
import { EmptyState } from '../components/ui/EmptyState';
import { CheckboxField, FormField, Input, Select } from '../components/ui/FormControls';
import { ModalDialog } from '../components/ui/ModalDialog';
import { TabBar } from '../components/ui/Tabs';

export function PermissionsPage() {
  const { t } = useI18n();
  const { instances, activeInstanceId, activeSid } = useApp();

  const [tab, setTab] = useState<'groups' | 'channel_groups' | 'tokens'>('groups');
  const [groups, setGroups] = useState<ServerGroup[]>([]);
  const [selectedSgid, setSelectedSgid] = useState<number | null>(null);
  const [perms, setPerms] = useState<PermissionEntry[]>([]);
  const [members, setMembers] = useState<GroupMember[]>([]);
  const [channelGroups, setChannelGroups] = useState<
    { cgid: number; name: string; group_type: number }[]
  >([]);
  const [tokens, setTokens] = useState<PrivilegeKey[]>([]);
  const [permFilter, setPermFilter] = useState('');
  const [error, setError] = useState<string | null>(null);

  // Modals
  const [showNewGroup, setShowNewGroup] = useState(false);
  const [newGroupName, setNewGroupName] = useState('');

  const [showPermModal, setShowPermModal] = useState(false);
  const [permName, setPermName] = useState('');
  const [permVal, setPermVal] = useState('75');
  const [permSkip, setPermSkip] = useState(false);
  const [permNegated, setPermNegated] = useState(false);

  const [newMemberId, setNewMemberId] = useState('');

  const [showNewToken, setShowNewToken] = useState(false);
  const [tokGroupId, setTokGroupId] = useState('');
  const [tokCount, setTokCount] = useState('1');
  const [tokDesc, setTokDesc] = useState('');

  const activeInst = instances.find((i) => i.id === activeInstanceId);
  const canAdmin =
    activeInst?.my_role === 'super_admin' || activeInst?.my_role === 'server_admin';

  const loadBase = useCallback(async () => {
    if (!activeInstanceId) return;
    setError(null);
    try {
      const [gList, cgList, tkList] = await Promise.all([
        apiRequest<ServerGroup[]>(
          `/instances/${activeInstanceId}/servers/${activeSid}/server-groups`,
        ),
        apiRequest<{ cgid: number; name: string; group_type: number }[]>(
          `/instances/${activeInstanceId}/servers/${activeSid}/channel-groups`,
        ),
        apiRequest<PrivilegeKey[]>(`/instances/${activeInstanceId}/servers/${activeSid}/tokens`),
      ]);
      setGroups(gList);
      setChannelGroups(cgList);
      setTokens(tkList);
      setSelectedSgid((prev) =>
        prev && gList.some((g) => g.sgid === prev) ? prev : gList[0]?.sgid ?? null,
      );
      if (!tokGroupId && gList[0]) setTokGroupId(String(gList[0].sgid));
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load groups');
    }
  }, [activeInstanceId, activeSid, tokGroupId]);

  useEffect(() => {
    loadBase();
  }, [loadBase]);

  const loadSelectedGroup = useCallback(async () => {
    if (!activeInstanceId || !selectedSgid) {
      setPerms([]);
      setMembers([]);
      return;
    }
    try {
      const [p, m] = await Promise.all([
        apiRequest<PermissionEntry[]>(
          `/instances/${activeInstanceId}/servers/${activeSid}/server-groups/${selectedSgid}/permissions`,
        ),
        apiRequest<GroupMember[]>(
          `/instances/${activeInstanceId}/servers/${activeSid}/server-groups/${selectedSgid}/members`,
        ),
      ]);
      setPerms(p);
      setMembers(m);
    } catch {}
  }, [activeInstanceId, activeSid, selectedSgid]);

  useEffect(() => {
    loadSelectedGroup();
  }, [loadSelectedGroup]);

  if (!activeInstanceId) {
    return (
      <EmptyState
        icon={Shield}
        title={t.workbench.emptyTitle}
        description={t.workbench.emptyDesc}
      />
    );
  }

  const filteredPerms = perms.filter((p) =>
    p.permname.toLowerCase().includes(permFilter.trim().toLowerCase()),
  );

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-xl sm:text-2xl font-semibold text-text tracking-tight">{t.permissions.title}</h1>
          <p className="mt-0.5 text-sm text-text-secondary tss-prose">{t.permissions.subtitle}</p>
        </div>
        {canAdmin && tab === 'groups' ? (
          <Button variant="primary" onClick={() => setShowNewGroup(true)}>
            <Plus className="w-4 h-4" aria-hidden="true" />
            <span>{t.permissions.newGroup}</span>
          </Button>
        ) : canAdmin && tab === 'tokens' && tokens.length > 0 ? (
          <Button variant="primary" onClick={() => setShowNewToken(true)}>
            <Plus className="w-4 h-4" aria-hidden="true" />
            <span>{t.permissions.newToken}</span>
          </Button>
        ) : null}
      </div>

      {error ? (
        <AlertBanner tone="danger" onDismiss={() => setError(null)}>
          {error}
        </AlertBanner>
      ) : null}

      <TabBar
        idPrefix="perm"
        ariaLabel={t.permissions.title}
        active={tab}
        onChange={setTab}
        items={[
          { id: 'groups', label: t.permissions.tabServerGroups, count: groups.length },
          {
            id: 'channel_groups',
            label: t.permissions.tabChannelGroups,
            count: channelGroups.length,
          },
          { id: 'tokens', label: t.permissions.tabTokens, count: tokens.length },
        ]}
      />

      {tab === 'groups' ? (
        /* Unified Split-Pane Permission Matrix */
        <div
          role="tabpanel"
          aria-labelledby="perm-tab-groups"
          className="tss-frame overflow-hidden grid grid-cols-1 lg:grid-cols-12 divide-y lg:divide-y-0 lg:divide-x divide-border"
        >
          {/* Left Group Directory & Members */}
          <aside className="lg:col-span-4 flex flex-col">
            <div className="p-3 space-y-0.5">
              {groups.map((g) => {
                const isSelected = selectedSgid === g.sgid;
                return (
                  <div
                    key={g.sgid}
                    className={`flex items-center justify-between gap-2 px-3 py-2 rounded-[var(--radius-DEFAULT)] border transition-colors ${
                      isSelected
                        ? 'bg-action-subtle border-action/20 text-action-text font-semibold'
                        : 'border-transparent hover:bg-surface-sunken/60 text-text'
                    }`}
                  >
                    <button
                      type="button"
                      onClick={() => setSelectedSgid(g.sgid)}
                      className="flex-1 text-left text-sm truncate cursor-pointer flex items-baseline gap-2"
                    >
                      <span className="font-mono text-xs text-text-muted shrink-0">#{g.sgid}</span>
                      <span className="truncate">{g.name}</span>
                    </button>
                    {canAdmin && g.sgid > 10 ? (
                      <button
                        type="button"
                        aria-label={`Delete ${g.name}`}
                        onClick={async () => {
                          await apiRequest(
                            `/instances/${activeInstanceId}/servers/${activeSid}/server-groups/${g.sgid}?force=true`,
                            { method: 'DELETE' },
                          );
                          await loadBase();
                        }}
                        className="text-text-muted hover:text-danger cursor-pointer p-1"
                      >
                        <Trash2 className="w-3.5 h-3.5" aria-hidden="true" />
                      </button>
                    ) : null}
                  </div>
                );
              })}
            </div>

            <div className="border-t border-border p-4 space-y-3 bg-surface/40 flex-1">
              <div className="flex items-center justify-between">
                <h2 className="text-xs font-semibold uppercase tracking-wide text-text-muted">
                  {t.permissions.members}
                </h2>
                <span className="font-mono text-xs text-text-muted tabular-nums">
                  {members.length}
                </span>
              </div>

              {canAdmin && selectedSgid ? (
                <form
                  className="flex items-center gap-2"
                  onSubmit={async (e) => {
                    e.preventDefault();
                    const cldbid = Number(newMemberId);
                    if (!cldbid) return;
                    await apiRequest(
                      `/instances/${activeInstanceId}/servers/${activeSid}/server-groups/${selectedSgid}/members`,
                      { method: 'POST', body: JSON.stringify({ cldbid }) },
                    );
                    setNewMemberId('');
                    await loadSelectedGroup();
                  }}
                >
                  <Input
                    type="number"
                    min={1}
                    placeholder={t.permissions.addMemberId}
                    value={newMemberId}
                    onChange={(e) => setNewMemberId(e.target.value)}
                  />
                  <Button type="submit" size="md" variant="secondary" aria-label={t.common.create}>
                    <Plus className="w-4 h-4" aria-hidden="true" />
                  </Button>
                </form>
              ) : null}

              {members.length === 0 ? (
                <p className="text-xs text-text-muted py-2">{t.common.noData}</p>
              ) : (
                <ul className="divide-y divide-border/50">
                  {members.map((m) => (
                    <li
                      key={m.cldbid}
                      className="py-2 flex items-center justify-between gap-2 text-sm"
                    >
                      <div className="min-w-0">
                        <div className="font-medium text-text truncate">{m.nickname}</div>
                        <div className="font-mono text-xs text-text-muted">DB#{m.cldbid}</div>
                      </div>
                      {canAdmin && selectedSgid ? (
                        <Button
                          size="xs"
                          variant="danger-link"
                          onClick={async () => {
                            await apiRequest(
                              `/instances/${activeInstanceId}/servers/${activeSid}/server-groups/${selectedSgid}/members/${m.cldbid}`,
                              { method: 'DELETE' },
                            );
                            await loadSelectedGroup();
                          }}
                        >
                          {t.common.delete}
                        </Button>
                      ) : null}
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </aside>

          {/* Right Permission Matrix */}
          <div className="lg:col-span-8 flex flex-col min-w-0">
            <div className="px-4 py-3 bg-surface/50 border-b border-border flex flex-wrap items-center justify-between gap-3">
              <search className="flex-1 min-w-52 max-w-md">
                <Input
                  type="search"
                  placeholder={t.permissions.filterPerm}
                  value={permFilter}
                  onChange={(e) => setPermFilter(e.target.value)}
                  className="min-h-9 py-1.5"
                />
              </search>
              {canAdmin && selectedSgid ? (
                <Button
                  size="sm"
                  variant="secondary"
                  onClick={() => {
                    setPermName('');
                    setPermVal('75');
                    setPermSkip(false);
                    setPermNegated(false);
                    setShowPermModal(true);
                  }}
                >
                  <Plus className="w-3.5 h-3.5" aria-hidden="true" />
                  <span>{t.permissions.addPerm}</span>
                </Button>
              ) : null}
            </div>

            <div className="tss-table-wrapper">
              <table className="tss-table tss-table-stack">
                <thead>
                  <tr>
                    <Th>{t.permissions.permName}</Th>
                    <Th numeric>{t.permissions.permValue}</Th>
                    <Th>{t.permissions.permFlags}</Th>
                    {canAdmin ? <Th numeric>{t.common.actions}</Th> : null}
                  </tr>
                </thead>
                <tbody>
                  {filteredPerms.map((p) => {
                    const isBool = p.permname.startsWith('b_');
                    return (
                      <tr key={p.permname} className="hover:bg-surface/60">
                        <Td label={t.permissions.permName}>
                          <span className="font-mono text-xs text-text">{p.permname}</span>
                        </Td>
                        <Td numeric label={t.permissions.permValue}>
                          {isBool ? (
                            <span
                              className={`inline-block font-mono text-xs px-2 py-0.5 rounded-[var(--radius-sm)] ${
                                p.permvalue > 0
                                  ? 'bg-success-subtle text-success-text'
                                  : 'bg-surface-sunken text-text-muted'
                              }`}
                            >
                              {p.permvalue > 0 ? t.common.yes : t.common.none}
                            </span>
                          ) : (
                            <span className="font-mono font-medium text-text tabular-nums">
                              {p.permvalue}
                            </span>
                          )}
                        </Td>
                        <Td label={t.permissions.permFlags}>
                          {p.permskip || p.permnegated ? (
                            <div className="inline-flex items-center gap-1">
                              {p.permskip ? (
                                <span className="font-mono text-[11px] px-1.5 py-0.5 rounded-[var(--radius-sm)] bg-action-subtle text-action-text">
                                  Skip
                                </span>
                              ) : null}
                              {p.permnegated ? (
                                <span className="font-mono text-[11px] px-1.5 py-0.5 rounded-[var(--radius-sm)] bg-warning-subtle text-warning-text">
                                  Neg
                                </span>
                              ) : null}
                            </div>
                          ) : (
                            <span className="text-text-muted text-xs">—</span>
                          )}
                        </Td>
                        {canAdmin && selectedSgid ? (
                          <Td numeric label={t.common.actions}>
                            <div className="inline-flex gap-1.5 justify-end">
                              <Button
                                size="xs"
                                variant="tertiary"
                                onClick={() => {
                                  setPermName(p.permname);
                                  setPermVal(String(p.permvalue));
                                  setPermSkip(p.permskip);
                                  setPermNegated(p.permnegated);
                                  setShowPermModal(true);
                                }}
                              >
                                {t.common.edit}
                              </Button>
                              <Button
                                size="xs"
                                variant="danger-link"
                                onClick={async () => {
                                  await apiRequest(
                                    `/instances/${activeInstanceId}/servers/${activeSid}/server-groups/${selectedSgid}/permissions/${encodeURIComponent(p.permname)}`,
                                    { method: 'DELETE' },
                                  );
                                  await loadSelectedGroup();
                                }}
                              >
                                {t.common.delete}
                              </Button>
                            </div>
                          </Td>
                        ) : null}
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </div>
        </div>
      ) : tab === 'channel_groups' ? (
        <div role="tabpanel" aria-labelledby="perm-tab-channel_groups">
          <TableContainer label={t.permissions.tabChannelGroups}>
            <table className="tss-table tss-table-stack">
              <thead>
                <tr>
                  <Th>CGID</Th>
                  <Th>{t.common.name}</Th>
                  <Th numeric>{t.common.type}</Th>
                </tr>
              </thead>
              <tbody>
                {channelGroups.map((cg) => (
                  <tr key={cg.cgid}>
                    <Td label="CGID">
                      <span className="font-mono text-xs">#{cg.cgid}</span>
                    </Td>
                    <Td label={t.common.name}>{cg.name}</Td>
                    <Td numeric label={t.common.type}>
                      <span className="font-mono text-xs">{cg.group_type}</span>
                    </Td>
                  </tr>
                ))}
              </tbody>
            </table>
          </TableContainer>
        </div>
      ) : tokens.length === 0 ? (
        <div role="tabpanel" aria-labelledby="perm-tab-tokens">
          <EmptyState
            icon={KeyRound}
            title={t.permissions.tabTokens}
            description={t.common.noData}
            action={
              canAdmin ? (
                <Button variant="primary" onClick={() => setShowNewToken(true)}>
                  <Plus className="w-4 h-4" aria-hidden="true" />
                  <span>{t.permissions.newToken}</span>
                </Button>
              ) : undefined
            }
          />
        </div>
      ) : (
        <div role="tabpanel" aria-labelledby="perm-tab-tokens">
          <TableContainer label={t.permissions.tabTokens}>
            <table className="tss-table tss-table-stack">
              <thead>
                <tr>
                  <Th>{t.permissions.tokenLabel}</Th>
                  <Th>{t.permissions.groupId}</Th>
                  <Th>{t.common.description}</Th>
                  <Th numeric>{t.common.actions}</Th>
                </tr>
              </thead>
              <tbody>
                {tokens.map((tk, idx) => {
                  const tokenElId = `priv-token-val-${idx}`;
                  return (
                    <tr key={tk.token}>
                      <Td label={t.permissions.tokenLabel}>
                        <code id={tokenElId} className="font-mono text-xs text-text break-all">
                          {tk.token}
                        </code>
                      </Td>
                      <Td label={t.permissions.groupId}>
                        <span className="font-mono text-xs">#{tk.token_id1}</span>
                      </Td>
                      <Td label={t.common.description}>{tk.description || '—'}</Td>
                      <Td numeric label={t.common.actions}>
                        <div className="inline-flex items-center gap-2 justify-end">
                          <CopyButton
                            targetId={tokenElId}
                            compact={false}
                            ariaLabel={`${t.permissions.copyToken} ${tk.token}`}
                          />
                          {canAdmin ? (
                            <Button
                              size="xs"
                              variant="danger-link"
                              onClick={async () => {
                                await apiRequest(
                                  `/instances/${activeInstanceId}/servers/${activeSid}/tokens/${encodeURIComponent(tk.token)}`,
                                  { method: 'DELETE' },
                                );
                                await loadBase();
                              }}
                            >
                              {t.common.delete}
                            </Button>
                          ) : null}
                        </div>
                      </Td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </TableContainer>
        </div>
      )}

      {/* New Group Modal */}
      <ModalDialog
        open={showNewGroup}
        onClose={() => setShowNewGroup(false)}
        title={t.permissions.newGroup}
      >
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            await apiRequest(
              `/instances/${activeInstanceId}/servers/${activeSid}/server-groups`,
              { method: 'POST', body: JSON.stringify({ name: newGroupName.trim(), group_type: 1 }) },
            );
            setShowNewGroup(false);
            setNewGroupName('');
            await loadBase();
          }}
        >
          <FormField label={t.common.name} htmlFor="sg-new-name" required>
            <Input
              id="sg-new-name"
              required
              value={newGroupName}
              onChange={(e) => setNewGroupName(e.target.value)}
            />
          </FormField>
          <div className="flex justify-end gap-2">
            <Button variant="tertiary" onClick={() => setShowNewGroup(false)}>
              {t.common.cancel}
            </Button>
            <Button type="submit" variant="primary">
              {t.common.create}
            </Button>
          </div>
        </form>
      </ModalDialog>

      {/* Set Permission Modal */}
      <ModalDialog
        open={showPermModal}
        onClose={() => setShowPermModal(false)}
        title={t.permissions.addPerm}
      >
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            if (!selectedSgid) return;
            await apiRequest(
              `/instances/${activeInstanceId}/servers/${activeSid}/server-groups/${selectedSgid}/permissions`,
              {
                method: 'PUT',
                body: JSON.stringify({
                  permname: permName.trim(),
                  permvalue: Number(permVal) || 0,
                  permnegated: permNegated,
                  permskip: permSkip,
                }),
              },
            );
            setShowPermModal(false);
            await loadSelectedGroup();
          }}
        >
          <FormField label={t.permissions.permName} htmlFor="perm-name" required>
            <Input
              id="perm-name"
              required
              className="font-mono"
              value={permName}
              onChange={(e) => setPermName(e.target.value)}
              placeholder="i_channel_join_power"
            />
          </FormField>
          <FormField label={t.permissions.permValue} htmlFor="perm-val" required>
            <Input
              id="perm-val"
              type="number"
              required
              value={permVal}
              onChange={(e) => setPermVal(e.target.value)}
            />
          </FormField>
          <div className="flex flex-wrap gap-6 mb-5">
            <CheckboxField
              label={t.permissions.permSkip}
              checked={permSkip}
              onChange={setPermSkip}
            />
            <CheckboxField
              label={t.permissions.permNegated}
              checked={permNegated}
              onChange={setPermNegated}
            />
          </div>
          <div className="flex justify-end gap-2">
            <Button variant="tertiary" onClick={() => setShowPermModal(false)}>
              {t.common.cancel}
            </Button>
            <Button type="submit" variant="primary">
              {t.common.save}
            </Button>
          </div>
        </form>
      </ModalDialog>

      {/* New Token Modal */}
      <ModalDialog
        open={showNewToken}
        onClose={() => setShowNewToken(false)}
        title={t.permissions.newToken}
      >
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            await apiRequest(`/instances/${activeInstanceId}/servers/${activeSid}/tokens`, {
              method: 'POST',
              body: JSON.stringify({
                token_type: 0,
                group_id: Number(tokGroupId) || groups[0]?.sgid || 6,
                description: tokDesc,
                count: Number(tokCount) || 1,
              }),
            });
            setShowNewToken(false);
            setTokDesc('');
            await loadBase();
          }}
        >
          <FormField label={t.permissions.serverGroup} htmlFor="tok-group" required>
            <Select
              id="tok-group"
              value={tokGroupId}
              onChange={(e) => setTokGroupId(e.target.value)}
            >
              {groups.map((g) => (
                <option key={g.sgid} value={g.sgid}>
                  #{g.sgid} {g.name}
                </option>
              ))}
            </Select>
          </FormField>
          <FormField label={t.permissions.tokenCount} htmlFor="tok-count">
            <Input
              id="tok-count"
              type="number"
              min={1}
              max={20}
              value={tokCount}
              onChange={(e) => setTokCount(e.target.value)}
            />
          </FormField>
          <FormField label={t.common.description} htmlFor="tok-desc">
            <Input
              id="tok-desc"
              value={tokDesc}
              onChange={(e) => setTokDesc(e.target.value)}
            />
          </FormField>
          <div className="flex justify-end gap-2">
            <Button variant="tertiary" onClick={() => setShowNewToken(false)}>
              {t.common.cancel}
            </Button>
            <Button type="submit" variant="primary">
              {t.common.create}
            </Button>
          </div>
        </form>
      </ModalDialog>
    </div>
  );
}
