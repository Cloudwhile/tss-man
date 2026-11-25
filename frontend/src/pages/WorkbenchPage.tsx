import React, { useCallback, useEffect, useState } from 'react';
import {
  Ban,
  Hash,
  Lock,
  Megaphone,
  MessageSquare,
  Mic,
  MicOff,
  MoveRight,
  Plus,
  Shield,
  Sparkles,
  Trash2,
  UserMinus,
  Volume2,
  VolumeX,
} from 'lucide-react';
import { useApp } from '../context/AppContext';
import { useI18n } from '../i18n';
import {
  apiRequest,
  ChannelNode,
  LiveTreeState,
  OnlineClient,
  ServerGroup,
} from '../lib/api';
import { AlertBanner } from '../components/ui/AlertBanner';
import { Button } from '../components/ui/Button';
import { CopyButton } from '../components/ui/CopyButton';
import { EmptyState } from '../components/ui/EmptyState';
import {
  CheckboxField,
  FormField,
  Input,
  Select,
  Textarea,
} from '../components/ui/FormControls';
import { ModalDialog } from '../components/ui/ModalDialog';
import { PopoverMenu } from '../components/ui/PopoverMenu';
import { SlotMeter } from '../components/ui/SlotMeter';
import { StatusBadge } from '../components/ui/StatusBadge';

function formatMs(ms: number): string {
  const secs = Math.floor(ms / 1000);
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  if (m >= 60) {
    const h = Math.floor(m / 60);
    return `${h}h ${m % 60}m`;
  }
  return `${m}m ${s}s`;
}

function formatDuration(secs: number): string {
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (d > 0) return `${d}d ${h}h`;
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}

