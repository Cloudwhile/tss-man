use crate::types::{
    AddBanParams, BanRule, ChannelGroup, ChannelNode, ComplaintEntry, CreateChannelParams,
    CreatePrivilegeKeyParams, DbClient, EditChannelParams, EditServerInfoParams, GroupMember,
    OnlineClient, PermissionEntry, PrivilegeKey, Result, ServerGroup, ServerInfoSnapshot,
    SetPermissionParams, TsLogEntry, TsNotifyEvent, TsSnapshotData, VirtualServerSummary,
};
use async_trait::async_trait;
use std::collections::BTreeMap;
use tokio::sync::broadcast;

#[async_trait]
pub trait TsProtocolDriver: Send + Sync {
    fn supports_push_events(&self) -> bool;

    fn subscribe_events(&self) -> broadcast::Receiver<TsNotifyEvent>;

    async fn test_connection(&self) -> Result<()>;

    // Virtual servers
    async fn list_virtual_servers(&self) -> Result<Vec<VirtualServerSummary>>;
    async fn start_virtual_server(&self, sid: u32) -> Result<()>;
    async fn stop_virtual_server(&self, sid: u32) -> Result<()>;
    async fn create_virtual_server(&self, name: &str, max_clients: u32, port: Option<u16>) -> Result<u32>;
    async fn delete_virtual_server(&self, sid: u32) -> Result<()>;
    async fn get_server_info(&self, sid: u32) -> Result<ServerInfoSnapshot>;
    async fn edit_server_info(&self, sid: u32, params: &EditServerInfoParams) -> Result<()>;

    // Live channels & clients
    async fn list_channels(&self, sid: u32) -> Result<Vec<ChannelNode>>;
    async fn create_channel(&self, sid: u32, params: &CreateChannelParams) -> Result<u32>;
    async fn edit_channel(&self, sid: u32, cid: u32, params: &EditChannelParams) -> Result<()>;
    async fn move_channel(&self, sid: u32, cid: u32, cpid: u32, order: u32) -> Result<()>;
    async fn delete_channel(&self, sid: u32, cid: u32, force: bool) -> Result<()>;

    async fn list_clients(&self, sid: u32) -> Result<Vec<OnlineClient>>;
    async fn kick_client(&self, sid: u32, clid: u32, from_server: bool, reason: Option<&str>) -> Result<()>;
    async fn move_client(&self, sid: u32, clid: u32, cid: u32, password: Option<&str>) -> Result<()>;
    async fn poke_client(&self, sid: u32, clid: u32, msg: &str) -> Result<()>;
    async fn ban_client(&self, sid: u32, clid: u32, duration_secs: i64, reason: Option<&str>) -> Result<()>;
    async fn send_text_message(&self, sid: u32, target_mode: u8, target: u32, msg: &str) -> Result<()>;

    // Server groups, channel groups & permissions
    async fn list_server_groups(&self, sid: u32) -> Result<Vec<ServerGroup>>;
    async fn create_server_group(&self, sid: u32, name: &str, group_type: Option<u8>) -> Result<u32>;
    async fn rename_server_group(&self, sid: u32, sgid: u32, name: &str) -> Result<()>;
    async fn delete_server_group(&self, sid: u32, sgid: u32, force: bool) -> Result<()>;
    async fn list_server_group_clients(&self, sid: u32, sgid: u32) -> Result<Vec<GroupMember>>;
    async fn add_client_to_server_group(&self, sid: u32, sgid: u32, cldbid: u32) -> Result<()>;
    async fn remove_client_from_server_group(&self, sid: u32, sgid: u32, cldbid: u32) -> Result<()>;
    async fn list_server_group_permissions(&self, sid: u32, sgid: u32) -> Result<Vec<PermissionEntry>>;
    async fn set_server_group_permission(&self, sid: u32, sgid: u32, perm: &SetPermissionParams) -> Result<()>;
    async fn remove_server_group_permission(&self, sid: u32, sgid: u32, permname: &str) -> Result<()>;

