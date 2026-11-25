export interface ProblemDetails {
  type: string;
  title: string;
  status: number;
  detail: string;
  code: string;
  ts_error_id?: number;
}

export class ApiError extends Error {
  status: number;
  code: string;
  tsErrorId?: number;

  constructor(problem: ProblemDetails) {
    super(problem.detail || problem.title || `HTTP ${problem.status}`);
    this.status = problem.status;
    this.code = problem.code;
    this.tsErrorId = problem.ts_error_id;
  }
}

let accessTokenMemory: string | null = null;
let refreshInFlight: Promise<string | null> | null = null;

export function getAccessToken(): string | null {
  return accessTokenMemory;
}

export function setAccessToken(token: string | null): void {
  accessTokenMemory = token;
}

async function tryRefreshAccessToken(): Promise<string | null> {
  if (refreshInFlight) return refreshInFlight;
  refreshInFlight = (async () => {
    try {
      const resp = await fetch('/api/v1/auth/refresh', {
        method: 'POST',
        credentials: 'include',
      });
      if (!resp.ok) {
        setAccessToken(null);
        return null;
      }
      const data = await resp.json();
      if (data?.access_token) {
        setAccessToken(data.access_token);
        return data.access_token as string;
      }
      setAccessToken(null);
      return null;
    } catch {
      setAccessToken(null);
      return null;
    } finally {
      refreshInFlight = null;
    }
  })();
  return refreshInFlight;
}

export async function apiRequest<T>(
  path: string,
  options: RequestInit = {},
  retryAuth = true,
): Promise<T> {
  const headers = new Headers(options.headers || {});
  if (options.body && !headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }
  if (accessTokenMemory && !headers.has('Authorization')) {
    headers.set('Authorization', `Bearer ${accessTokenMemory}`);
  }

  const resp = await fetch(`/api/v1${path}`, {
    ...options,
    headers,
    credentials: 'include',
  });

  if (resp.status === 401 && retryAuth && !path.startsWith('/auth/login') && !path.startsWith('/auth/refresh')) {
    const renewed = await tryRefreshAccessToken();
    if (renewed) {
      return apiRequest<T>(path, options, false);
    }
  }

  if (!resp.ok) {
    let problem: ProblemDetails;
    try {
      problem = await resp.json();
    } catch {
      problem = {
        type: 'about:blank',
        title: resp.statusText,
        status: resp.status,
        detail: resp.statusText,
        code: `HTTP_${resp.status}`,
      };
    }
    throw new ApiError(problem);
  }

  if (resp.status === 204) {
    return undefined as T;
  }

  const text = await resp.text();
  if (!text) return undefined as T;
  return JSON.parse(text) as T;
}

export async function restoreSession(): Promise<{
  initialized: boolean;
  user: AuthUserProfile | null;
}> {
  const setup = await apiRequest<{ initialized: boolean }>('/setup/status', {}, false);
  if (!setup.initialized) {
    return { initialized: false, user: null };
  }
  const token = await tryRefreshAccessToken();
  if (!token) {
    return { initialized: true, user: null };
  }
  try {
    const user = await apiRequest<AuthUserProfile>('/auth/me');
    return { initialized: true, user };
  } catch {
    setAccessToken(null);
    return { initialized: true, user: null };
  }
}

// --- Shared Domain Types ---

export interface InstanceRoleBinding {
  instance_id: string;
  role: 'server_admin' | 'moderator' | 'auditor';
}

export interface AuthUserProfile {
  id: string;
  username: string;
  display_name: string;
  global_role: 'super_admin' | 'member';
  totp_enabled: boolean;
  instance_roles: InstanceRoleBinding[];
}

export interface InstanceView {
  id: string;
  name: string;
  protocol: 'raw_tcp' | 'ssh' | 'web_query';
  host: string;
  port: number;
  username: string;
  default_virtual_server_id: number;
  tls_verify: boolean;
  enabled: boolean;
  status: string;
  last_error: string | null;
  my_role: 'super_admin' | 'server_admin' | 'moderator' | 'auditor';
  created_at: string;
}

export interface VirtualServerSummary {
  id: number;
  port: number;
  status: string;
  clients_online: number;
  query_clients_online: number;
  max_clients: number;
  uptime_secs: number;
  name: string;
  autostart: boolean;
}

export interface ServerInfoSnapshot {
  virtual_server_id: number;
  unique_identifier: string;
  name: string;
  welcome_message: string;
  platform: string;
  version: string;
  max_clients: number;
  clients_online: number;
  query_clients_online: number;
  channels_online: number;
  uptime_secs: number;
  total_ping_ms: number;
  total_packetloss_pct: number;
  bw_in_bps: number;
  bw_out_bps: number;
  password_flag: boolean;
  hostbanner_url: string;
  hostbanner_gfx_url: string;
  hostbutton_tooltip: string;
  hostbutton_url: string;
  status: string;
}