export function WorkbenchPage() {
  const { t } = useI18n();
  const { instances, activeInstanceId, activeSid, subscribeWs } = useApp();

  const [tree, setTree] = useState<LiveTreeState | null>(null);
  const [groups, setGroups] = useState<ServerGroup[]>([]);
  const [treeFilter, setTreeFilter] = useState('');
  const [selectedChannelId, setSelectedChannelId] = useState<number | null>(null);
  const [selectedClientId, setSelectedClientId] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const [channelModalMode, setChannelModalMode] = useState<'create' | 'edit' | null>(null);
  const [chName, setChName] = useState('');
  const [chTopic, setChTopic] = useState('');
  const [chParent, setChParent] = useState('0');
  const [chMaxClients, setChMaxClients] = useState('-1');
  const [chTalkPower, setChTalkPower] = useState('0');
  const [chPermanent, setChPermanent] = useState(true);

  const [showBroadcast, setShowBroadcast] = useState(false);
  const [broadcastMsg, setBroadcastMsg] = useState('');

  const [clientAction, setClientAction] = useState<
    'move' | 'poke' | 'message' | 'kick_ch' | 'kick_srv' | 'ban' | 'groups' | null
  >(null);
  const [targetMoveCid, setTargetMoveCid] = useState('');
  const [actionText, setActionText] = useState('');
  const [banDuration, setBanDuration] = useState('3600');

  const activeInst = instances.find((i) => i.id === activeInstanceId);
  const canModerate =
    activeInst?.my_role === 'super_admin' ||
    activeInst?.my_role === 'server_admin' ||
    activeInst?.my_role === 'moderator';
  const canAdmin =
    activeInst?.my_role === 'super_admin' || activeInst?.my_role === 'server_admin';

  const loadTree = useCallback(async () => {
    if (!activeInstanceId) return;
    try {
      const [treeRes, grpRes] = await Promise.all([
        apiRequest<LiveTreeState>(`/instances/${activeInstanceId}/servers/${activeSid}/tree`),
        apiRequest<ServerGroup[]>(
          `/instances/${activeInstanceId}/servers/${activeSid}/server-groups`,
        ).catch(() => [] as ServerGroup[]),
      ]);
      setTree(treeRes);
      setGroups(grpRes);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load workbench');
    }
  }, [activeInstanceId, activeSid]);

  useEffect(() => {
    loadTree();
  }, [loadTree]);

  useEffect(() => {
    return subscribeWs((msg) => {
      if (msg.instance_id === activeInstanceId && msg.virtual_server_id === activeSid) {
        if (msg.event === 'state.delta' && msg.payload) {
          setTree((prev) => ({
            server_info:
              (msg.payload.server_info as LiveTreeState['server_info']) ??
              prev?.server_info ??
              null,
            channels: (msg.payload.channels as ChannelNode[]) ?? prev?.channels ?? [],
            clients: (msg.payload.clients as OnlineClient[]) ?? prev?.clients ?? [],
            updated_at: msg.timestamp,
          }));
        }
      }
    });
  }, [activeInstanceId, activeSid, subscribeWs]);

  if (!activeInstanceId || !tree) {
    return (
      <EmptyState
        icon={Volume2}
        title={t.workbench.emptyTitle}
        description={t.workbench.emptyDesc}
      />
    );
  }

  const selectedChannel = tree.channels.find((c) => c.cid === selectedChannelId) ?? null;
  const selectedClient = tree.clients.find((c) => c.clid === selectedClientId) ?? null;
  const voiceClients = tree.clients.filter((c) => c.client_type === 0);
  const normalizedFilter = treeFilter.trim().toLowerCase();

  const clientActionLabel =
    clientAction === 'move'
      ? t.workbench.moveClientBtn
      : clientAction === 'poke'
        ? t.workbench.pokeBtn
        : clientAction === 'message'
          ? t.workbench.messageBtn
          : clientAction === 'kick_srv'
            ? t.workbench.kickServerBtn
            : clientAction === 'kick_ch'
              ? t.workbench.kickChannelBtn
              : clientAction === 'ban'
                ? t.workbench.banClientBtn
                : clientAction === 'groups'
                  ? t.workbench.groupsBtn
                  : '';

  const openCreateChannel = (parentCid = 0) => {
    setChannelModalMode('create');
    setChName('');
    setChTopic('');
    setChParent(String(parentCid));
    setChMaxClients('-1');
    setChTalkPower('0');
    setChPermanent(true);
  };

  const openEditChannel = (ch: ChannelNode) => {
    setChannelModalMode('edit');
    setChName(ch.channel_name);
    setChTopic(ch.channel_topic);
    setChParent(String(ch.pid));
    setChMaxClients(String(ch.max_clients));
    setChTalkPower(String(ch.needed_talk_power));
    setChPermanent(ch.is_permanent);
  };

  const handleSaveChannel = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    try {
      if (channelModalMode === 'create') {
        await apiRequest(`/instances/${activeInstanceId}/servers/${activeSid}/channels`, {
          method: 'POST',
          body: JSON.stringify({
            name: chName.trim(),
            parent_id: Number(chParent) || 0,
            topic: chTopic,
            max_clients: Number(chMaxClients),
            needed_talk_power: Number(chTalkPower) || 0,
            is_permanent: chPermanent,
          }),
        });
      } else if (channelModalMode === 'edit' && selectedChannel) {
        await apiRequest(
          `/instances/${activeInstanceId}/servers/${activeSid}/channels/${selectedChannel.cid}`,
          {
            method: 'PUT',
            body: JSON.stringify({
              name: chName.trim(),
              topic: chTopic,
              max_clients: Number(chMaxClients),
              needed_talk_power: Number(chTalkPower) || 0,
              is_permanent: chPermanent,
            }),
          },
        );
      }
      setChannelModalMode(null);
      await loadTree();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Channel save failed');
    }
  };

  const handleDeleteChannel = async (cid: number) => {
    setError(null);
    try {
      await apiRequest(
        `/instances/${activeInstanceId}/servers/${activeSid}/channels/${cid}?force=true`,
        { method: 'DELETE' },
      );
      setSelectedChannelId(null);
      await loadTree();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Delete channel failed');
    }
  };

  const handleClientActionSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedClient || !clientAction) return;
    setError(null);
    const base = `/instances/${activeInstanceId}/servers/${activeSid}/clients/${selectedClient.clid}`;
    try {
      if (clientAction === 'move') {
        await apiRequest(`${base}/move`, {
          method: 'POST',
          body: JSON.stringify({ cid: Number(targetMoveCid) }),
        });
      } else if (clientAction === 'poke') {
        await apiRequest(`${base}/poke`, {
          method: 'POST',
          body: JSON.stringify({ msg: actionText }),
        });
        setNotice(t.workbench.pokedNotice.replace('{name}', selectedClient.nickname));
      } else if (clientAction === 'message') {
        await apiRequest(`${base}/message`, {
          method: 'POST',
          body: JSON.stringify({ msg: actionText }),
        });
        setNotice(t.workbench.messagedNotice.replace('{name}', selectedClient.nickname));
      } else if (clientAction === 'kick_ch' || clientAction === 'kick_srv') {
        await apiRequest(`${base}/kick`, {
          method: 'POST',
          body: JSON.stringify({
            from_server: clientAction === 'kick_srv',
            reason: actionText || 'Kicked by operator',
          }),
        });
        if (clientAction === 'kick_srv') setSelectedClientId(null);
      } else if (clientAction === 'ban') {
        await apiRequest(`${base}/ban`, {
          method: 'POST',
          body: JSON.stringify({
            duration_secs: Number(banDuration) || 0,
            reason: actionText || 'Banned by operator',
          }),
        });
        setSelectedClientId(null);
      }
      setClientAction(null);
      setActionText('');
      await loadTree();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Client action failed');
    }
  };

  const toggleClientGroup = async (sgid: number, assigned: boolean) => {
    if (!selectedClient) return;
    setError(null);
    try {
      await apiRequest(
        `/instances/${activeInstanceId}/servers/${activeSid}/clients/${selectedClient.clid}/groups`,
        {
          method: 'POST',
          body: JSON.stringify({
            action: assigned ? 'remove' : 'add',
            sgid,
            cldbid: selectedClient.cldbid,
          }),
        },
      );
      await loadTree();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Group change failed');
    }
  };

  const branchHasMatch = (ch: ChannelNode): boolean => {
    if (!normalizedFilter) return true;
    if (ch.channel_name.toLowerCase().includes(normalizedFilter)) return true;
    if (
      voiceClients.some(
        (cl) => cl.cid === ch.cid && cl.nickname.toLowerCase().includes(normalizedFilter),
      )
    ) {
      return true;
    }
    return tree.channels
      .filter((child) => child.pid === ch.cid)
      .some((child) => branchHasMatch(child));
  };

  const renderChannelBranch = (pid: number, depth: number): React.ReactNode => {
    const children = tree.channels
      .filter((c) => c.pid === pid && branchHasMatch(c))
      .sort((a, b) => a.channel_order - b.channel_order || a.cid - b.cid);

    return children.map((ch) => {
      const chClients = voiceClients.filter((cl) => cl.cid === ch.cid);
      const isSelected = selectedChannelId === ch.cid && !selectedClientId;
      return (
        <div
          key={ch.cid}
          className={depth > 0 ? 'ms-4 ps-3 border-s border-border/80' : undefined}
        >
          <button
            type="button"
            onClick={() => {
              setSelectedChannelId(ch.cid);
              setSelectedClientId(null);
            }}
            className={`w-full flex items-center justify-between gap-2 px-2.5 py-1.5 rounded-[var(--radius-sm)] text-left text-sm transition-colors cursor-pointer ${
              isSelected
                ? 'bg-action-subtle text-action-text font-semibold'
                : 'hover:bg-surface text-text font-medium'
            }`}
          >
            <span className="flex items-center gap-2 min-w-0">
              {ch.has_password ? (
                <Lock className="w-3.5 h-3.5 text-warning shrink-0" aria-label="Password protected" />
              ) : (
                <Hash className="w-3.5 h-3.5 text-text-muted shrink-0" aria-hidden="true" />
              )}
              <span className="truncate">{ch.channel_name}</span>
              {ch.is_default ? (
                <span className="font-mono text-[10px] px-1.5 py-0.5 rounded-[var(--radius-sm)] bg-surface-sunken text-text-secondary shrink-0">
                  {t.workbench.defaultTag}
                </span>
              ) : null}
            </span>
            <span className="font-mono text-xs text-text-muted tabular-nums shrink-0">
              {chClients.length}
              {ch.max_clients > 0 ? `/${ch.max_clients}` : ''}
            </span>
          </button>

          {chClients.length > 0 ? (
            <div className="ms-4 ps-3 border-s border-border/60 py-0.5 space-y-0.5">
              {chClients.map((cl) => {
                const clSelected = selectedClientId === cl.clid;
                const matchesSearch =
                  !normalizedFilter || cl.nickname.toLowerCase().includes(normalizedFilter);
                return (
                  <button
                    key={cl.clid}
                    type="button"
                    onClick={() => {
                      setSelectedClientId(cl.clid);
                      setSelectedChannelId(null);
                    }}
                    className={`w-full flex items-center justify-between gap-2 px-2.5 py-1.5 rounded-[var(--radius-sm)] text-left text-sm transition-colors cursor-pointer ${
                      clSelected
                        ? 'bg-action-subtle text-action-text font-semibold'
                        : matchesSearch
                          ? 'hover:bg-surface text-text-secondary hover:text-text'
                          : 'opacity-45 hover:opacity-80 text-text-muted'
                    }`}
                  >
                    <span className="flex items-center gap-2 min-w-0">
                      {cl.away ? (
                        <VolumeX
                          className="w-3.5 h-3.5 text-warning shrink-0"
                          aria-label={t.workbench.awayStatus}
                        />
                      ) : cl.input_muted ? (
                        <MicOff className="w-3.5 h-3.5 text-danger shrink-0" aria-label="Muted" />
                      ) : (
                        <Mic
                          className={`w-3.5 h-3.5 shrink-0 ${
                            cl.is_talking ? 'text-success' : 'text-text-muted'
                          }`}
                          aria-label={cl.is_talking ? 'Talking' : 'Idle'}
                        />
                      )}
                      <span className="truncate">{cl.nickname}</span>
                      {cl.idle_time_ms >= 300_000 ? (
                        <span className="font-mono text-[10px] px-1.5 py-0.5 rounded-[var(--radius-sm)] bg-warning-subtle text-warning-text shrink-0">
                          {t.workbench.afkTag}
                        </span>
                      ) : null}
                    </span>
                    <span className="font-mono text-xs text-text-muted tabular-nums shrink-0">
                      {cl.ping_ms.toFixed(0)}ms
                    </span>
                  </button>
                );
              })}
            </div>
          ) : null}

          {renderChannelBranch(ch.cid, depth + 1)}
        </div>
      );
    });
  };

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-xl sm:text-2xl font-semibold text-text tracking-tight">{t.workbench.title}</h1>
          <p className="mt-0.5 text-sm text-text-secondary tss-prose">{t.workbench.subtitle}</p>
        </div>
        {canModerate ? (
          <div className="flex flex-wrap items-center gap-2">
            <Button variant="secondary" onClick={() => setShowBroadcast(true)}>
              <Megaphone className="w-4 h-4" aria-hidden="true" />
              <span>{t.workbench.broadcastBtn}</span>
            </Button>
            <Button variant="primary" onClick={() => openCreateChannel(0)}>
              <Plus className="w-4 h-4" aria-hidden="true" />
              <span>{t.workbench.newChannelBtn}</span>
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

      {/* Unified Dual-Pane Comms Frame */}
      <div className="tss-frame overflow-hidden grid grid-cols-1 lg:grid-cols-12">
        {/* Live Channel Tree */}
        <section className="lg:col-span-7 border-b lg:border-b-0 lg:border-r border-border flex flex-col min-w-0">
          <div className="px-4 py-3 bg-surface/60 border-b border-border flex flex-wrap items-center justify-between gap-3">
            <div className="flex items-center gap-3 min-w-0 flex-1">
              <div className="font-semibold text-sm text-text truncate">
                {tree.server_info?.name ?? t.common.selectVServer}
              </div>
              <div className="w-24 shrink-0 hidden sm:block">
                <SlotMeter
                  used={voiceClients.length}
                  max={tree.server_info?.max_clients ?? 1}
                  label={t.dashboard.utilization}
                />
              </div>
              <span className="font-mono text-xs text-text-muted tabular-nums shrink-0">
                {voiceClients.length}/{tree.server_info?.max_clients ?? 0}
              </span>
            </div>
            <search className="w-full sm:w-52">
              <Input
                type="search"
                aria-label={t.workbench.filterTree}
                placeholder={t.workbench.filterTree}
                value={treeFilter}
                onChange={(e) => setTreeFilter(e.target.value)}
                className="min-h-8 py-1 text-xs"
              />
            </search>
          </div>

          <div className="p-3 space-y-0.5 overflow-y-auto max-h-[65vh] tss-scroller">
            {renderChannelBranch(0, 0)}
          </div>
        </section>

        {/* Sticky Contextual Inspector */}
        <aside className="lg:col-span-5 p-5 flex flex-col justify-start min-w-0">
          {selectedClient ? (
            <div className="space-y-5">
              <div className="flex items-start justify-between gap-3 pb-4 border-b border-border">
                <div className="min-w-0">
                  <div className="text-[11px] font-semibold uppercase tracking-wide text-text-muted">
                    {t.workbench.clientInfo}
                  </div>
                  <h2 className="mt-1 text-lg font-semibold text-text truncate">
                    {selectedClient.nickname}
                  </h2>
                  <div className="mt-1 flex items-center gap-1.5 min-w-0">
                    <code
                      id="wb-client-uid"
                      className="font-mono text-xs text-text-muted truncate"
                    >
                      {selectedClient.uid}
                    </code>
                    <CopyButton targetId="wb-client-uid" />
                  </div>
                </div>
                <StatusBadge
                  variant="dot"
                  tone={selectedClient.away ? 'warning' : 'success'}
                  label={selectedClient.away ? t.workbench.awayStatus : t.common.online}
                />
              </div>

              <dl className="grid grid-cols-2 gap-2.5 text-sm">
                <div className="tss-well p-3">
                  <dt className="text-[11px] font-medium text-text-muted">{t.workbench.clientIp}</dt>
                  <dd className="mt-1 flex items-center justify-between gap-1">
                    <span className="truncate">
                      <span id="wb-client-ip" className="font-mono text-xs font-medium text-text">
                        {selectedClient.ip}
                      </span>
                      <span className="ms-1 text-xs text-text-muted">
                        ({selectedClient.country || '—'})
                      </span>
                    </span>
                    <CopyButton targetId="wb-client-ip" />
                  </dd>
                </div>
                <div className="tss-well p-3">
                  <dt className="text-[11px] font-medium text-text-muted">{t.workbench.idleTime}</dt>
                  <dd className="tabular-nums text-text mt-1 text-xs font-semibold font-mono">
                    {formatMs(selectedClient.idle_time_ms)}
                  </dd>
                </div>
                <div className="tss-well p-3">
                  <dt className="text-[11px] font-medium text-text-muted">{t.workbench.connectedTime}</dt>
                  <dd className="tabular-nums text-text mt-1 text-xs font-semibold font-mono">
                    {formatMs(selectedClient.connected_time_ms)}
                  </dd>
                </div>
                <div className="tss-well p-3">
                  <dt className="text-[11px] font-medium text-text-muted">{t.workbench.clientVersion}</dt>
                  <dd
                    className="text-xs font-medium text-text mt-1 truncate"
                    title={`${selectedClient.platform} · ${selectedClient.version}`}
                  >
                    {selectedClient.platform} · {selectedClient.version}
                  </dd>
                </div>
              </dl>

              <div>
                <div className="text-xs font-medium text-text-muted mb-2">
                  {t.workbench.groupsBtn}
                </div>
                <div className="flex flex-wrap gap-1.5">
                  {selectedClient.server_groups.map((gid) => {
                    const grp = groups.find((g) => g.sgid === gid);
                    return (
                      <span
                        key={gid}
                        className="px-2 py-0.5 text-xs rounded-[var(--radius-sm)] bg-action-subtle text-action-text font-medium"
                      >
                        {grp?.name ?? `#${gid}`}
                      </span>
                    );
                  })}
                </div>
              </div>

              {canModerate ? (
                <div className="pt-4 border-t border-border flex flex-wrap items-center gap-1.5">
                    <Button
                      size="xs"
                      variant="secondary"
                      onClick={() => {
                        setTargetMoveCid(String(tree.channels[0]?.cid ?? 1));
                        setClientAction('move');
                      }}
                    >
                      <MoveRight className="w-3.5 h-3.5" aria-hidden="true" />
                      <span>{t.workbench.moveClientBtn}</span>
                    </Button>
                    <Button
                      size="xs"
                      variant="secondary"
                      onClick={() => {
                        setActionText('');
                        setClientAction('poke');
                      }}
                    >
                      <Sparkles className="w-3.5 h-3.5" aria-hidden="true" />
                      <span>{t.workbench.pokeBtn}</span>
                    </Button>
                    <Button
                      size="xs"
                      variant="secondary"
                      onClick={() => {
                        setActionText('');
                        setClientAction('message');
                      }}
                    >
                      <MessageSquare className="w-3.5 h-3.5" aria-hidden="true" />
                      <span>{t.workbench.messageBtn}</span>
                    </Button>
                    {canAdmin ? (
                      <Button
                        size="xs"
                        variant="secondary"
                        onClick={() => setClientAction('groups')}
                      >
                        <Shield className="w-3.5 h-3.5" aria-hidden="true" />
                        <span>{t.workbench.groupsBtn}</span>
                      </Button>
                    ) : null}
                  <PopoverMenu
                    iconOnly
                    triggerLabel={t.common.moreActions}
                    items={[
                      {
                        label: t.workbench.kickChannelBtn,
                        danger: true,
                        icon: <UserMinus className="w-3.5 h-3.5" aria-hidden="true" />,
                        onClick: () => {
                          setActionText('');
                          setClientAction('kick_ch');
                        },
                      },
                      {
                        label: t.workbench.kickServerBtn,
                        danger: true,
                        icon: <UserMinus className="w-3.5 h-3.5" aria-hidden="true" />,
                        onClick: () => {
                          setActionText('');
                          setClientAction('kick_srv');
                        },
                      },
                      {
                        label: t.workbench.banClientBtn,
                        danger: true,
                        dividerBefore: true,
                        icon: <Ban className="w-3.5 h-3.5" aria-hidden="true" />,
                        onClick: () => {
                          setActionText('');
                          setClientAction('ban');
                        },
                      },
                    ]}
                  />
                </div>
              ) : null}
            </div>
          ) : selectedChannel ? (
            <div className="space-y-5">
              <div className="pb-4 border-b border-border">
                <div className="font-mono text-[11px] uppercase tracking-wide text-text-muted">
                  {t.workbench.channelInfo} · #{selectedChannel.cid}
                </div>
                <h2 className="mt-1 text-lg font-semibold text-text">
                  {selectedChannel.channel_name}
                </h2>
                {selectedChannel.channel_topic ? (
                  <p className="mt-1.5 text-sm text-text-secondary tss-prose">
                    {selectedChannel.channel_topic}
                  </p>
                ) : null}
              </div>

              <dl className="grid grid-cols-2 gap-2.5 text-sm">
                <div className="tss-well p-3">
                  <dt className="text-[11px] font-medium text-text-muted">{t.workbench.slots}</dt>
                  <dd className="tabular-nums text-text mt-1 text-xs font-semibold font-mono">
                    {selectedChannel.max_clients < 0
                      ? t.workbench.unlimited
                      : selectedChannel.max_clients}
                  </dd>
                </div>
                <div className="tss-well p-3">
                  <dt className="text-[11px] font-medium text-text-muted">{t.workbench.talkPower}</dt>
                  <dd className="tabular-nums text-text mt-1 text-xs font-semibold font-mono">
                    {selectedChannel.needed_talk_power}
                  </dd>
                </div>
                <div className="tss-well p-3">
                  <dt className="text-[11px] font-medium text-text-muted">{t.workbench.codecLabel}</dt>
                  <dd className="font-mono text-xs font-semibold text-text mt-1">
                    Opus #{selectedChannel.codec} (Q{selectedChannel.codec_quality})
                  </dd>
                </div>
                <div className="tss-well p-3">
                  <dt className="text-[11px] font-medium text-text-muted">{t.workbench.flagsLabel}</dt>
                  <dd className="text-xs font-medium text-text mt-1">
                    {selectedChannel.is_permanent
                      ? t.workbench.permanentChannel
                      : t.workbench.temporaryChannel}
                  </dd>
                </div>
              </dl>

              {canModerate ? (
                <div className="pt-4 border-t border-border flex flex-wrap gap-2">
                  <Button
                    size="xs"
                    variant="secondary"
                    onClick={() => openCreateChannel(selectedChannel.cid)}
                  >
                    <Plus className="w-3.5 h-3.5" aria-hidden="true" />
                    <span>{t.workbench.newChannelBtn}</span>
                  </Button>
                  <Button
                    size="xs"
                    variant="secondary"
                    onClick={() => openEditChannel(selectedChannel)}
                  >
                    {t.workbench.editChannelBtn}
                  </Button>
                  {!selectedChannel.is_default &&
                  (canAdmin || !selectedChannel.is_permanent) ? (
                    <Button
                      size="xs"
                      variant="danger-link"
                      onClick={() => handleDeleteChannel(selectedChannel.cid)}
                    >
                      <Trash2 className="w-3.5 h-3.5" aria-hidden="true" />
                      <span>{t.workbench.deleteChannelBtn}</span>
                    </Button>
                  ) : null}
                </div>
              ) : null}
            </div>
          ) : (
            /* Rich Virtual Server Live Overview when nothing is selected */
            <div className="space-y-5">
              <div className="pb-4 border-b border-border flex items-center justify-between gap-2">
                <div>
                  <div className="text-[11px] font-semibold uppercase tracking-wide text-text-muted">
                    {t.workbench.overviewTitle}
                  </div>
                  <h2 className="mt-1 text-base font-semibold text-text">
                    {tree.server_info?.name ?? t.common.selectVServer}
                  </h2>
                </div>
                <StatusBadge
                  variant="dot"
                  tone={tree.server_info?.status === 'online' ? 'success' : 'warning'}
                  label={
                    tree.server_info?.status === 'online' ? t.common.online : t.common.offline
                  }
                />
              </div>

              <dl className="grid grid-cols-3 gap-2.5 text-sm">
                <div className="tss-well p-3">
                  <dt className="text-[11px] font-medium text-text-muted">{t.dashboard.channelsCount}</dt>
                  <dd className="mt-1 text-lg font-semibold text-text tabular-nums leading-none">
                    {tree.channels.length}
                  </dd>
                </div>
                <div className="tss-well p-3">
                  <dt className="text-[11px] font-medium text-text-muted">{t.dashboard.kpiOnline}</dt>
                  <dd className="mt-1 text-lg font-semibold text-text tabular-nums leading-none">
                    {voiceClients.length}
                  </dd>
                </div>
                <div className="tss-well p-3">
                  <dt className="text-[11px] font-medium text-text-muted">{t.dashboard.uptime}</dt>
                  <dd className="mt-1 text-xs font-semibold font-mono text-text tabular-nums">
                    {tree.server_info ? formatDuration(tree.server_info.uptime_secs) : '—'}
                  </dd>
                </div>
              </dl>

              {tree.server_info?.welcome_message ? (
                <div>
                  <div className="text-xs text-text-muted mb-1.5">{t.instances.welcomeMsg}</div>
                  <div className="tss-well p-3 font-mono text-xs text-text-secondary leading-relaxed">
                    {tree.server_info.welcome_message}
                  </div>
                </div>
              ) : null}

              <p className="text-xs text-text-muted leading-normal tss-prose pt-2">
                {t.workbench.selectTargetPrompt}
              </p>
            </div>
          )}
        </aside>
      </div>

      {/* Create / Edit Channel Modal */}
      <ModalDialog
        open={channelModalMode !== null}
        onClose={() => setChannelModalMode(null)}
        title={
          channelModalMode === 'create'
            ? t.workbench.newChannelBtn
            : t.workbench.editChannelBtn
        }
      >
        <form onSubmit={handleSaveChannel}>
          <FormField label={t.common.name} htmlFor="ch-name" required>
            <Input
              id="ch-name"
              required
              value={chName}
              onChange={(e) => setChName(e.target.value)}
            />
          </FormField>
          <FormField label={t.workbench.topic} htmlFor="ch-topic">
            <Input
              id="ch-topic"
              value={chTopic}
              onChange={(e) => setChTopic(e.target.value)}
            />
          </FormField>
          {channelModalMode === 'create' ? (
            <FormField label={t.workbench.parentChannel} htmlFor="ch-parent">
              <Select
                id="ch-parent"
                value={chParent}
                onChange={(e) => setChParent(e.target.value)}
              >
                <option value="0">{t.workbench.rootChannel}</option>
                {tree.channels.map((c) => (
                  <option key={c.cid} value={c.cid}>
                    #{c.cid} {c.channel_name}
                  </option>
                ))}
              </Select>
            </FormField>
          ) : null}
          <div className="grid grid-cols-2 gap-4 max-w-md">
            <FormField label={t.workbench.slots} htmlFor="ch-max">
              <Input
                id="ch-max"
                type="number"
                value={chMaxClients}
                onChange={(e) => setChMaxClients(e.target.value)}
              />
            </FormField>
            <FormField label={t.workbench.talkPower} htmlFor="ch-tp">
              <Input
                id="ch-tp"
                type="number"
                value={chTalkPower}
                onChange={(e) => setChTalkPower(e.target.value)}
              />
            </FormField>
          </div>
          <div className="mb-5">
            <CheckboxField
              label={t.workbench.permanentChannel}
              checked={chPermanent}
              onChange={setChPermanent}
            />
          </div>
          <div className="flex justify-end gap-2">
            <Button variant="tertiary" onClick={() => setChannelModalMode(null)}>
              {t.common.cancel}
            </Button>
            <Button type="submit" variant="primary">
              {t.common.save}
            </Button>
          </div>
        </form>
      </ModalDialog>

      {/* Broadcast Modal */}
      <ModalDialog
        open={showBroadcast}
        onClose={() => setShowBroadcast(false)}
        title={t.workbench.broadcastBtn}
      >
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            await apiRequest(`/instances/${activeInstanceId}/servers/${activeSid}/broadcast`, {
              method: 'POST',
              body: JSON.stringify({ msg: broadcastMsg }),
            });
            setShowBroadcast(false);
            setBroadcastMsg('');
            setNotice(t.workbench.broadcastNotice);
          }}
        >
          <FormField label={t.common.message} htmlFor="srv-broadcast" required>
            <Textarea
              id="srv-broadcast"
              required
              value={broadcastMsg}
              onChange={(e) => setBroadcastMsg(e.target.value)}
            />
          </FormField>
          <div className="flex justify-end gap-2">
            <Button variant="tertiary" onClick={() => setShowBroadcast(false)}>
              {t.common.cancel}
            </Button>
            <Button type="submit" variant="primary">
              {t.workbench.broadcastBtn}
            </Button>
          </div>
        </form>
      </ModalDialog>

      {/* Client Action Modal */}
      <ModalDialog
        open={clientAction !== null && selectedClient !== null}
        onClose={() => setClientAction(null)}
        title={`${selectedClient?.nickname ?? ''} · ${clientActionLabel}`}
      >
        {clientAction === 'groups' && selectedClient ? (
          <div className="space-y-2.5">
            {groups.map((g) => {
              const assigned = selectedClient.server_groups.includes(g.sgid);
              return (
                <div
                  key={g.sgid}
                  className="flex items-center justify-between px-3 py-2 rounded-[var(--radius-DEFAULT)] border border-border bg-surface/60"
                >
                  <span className="text-sm font-medium text-text">{g.name}</span>
                  <Button
                    size="xs"
                    variant={assigned ? 'danger-link' : 'secondary'}
                    onClick={() => toggleClientGroup(g.sgid, assigned)}
                  >
                    {assigned ? t.workbench.removeGroup : t.workbench.assignGroup}
                  </Button>
                </div>
              );
            })}
          </div>
        ) : (
          <form onSubmit={handleClientActionSubmit}>
            {clientAction === 'move' ? (
              <FormField label={t.workbench.targetChannel} htmlFor="cl-move-cid" required>
                <Select
                  id="cl-move-cid"
                  value={targetMoveCid}
                  onChange={(e) => setTargetMoveCid(e.target.value)}
                >
                  {tree.channels.map((c) => (
                    <option key={c.cid} value={c.cid}>
                      #{c.cid} {c.channel_name}
                    </option>
                  ))}
                </Select>
              </FormField>
            ) : (
              <>
                {clientAction === 'ban' ? (
                  <FormField label={t.governance.banDuration} htmlFor="cl-ban-dur">
                    <Input
                      id="cl-ban-dur"
                      type="number"
                      value={banDuration}
                      onChange={(e) => setBanDuration(e.target.value)}
                    />
                  </FormField>
                ) : null}
                <FormField
                  label={
                    clientAction === 'ban' || clientAction === 'kick_srv'
                      ? t.governance.banReason
                      : t.common.message
                  }
                  htmlFor="cl-action-msg"
                  required={clientAction === 'poke' || clientAction === 'message'}
                >
                  <Input
                    id="cl-action-msg"
                    required={clientAction === 'poke' || clientAction === 'message'}
                    value={actionText}
                    onChange={(e) => setActionText(e.target.value)}
                  />
                </FormField>
              </>
            )}
            <div className="flex justify-end gap-2">
              <Button
                variant="tertiary"
                autoFocus={clientAction === 'ban' || clientAction === 'kick_srv'}
                onClick={() => setClientAction(null)}
              >
                {t.common.cancel}
              </Button>
              <Button
                type="submit"
                variant={
                  clientAction === 'ban' || clientAction === 'kick_srv'
                    ? 'danger-confirm'
                    : 'primary'
                }
              >
                {t.common.confirm}
              </Button>
            </div>
          </form>
        )}
      </ModalDialog>
    </div>
  );
}