    async fn list_channel_groups(&self, sid: u32) -> Result<Vec<ChannelGroup>>;
    async fn list_privilege_keys(&self, sid: u32) -> Result<Vec<PrivilegeKey>>;
    async fn create_privilege_key(&self, sid: u32, params: &CreatePrivilegeKeyParams) -> Result<String>;
    async fn delete_privilege_key(&self, sid: u32, token: &str) -> Result<()>;

    // Governance & snapshots
    async fn list_bans(&self, sid: u32) -> Result<Vec<BanRule>>;
    async fn add_ban(&self, sid: u32, params: &AddBanParams) -> Result<u32>;
    async fn delete_ban(&self, sid: u32, banid: u32) -> Result<()>;

    async fn list_db_clients(&self, sid: u32, offset: u32, limit: u32, pattern: Option<&str>) -> Result<Vec<DbClient>>;
    async fn delete_db_client(&self, sid: u32, cldbid: u32) -> Result<()>;

    async fn list_complaints(&self, sid: u32) -> Result<Vec<ComplaintEntry>>;
    async fn delete_complaint(&self, sid: u32, tcldbid: u32, fcldbid: u32) -> Result<()>;

    async fn view_server_logs(&self, sid: u32, lines: u32) -> Result<Vec<TsLogEntry>>;
    async fn create_snapshot(&self, sid: u32) -> Result<TsSnapshotData>;
    async fn deploy_snapshot(&self, sid: u32, snapshot: &TsSnapshotData) -> Result<()>;

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

pub type RecordMap = BTreeMap<String, String>;

fn get_str(m: &RecordMap, k: &str) -> String {
    m.get(k).cloned().unwrap_or_default()
}

fn get_u32(m: &RecordMap, k: &str) -> u32 {
    m.get(k).and_then(|v| v.parse().ok()).unwrap_or(0)
}

fn get_i32(m: &RecordMap, k: &str) -> i32 {
    m.get(k).and_then(|v| v.parse().ok()).unwrap_or(0)
}

fn get_u64(m: &RecordMap, k: &str) -> u64 {
    m.get(k).and_then(|v| v.parse().ok()).unwrap_or(0)
}

fn get_i64(m: &RecordMap, k: &str) -> i64 {
    m.get(k).and_then(|v| v.parse().ok()).unwrap_or(0)
}

fn get_f64(m: &RecordMap, k: &str) -> f64 {
    m.get(k).and_then(|v| v.parse().ok()).unwrap_or(0.0)
}

fn get_bool(m: &RecordMap, k: &str) -> bool {
    m.get(k).map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false)
}

pub fn map_virtual_server(m: &RecordMap) -> VirtualServerSummary {
    VirtualServerSummary {
        id: get_u32(m, "virtualserver_id"),
        port: get_u32(m, "virtualserver_port") as u16,
        status: get_str(m, "virtualserver_status"),
        clients_online: get_u32(m, "virtualserver_clientsonline"),
        query_clients_online: get_u32(m, "virtualserver_queryclientsonline"),
        max_clients: get_u32(m, "virtualserver_maxclients"),
        uptime_secs: get_u64(m, "virtualserver_uptime"),
        name: get_str(m, "virtualserver_name"),
        autostart: get_bool(m, "virtualserver_autostart"),
    }
}