export interface ChannelNode {
  cid: number;
  pid: number;
  channel_order: number;
  channel_name: string;
  channel_topic: string;
  total_clients: number;
  max_clients: number;
  is_permanent: boolean;
  is_semi_permanent: boolean;
  is_default: boolean;
  has_password: boolean;
  codec: number;
  codec_quality: number;
  needed_talk_power: number;
}

export interface OnlineClient {
  clid: number;
  cid: number;
  cldbid: number;
  uid: string;
  nickname: string;
  client_type: number;
  input_muted: boolean;
  output_muted: boolean;
  away: boolean;
  away_message: string;
  is_talking: boolean;
  server_groups: number[];
  channel_group_id: number;
  idle_time_ms: number;
  connected_time_ms: number;
  ip: string;
  country: string;
  version: string;
  platform: string;
  ping_ms: number;
}

export interface LiveTreeState {
  server_info: ServerInfoSnapshot | null;
  channels: ChannelNode[];
  clients: OnlineClient[];
  updated_at: string;
}

export interface MetricPoint {
  recorded_at: string;
  clients_online: number;
  max_clients: number;
  channels_online: number;
  avg_ping_ms: number;
  packet_loss_pct: number;
  bw_in_bps: number;
  bw_out_bps: number;
}

export interface ServerGroup {
  sgid: number;
  name: string;
  group_type: number;
  icon_id: number;
  savedb: boolean;
  sortid: number;
  namemode: number;
}

export interface PermissionEntry {
  permid: number;
  permname: string;
  permvalue: number;
  permnegated: boolean;
  permskip: boolean;
}

export interface GroupMember {
  cldbid: number;
  nickname: string;
  uid: string;
}

export interface PrivilegeKey {
  token: string;
  token_type: number;
  token_id1: number;
  token_id2: number;
  created: number;
  description: string;
}

export interface BanRule {
  banid: number;
  ip: string;
  name: string;
  uid: string;
  created: number;
  duration_secs: number;
  invoker_name: string;
  invoker_uid: string;
  reason: string;
  enforcements: number;
}

export interface DbClient {
  cldbid: number;
  uid: string;
  nickname: string;
  created: number;
  last_connected: number;
  total_connections: number;
  last_ip: string;
  description: string;
}

export interface ComplaintEntry {
  tcldbid: number;
  tname: string;
  fcldbid: number;
  fname: string;
  message: string;
  timestamp: number;
}

export interface TsLogEntry {
  line: string;
  timestamp: string;
  level: string;
  channel: string;
  message: string;
}

export interface AuditLogView {
  id: string;
  actor_username: string;
  instance_id: string | null;
  virtual_server_id: number | null;
  action: string;
  target: string;
  detail_json: string;
  ip_addr: string;
  created_at: string;
}

export interface SnapshotSummary {
  server_name: string;
  max_clients: number;
  channel_count: number;
  group_count: number;
  channel_names: string[];
  group_names: string[];
}

export interface SnapshotView {
  id: string;
  instance_id: string;
  virtual_server_id: number;
  name: string;
  trigger_kind: string;
  summary: SnapshotSummary;
  size_bytes: number;
  created_at: string;
}

export interface SnapshotDiffResp {
  snapshot_name: string;
  against_label: string;
  snapshot_summary: SnapshotSummary;
  target_summary: SnapshotSummary;
  channels_added_in_target: string[];
  channels_removed_from_target: string[];
  groups_added_in_target: string[];
  groups_removed_from_target: string[];
}

export interface BackupPolicyView {
  id: string;
  instance_id: string;
  virtual_server_id: number;
  cron_expr: string;
  retention_count: number;
  enabled: boolean;
  last_run_at: string | null;
  last_status: string | null;
  created_at: string;
}

export interface AutomationRuleView {
  id: string;
  instance_id: string;
  virtual_server_id: number;
  name: string;
  rule_kind: 'afk_mover' | 'timed_broadcast' | 'alert_offline' | 'alert_packet_loss';
  config: Record<string, unknown>;
  enabled: boolean;
  last_triggered_at: string | null;
  created_at: string;
}

export interface WebhookView {
  id: string;
  name: string;
  url: string;
  has_signing_secret: boolean;
  events_csv: string;
  enabled: boolean;
  last_delivery_status: string | null;
  created_at: string;
}

export interface UserRowView {
  id: string;
  username: string;
  display_name: string;
  global_role: 'super_admin' | 'member';
  totp_enabled: boolean;
  is_active: boolean;
  last_login_at: string | null;
  created_at: string;
  instance_roles: InstanceRoleBinding[];
}
