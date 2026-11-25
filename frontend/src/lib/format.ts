import type { Dict, Locale } from '../i18n';
import type { BadgeTone } from '../components/ui/StatusBadge';

export function formatBytesPerSec(bps: number): string {
  if (bps < 1024) return `${bps} B/s`;
  if (bps < 1024 * 1024) return `${(bps / 1024).toFixed(1)} KB/s`;
  return `${(bps / (1024 * 1024)).toFixed(2)} MB/s`;
}

export function splitThroughputParts(bps: number): { num: string; unit: string } {
  if (bps < 1024) return { num: String(Math.round(bps)), unit: 'B/s' };
  if (bps < 1024 * 1024) return { num: (bps / 1024).toFixed(1), unit: 'KB/s' };
  return { num: (bps / (1024 * 1024)).toFixed(2), unit: 'MB/s' };
}

export function formatDuration(secs: number): string {
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (d > 0) return `${d}d ${h}h ${m}m`;
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}

export function formatProtocolBadge(protocol: string): string {
  if (protocol === 'raw_tcp') return 'RAW TCP';
  if (protocol === 'ssh') return 'SSH';
  if (protocol === 'web_query') return 'WebQuery';
  return protocol.toUpperCase();
}

export function formatInstanceStatus(status: string, t: Dict): string {
  if (status === 'online') return t.common.online;
  if (status === 'connecting') return t.common.connecting;
  return t.common.offline;
}

export function formatRoleLabel(role: string, locale: Locale): { label: string; tone: BadgeTone } {
  const zh = locale === 'zh-CN';
  switch (role) {
    case 'super_admin':
      return { label: zh ? '超级管理员' : 'Super Admin', tone: 'action' };
    case 'server_admin':
      return { label: zh ? '实例管理员' : 'Server Admin', tone: 'action' };
    case 'moderator':
      return { label: zh ? '协管巡查' : 'Moderator', tone: 'success' };
    case 'auditor':
      return { label: zh ? '只读审计' : 'Auditor', tone: 'neutral' };
    default:
      return { label: zh ? '团队成员' : 'Member', tone: 'neutral' };
  }
}

export function formatTriggerKind(kind: string, locale: Locale): { label: string; tone: BadgeTone } {
  const zh = locale === 'zh-CN';
  if (kind === 'pre_rollback') {
    return { label: zh ? '回滚前保护' : 'Pre-Rollback', tone: 'warning' };
  }
  if (kind === 'scheduled' || kind === 'cron') {
    return { label: zh ? '定时备份' : 'Scheduled', tone: 'success' };
  }
  if (kind === 'imported' || kind === 'import') {
    return { label: zh ? '外部导入' : 'Imported', tone: 'neutral' };
  }
  return { label: zh ? '手动快照' : 'Manual', tone: 'action' };
}

export function formatAuditAction(action: string, locale: Locale): { label: string; tone: BadgeTone } {
  const zh = locale === 'zh-CN';
  const map: Record<string, { zh: string; en: string; tone: BadgeTone }> = {
    'setup.init': { zh: '系统初始化', en: 'System Init', tone: 'action' },
    'auth.login': { zh: '管理员登录', en: 'Admin Sign-in', tone: 'success' },
    'instance.create': { zh: '接入实例', en: 'Instance Added', tone: 'action' },
    'instance.delete': { zh: '移除实例', en: 'Instance Deleted', tone: 'danger' },
    'channel.create': { zh: '新建频道', en: 'Channel Created', tone: 'action' },
    'channel.delete': { zh: '删除频道', en: 'Channel Deleted', tone: 'warning' },
    'snapshot.create': { zh: '创建快照', en: 'Snapshot Saved', tone: 'success' },
    'snapshot.deploy': { zh: '回滚快照', en: 'Snapshot Deployed', tone: 'warning' },
    'snapshot.delete': { zh: '删除快照', en: 'Snapshot Deleted', tone: 'danger' },
    'policy.create': { zh: '新建备份策略', en: 'Policy Created', tone: 'action' },
    'policy.delete': { zh: '移除备份策略', en: 'Policy Deleted', tone: 'warning' },
    'automation.create': { zh: '新建巡检规则', en: 'Rule Created', tone: 'action' },
    'automation.delete': { zh: '删除巡检规则', en: 'Rule Deleted', tone: 'warning' },
    'automation.afk_mover': { zh: '自动移入挂机', en: 'AFK Auto-Move', tone: 'success' },
    'automation.timed_broadcast': { zh: '定时广播下发', en: 'Timed Broadcast', tone: 'action' },
    'automation.alert_packet_loss': { zh: '高丢包告警', en: 'Loss Alert', tone: 'warning' },
    'automation.alert_offline': { zh: '离线探活告警', en: 'Offline Alert', tone: 'danger' },
    'webhook.create': { zh: '绑定 Webhook', en: 'Webhook Added', tone: 'action' },
    'webhook.delete': { zh: '移除 Webhook', en: 'Webhook Removed', tone: 'warning' },
    'ban.create': { zh: '新增封禁', en: 'Ban Added', tone: 'danger' },
    'ban.delete': { zh: '解除封禁', en: 'Ban Lifted', tone: 'success' },
    'group.create': { zh: '新建权限组', en: 'Group Created', tone: 'action' },
    'token.create': { zh: '签发特权密钥', en: 'Token Minted', tone: 'action' },
    'client.kick': { zh: '踢出用户', en: 'Client Kicked', tone: 'warning' },
    'client.ban': { zh: '封禁用户', en: 'Client Banned', tone: 'danger' },
  };
  const hit = map[action];
  if (hit) return { label: zh ? hit.zh : hit.en, tone: hit.tone };
  return { label: action, tone: 'neutral' };
}

export function formatCompactTime(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const now = new Date();
  const sameDay =
    d.getFullYear() === now.getFullYear() &&
    d.getMonth() === now.getMonth() &&
    d.getDate() === now.getDate();
  if (sameDay) {
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  }
  return `${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')} ${d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`;
}