pub fn map_server_info(sid: u32, m: &RecordMap) -> ServerInfoSnapshot {
    let bw_in = get_u64(m, "connection_bytes_received_total")
        .max(get_u64(m, "connection_bandwidth_received_last_second_total"));
    let bw_out = get_u64(m, "connection_bytes_sent_total")
        .max(get_u64(m, "connection_bandwidth_sent_last_second_total"));
    ServerInfoSnapshot {
        virtual_server_id: m
            .get("virtualserver_id")
            .and_then(|v| v.parse().ok())
            .unwrap_or(sid),
        unique_identifier: get_str(m, "virtualserver_unique_identifier"),
        name: get_str(m, "virtualserver_name"),
        welcome_message: get_str(m, "virtualserver_welcomemessage"),
        platform: get_str(m, "virtualserver_platform"),
        version: get_str(m, "virtualserver_version"),
        max_clients: get_u32(m, "virtualserver_maxclients"),
        clients_online: get_u32(m, "virtualserver_clientsonline"),
        query_clients_online: get_u32(m, "virtualserver_queryclientsonline"),
        channels_online: get_u32(m, "virtualserver_channelsonline"),
        uptime_secs: get_u64(m, "virtualserver_uptime"),
        total_ping_ms: get_f64(m, "virtualserver_total_ping"),
        total_packetloss_pct: get_f64(m, "virtualserver_total_packetloss_total") * 100.0,
        bw_in_bps: m
            .get("connection_bandwidth_received_last_second_total")
            .and_then(|v| v.parse().ok())
            .unwrap_or(bw_in),
        bw_out_bps: m
            .get("connection_bandwidth_sent_last_second_total")
            .and_then(|v| v.parse().ok())
            .unwrap_or(bw_out),
        password_flag: get_bool(m, "virtualserver_flag_password"),
        hostbanner_url: get_str(m, "virtualserver_hostbanner_url"),
        hostbanner_gfx_url: get_str(m, "virtualserver_hostbanner_gfx_url"),
        hostbutton_tooltip: get_str(m, "virtualserver_hostbutton_tooltip"),
        hostbutton_url: get_str(m, "virtualserver_hostbutton_url"),
        status: m
            .get("virtualserver_status")
            .cloned()
            .unwrap_or_else(|| "online".to_string()),
    }
}

pub fn map_channel(m: &RecordMap) -> ChannelNode {
    ChannelNode {
        cid: get_u32(m, "cid"),
        pid: get_u32(m, "pid"),
        channel_order: get_u32(m, "channel_order"),
        channel_name: get_str(m, "channel_name"),
        channel_topic: get_str(m, "channel_topic"),
        total_clients: get_u32(m, "total_clients"),
        max_clients: get_i32(m, "channel_maxclients"),
        is_permanent: get_bool(m, "channel_flag_permanent"),
        is_semi_permanent: get_bool(m, "channel_flag_semi_permanent"),
        is_default: get_bool(m, "channel_flag_default"),
        has_password: get_bool(m, "channel_flag_password"),
        codec: get_u32(m, "channel_codec"),
        codec_quality: get_u32(m, "channel_codec_quality"),
        needed_talk_power: get_i32(m, "channel_needed_talk_power"),
    }
}

pub fn map_client(m: &RecordMap) -> OnlineClient {
    let server_groups = get_str(m, "client_servergroups")
        .split(',')
        .filter_map(|s| s.trim().parse::<u32>().ok())
        .collect();
    OnlineClient {
        clid: get_u32(m, "clid"),
        cid: get_u32(m, "cid"),
        cldbid: get_u32(m, "client_database_id"),
        uid: get_str(m, "client_unique_identifier"),
        nickname: get_str(m, "client_nickname"),
        client_type: get_u32(m, "client_type") as u8,
        input_muted: get_bool(m, "client_input_muted"),
        output_muted: get_bool(m, "client_output_muted"),
        away: get_bool(m, "client_away"),
        away_message: get_str(m, "client_away_message"),
        is_talking: get_bool(m, "client_flag_talking"),
        server_groups,
        channel_group_id: get_u32(m, "client_channel_group_id"),
        idle_time_ms: get_u64(m, "client_idle_time"),
        connected_time_ms: get_u64(m, "client_connected_time"),
        ip: get_str(m, "connection_client_ip"),
        country: get_str(m, "client_country"),
        version: get_str(m, "client_version"),
        platform: get_str(m, "client_platform"),
        ping_ms: get_f64(m, "client_ping"),
    }
}

pub fn map_server_group(m: &RecordMap) -> ServerGroup {
    ServerGroup {
        sgid: get_u32(m, "sgid"),
        name: get_str(m, "name"),
        group_type: get_u32(m, "type") as u8,
        icon_id: get_i64(m, "iconid"),
        savedb: get_bool(m, "savedb"),
        sortid: get_i32(m, "sortid"),
        namemode: get_u32(m, "namemode") as u8,
    }
}

