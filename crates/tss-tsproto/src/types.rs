use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TsProtoError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP/WebQuery transport error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("SSH transport error: {0}")]
    Ssh(String),

    #[error("Connection closed by TeamSpeak server")]
    ConnectionClosed,

    #[error("Operation timed out waiting for TeamSpeak response")]
    Timeout,

    #[error("Protocol parse error: {0}")]
    Parse(String),

    #[error("TeamSpeak ServerQuery error id={id}: {msg}")]
    QueryError {
        id: u32,
        msg: String,
        extra_msg: Option<String>,
    },

    #[error("Unsupported operation for protocol: {0}")]
    Unsupported(String),
}

pub type Result<T> = std::result::Result<T, TsProtoError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolKind {
    RawTcp,
    Ssh,
    WebQuery,
}

impl ProtocolKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProtocolKind::RawTcp => "raw_tcp",
            ProtocolKind::Ssh => "ssh",
            ProtocolKind::WebQuery => "web_query",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "raw_tcp" | "raw" | "tcp" => Some(Self::RawTcp),
            "ssh" => Some(Self::Ssh),
            "web_query" | "webquery" | "http" | "https" => Some(Self::WebQuery),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsConnectionConfig {
    pub protocol: ProtocolKind,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub secret: String,
    pub virtual_server_id: u32,
    pub tls_verify: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualServerSummary {
    pub id: u32,
    pub port: u16,
    pub status: String,
    pub clients_online: u32,
    pub query_clients_online: u32,
    pub max_clients: u32,
    pub uptime_secs: u64,
    pub name: String,
    pub autostart: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfoSnapshot {
    pub virtual_server_id: u32,
    pub unique_identifier: String,
    pub name: String,
    pub welcome_message: String,
    pub platform: String,
    pub version: String,
    pub max_clients: u32,
    pub clients_online: u32,
    pub query_clients_online: u32,
    pub channels_online: u32,
    pub uptime_secs: u64,
    pub total_ping_ms: f64,
    pub total_packetloss_pct: f64,
    pub bw_in_bps: u64,
    pub bw_out_bps: u64,
    pub password_flag: bool,
    pub hostbanner_url: String,
    pub hostbanner_gfx_url: String,
    pub hostbutton_tooltip: String,
    pub hostbutton_url: String,
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EditServerInfoParams {
    pub name: Option<String>,
    pub welcome_message: Option<String>,
    pub max_clients: Option<u32>,
    pub password: Option<String>,
    pub hostbanner_url: Option<String>,
    pub hostbanner_gfx_url: Option<String>,
    pub hostbutton_tooltip: Option<String>,
    pub hostbutton_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelNode {
    pub cid: u32,
    pub pid: u32,
    pub channel_order: u32,
    pub channel_name: String,
    pub channel_topic: String,
    pub total_clients: u32,
    pub max_clients: i32,
    pub is_permanent: bool,
    pub is_semi_permanent: bool,
    pub is_default: bool,
    pub has_password: bool,
    pub codec: u32,
    pub codec_quality: u32,
    pub needed_talk_power: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateChannelParams {
    pub name: String,
    pub parent_id: Option<u32>,
    pub topic: Option<String>,
    pub description: Option<String>,
    pub password: Option<String>,
    pub max_clients: Option<i32>,
    pub is_permanent: Option<bool>,
    pub is_semi_permanent: Option<bool>,
    pub is_default: Option<bool>,
    pub codec: Option<u32>,
    pub codec_quality: Option<u32>,
    pub needed_talk_power: Option<i32>,
    pub order: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EditChannelParams {
    pub name: Option<String>,
    pub topic: Option<String>,
    pub description: Option<String>,
    pub password: Option<String>,
    pub max_clients: Option<i32>,
    pub is_permanent: Option<bool>,
    pub is_semi_permanent: Option<bool>,
    pub is_default: Option<bool>,
    pub codec: Option<u32>,
    pub codec_quality: Option<u32>,
    pub needed_talk_power: Option<i32>,
    pub order: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnlineClient {
    pub clid: u32,
    pub cid: u32,
    pub cldbid: u32,
    pub uid: String,
    pub nickname: String,
    /// 0 = normal voice client, 1 = ServerQuery client
    pub client_type: u8,
    pub input_muted: bool,
    pub output_muted: bool,
    pub away: bool,
    pub away_message: String,
    pub is_talking: bool,
    pub server_groups: Vec<u32>,
    pub channel_group_id: u32,
    pub idle_time_ms: u64,
    pub connected_time_ms: u64,
    pub ip: String,
    pub country: String,
    pub version: String,
    pub platform: String,
    pub ping_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerGroup {
    pub sgid: u32,
    pub name: String,
    /// 0 = template, 1 = normal, 2 = ServerQuery
    pub group_type: u8,
    pub icon_id: i64,
    pub savedb: bool,
    pub sortid: i32,
    pub namemode: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelGroup {
    pub cgid: u32,
    pub name: String,
    pub group_type: u8,
    pub icon_id: i64,
    pub savedb: bool,
    pub sortid: i32,
    pub namemode: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMember {
    pub cldbid: u32,
    pub nickname: String,
    pub uid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionEntry {
    pub permid: u32,
    pub permname: String,
    pub permvalue: i64,
    pub permnegated: bool,
    pub permskip: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetPermissionParams {
    pub permname: String,
    pub permvalue: i64,
    pub permnegated: bool,
    pub permskip: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivilegeKey {
    pub token: String,
    /// 0 = server group, 1 = channel group
    pub token_type: u8,
    pub token_id1: u32,
    pub token_id2: u32,
    pub created: i64,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePrivilegeKeyParams {
    pub token_type: u8,
    pub group_id: u32,
    pub channel_id: Option<u32>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BanRule {
    pub banid: u32,
    pub ip: String,
    pub name: String,
    pub uid: String,
    pub created: i64,
    pub duration_secs: i64,
    pub invoker_name: String,
    pub invoker_uid: String,
    pub reason: String,
    pub enforcements: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddBanParams {
    pub ip: Option<String>,
    pub name: Option<String>,
    pub uid: Option<String>,
    pub duration_secs: Option<i64>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbClient {
    pub cldbid: u32,
    pub uid: String,
    pub nickname: String,
    pub created: i64,
    pub last_connected: i64,
    pub total_connections: u32,
    pub last_ip: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplaintEntry {
    pub tcldbid: u32,
    pub tname: String,
    pub fcldbid: u32,
    pub fname: String,
    pub message: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsLogEntry {
    pub line: String,
    pub timestamp: String,
    pub level: String,
    pub channel: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsSnapshotData {
    pub version: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TsNotifyEvent {
    ClientEntered {
        clid: u32,
        ctid: u32,
        nickname: String,
        uid: String,
    },
    ClientLeft {
        clid: u32,
        cfid: u32,
        reason_id: u32,
        reason_msg: String,
    },
    ClientMoved {
        clid: u32,
        ctid: u32,
        reason_id: u32,
        invoker_name: Option<String>,
    },
    ChannelCreated {
        cid: u32,
        pid: u32,
        name: String,
    },
    ChannelEdited {
        cid: u32,
        invoker_name: Option<String>,
    },
    ChannelDeleted {
        cid: u32,
    },
    ChannelMoved {
        cid: u32,
        cpid: u32,
        order: u32,
    },
    TextMessage {
        target_mode: u8,
        invoker_id: u32,
        invoker_name: String,
        msg: String,
    },
    ServerEdited,
}