pub fn map_channel_group(m: &RecordMap) -> ChannelGroup {
    ChannelGroup {
        cgid: get_u32(m, "cgid"),
        name: get_str(m, "name"),
        group_type: get_u32(m, "type") as u8,
        icon_id: get_i64(m, "iconid"),
        savedb: get_bool(m, "savedb"),
        sortid: get_i32(m, "sortid"),
        namemode: get_u32(m, "namemode") as u8,
    }
}

pub fn map_group_member(m: &RecordMap) -> GroupMember {
    GroupMember {
        cldbid: get_u32(m, "cldbid"),
        nickname: get_str(m, "client_nickname"),
        uid: get_str(m, "client_unique_identifier"),
    }
}

pub fn map_permission(m: &RecordMap) -> PermissionEntry {
    let permid = get_u32(m, "permid");
    let permsid = get_str(m, "permsid");
    PermissionEntry {
        permid,
        permname: if permsid.is_empty() {
            format!("perm_{permid}")
        } else {
            permsid
        },
        permvalue: get_i64(m, "permvalue"),
        permnegated: get_bool(m, "permnegated"),
        permskip: get_bool(m, "permskip"),
    }
}

pub fn map_privilege_key(m: &RecordMap) -> PrivilegeKey {
    PrivilegeKey {
        token: get_str(m, "token"),
        token_type: get_u32(m, "token_type") as u8,
        token_id1: get_u32(m, "token_id1"),
        token_id2: get_u32(m, "token_id2"),
        created: get_i64(m, "token_created"),
        description: get_str(m, "token_description"),
    }
}

pub fn map_ban_rule(m: &RecordMap) -> BanRule {
    BanRule {
        banid: get_u32(m, "banid"),
        ip: get_str(m, "ip"),
        name: get_str(m, "name"),
        uid: get_str(m, "uid"),
        created: get_i64(m, "created"),
        duration_secs: get_i64(m, "duration"),
        invoker_name: get_str(m, "invokername"),
        invoker_uid: get_str(m, "invokeruid"),
        reason: get_str(m, "reason"),
        enforcements: get_u64(m, "enforcements"),
    }
}

pub fn map_db_client(m: &RecordMap) -> DbClient {
    DbClient {
        cldbid: get_u32(m, "cldbid"),
        uid: get_str(m, "client_unique_identifier"),
        nickname: get_str(m, "client_nickname"),
        created: get_i64(m, "client_created"),
        last_connected: get_i64(m, "client_lastconnected"),
        total_connections: get_u32(m, "client_totalconnections"),
        last_ip: get_str(m, "client_lastip"),
        description: get_str(m, "client_description"),
    }
}

pub fn map_complaint(m: &RecordMap) -> ComplaintEntry {
    ComplaintEntry {
        tcldbid: get_u32(m, "tcldbid"),
        tname: get_str(m, "tname"),
        fcldbid: get_u32(m, "fcldbid"),
        fname: get_str(m, "fname"),
        message: get_str(m, "message"),
        timestamp: get_i64(m, "timestamp"),
    }
}

pub fn map_log_entry(m: &RecordMap) -> TsLogEntry {
    let raw = get_str(m, "l");
    // Format: "2026-11-24 12:00:00.000000|INFO    |VirtualServer |1  |..."
    let parts: Vec<&str> = raw.splitn(5, '|').collect();
    if parts.len() == 5 {
        TsLogEntry {
            line: raw.clone(),
            timestamp: parts[0].trim().to_string(),
            level: parts[1].trim().to_string(),
            channel: parts[2].trim().to_string(),
            message: parts[4].trim().to_string(),
        }
    } else {
        TsLogEntry {
            line: raw.clone(),
            timestamp: String::new(),
            level: "INFO".to_string(),
            channel: "Server".to_string(),
            message: raw,
        }
    }
}
