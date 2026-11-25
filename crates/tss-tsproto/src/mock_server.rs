use crate::codec::{escape_ts, parse_records, trim_ts_line};
use crate::types::{
    BanRule, ChannelGroup, ChannelNode, ComplaintEntry, DbClient, OnlineClient, PermissionEntry,
    PrivilegeKey, ServerGroup,
};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, watch, RwLock};
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MockVirtualServer {
    id: u32,
    port: u16,
    status: String,
    name: String,
    welcome_message: String,
    max_clients: u32,
    uptime_secs: u64,
    password: String,
    hostbanner_url: String,
    hostbanner_gfx_url: String,
    hostbutton_tooltip: String,
    hostbutton_url: String,
    avg_ping_ms: f64,
    packet_loss_ratio: f64,
    bw_in_bps: u64,
    bw_out_bps: u64,
    channels: Vec<ChannelNode>,
    clients: Vec<OnlineClient>,
    server_groups: Vec<ServerGroup>,
    channel_groups: Vec<ChannelGroup>,
    group_members: HashMap<u32, Vec<u32>>,
    group_perms: HashMap<u32, Vec<PermissionEntry>>,
    privilege_keys: Vec<PrivilegeKey>,
    bans: Vec<BanRule>,
    db_clients: Vec<DbClient>,
    complaints: Vec<ComplaintEntry>,
    logs: Vec<String>,
    next_cid: u32,
    next_sgid: u32,
    next_banid: u32,
}

struct MockWorld {
    servers: BTreeMap<u32, MockVirtualServer>,
    next_sid: u32,
    username: String,
    password: String,
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn default_permissions(is_admin: bool, is_mod: bool) -> Vec<PermissionEntry> {
    let mut list = vec![
        ("i_channel_join_power", if is_admin { 75 } else if is_mod { 50 } else { 25 }),
        ("i_channel_needed_join_power", 0),
        ("i_client_talk_power", if is_admin { 75 } else if is_mod { 50 } else { 20 }),
        ("i_client_kick_from_server_power", if is_admin { 75 } else if is_mod { 50 } else { 0 }),
        ("i_client_needed_kick_from_server_power", if is_admin { 75 } else if is_mod { 50 } else { 20 }),
        ("i_client_kick_from_channel_power", if is_admin { 75 } else if is_mod { 50 } else { 0 }),
        ("i_client_move_power", if is_admin { 75 } else if is_mod { 50 } else { 0 }),
        ("i_client_needed_move_power", if is_admin { 75 } else if is_mod { 50 } else { 20 }),
        ("i_client_ban_power", if is_admin { 75 } else if is_mod { 50 } else { 0 }),
        ("i_client_ban_max_bantime", if is_admin { -1 } else if is_mod { 86400 } else { 0 }),
        ("i_group_member_add_power", if is_admin { 75 } else { 0 }),
        ("i_group_member_remove_power", if is_admin { 75 } else { 0 }),
        ("i_group_modify_power", if is_admin { 75 } else { 0 }),
        ("b_virtualserver_info_view", 1),
        ("b_virtualserver_channel_list", 1),
        ("b_virtualserver_client_list", 1),
        ("b_virtualserver_modify_name", if is_admin { 1 } else { 0 }),
        ("b_virtualserver_modify_maxclients", if is_admin { 1 } else { 0 }),
        ("b_virtualserver_snapshot_create", if is_admin { 1 } else { 0 }),
        ("b_virtualserver_snapshot_deploy", if is_admin { 1 } else { 0 }),
        ("b_channel_create_permanent", if is_admin || is_mod { 1 } else { 0 }),
        ("b_channel_delete_permanent", if is_admin { 1 } else { 0 }),
        ("b_client_ban_list", if is_admin || is_mod { 1 } else { 0 }),
        ("b_client_ban_create", if is_admin || is_mod { 1 } else { 0 }),
    ];
    list.drain(..)
        .enumerate()
        .map(|(idx, (name, val))| PermissionEntry {
            permid: (idx + 1) as u32,
            permname: name.to_string(),
            permvalue: val,
            permnegated: false,
            permskip: false,
        })
        .collect()
}

fn seed_world() -> MockWorld {
    let now = unix_now();
    let db_clients = vec![
        DbClient { cldbid: 1, uid: "b2x1QWRtaW5VaWRWMQ==".into(), nickname: "Commander_Kael".into(), created: now - 86400 * 90, last_connected: now - 120, total_connections: 142, last_ip: "10.24.8.11".into(), description: "Server Owner".into() },
        DbClient { cldbid: 2, uid: "a1Z2TW9kZXJhdG9yVTI=".into(), nickname: "Valkyrie_Mod".into(), created: now - 86400 * 60, last_connected: now - 300, total_connections: 88, last_ip: "10.24.8.22".into(), description: "Senior Mod".into() },
        DbClient { cldbid: 3, uid: "c3J4UmFpZExlYWRlclYz".into(), nickname: "Tactician_Lin".into(), created: now - 86400 * 45, last_connected: now - 600, total_connections: 63, last_ip: "10.24.9.14".into(), description: "Alpha Squad Lead".into() },
        DbClient { cldbid: 4, uid: "ZDR2QWZrbG91bmdlVjQ=".into(), nickname: "SilentFox".into(), created: now - 86400 * 30, last_connected: now - 900, total_connections: 29, last_ip: "10.24.9.45".into(), description: "Idle test client".into() },
        DbClient { cldbid: 5, uid: "ZTVzV2VzdFNjb3V0VjU=".into(), nickname: "Echo_7".into(), created: now - 86400 * 25, last_connected: now - 400, total_connections: 51, last_ip: "10.24.10.5".into(), description: "".into() },
        DbClient { cldbid: 6, uid: "ZjZ0U25pcGVyTmlnaHQ2".into(), nickname: "NightOwl".into(), created: now - 86400 * 20, last_connected: now - 250, total_connections: 37, last_ip: "10.24.10.9".into(), description: "".into() },
        DbClient { cldbid: 7, uid: "ZzdoTUVESUNfU1FVQUQ3".into(), nickname: "Medic_Sora".into(), created: now - 86400 * 14, last_connected: now - 180, total_connections: 22, last_ip: "10.24.11.3".into(), description: "".into() },
        DbClient { cldbid: 8, uid: "aDhpTmlnaHRHdWVzdDg=".into(), nickname: "Guest_994".into(), created: now - 86400 * 2, last_connected: now - 60, total_connections: 3, last_ip: "10.24.15.77".into(), description: "".into() },
        DbClient { cldbid: 9, uid: "aTlqT2ZmbGluZVVzZXI5".into(), nickname: "GhostRecon".into(), created: now - 86400 * 40, last_connected: now - 86400 * 3, total_connections: 19, last_ip: "198.51.100.42".into(), description: "Former raid member".into() },
        DbClient { cldbid: 10, uid: "ajBrT2ZmbGluZVVzZXIxMA==".into(), nickname: "ZeroCool".into(), created: now - 86400 * 18, last_connected: now - 86400, total_connections: 11, last_ip: "203.0.113.88".into(), description: "".into() },
    ];

    let clients_s1 = vec![
        OnlineClient { clid: 1, cid: 2, cldbid: 1, uid: db_clients[0].uid.clone(), nickname: "Commander_Kael".into(), client_type: 0, input_muted: false, output_muted: false, away: false, away_message: "".into(), is_talking: true, server_groups: vec![6, 9], channel_group_id: 5, idle_time_ms: 1200, connected_time_ms: 5400_000, ip: "10.24.8.11".into(), country: "CN".into(), version: "3.6.2 [Build: 1695203293]".into(), platform: "Windows".into(), ping_ms: 18.4 },
        OnlineClient { clid: 2, cid: 2, cldbid: 2, uid: db_clients[1].uid.clone(), nickname: "Valkyrie_Mod".into(), client_type: 0, input_muted: false, output_muted: false, away: false, away_message: "".into(), is_talking: false, server_groups: vec![7, 9], channel_group_id: 6, idle_time_ms: 9500, connected_time_ms: 4200_000, ip: "10.24.8.22".into(), country: "CN".into(), version: "6.0.0-beta".into(), platform: "Linux".into(), ping_ms: 24.1 },
        OnlineClient { clid: 3, cid: 3, cldbid: 3, uid: db_clients[2].uid.clone(), nickname: "Tactician_Lin".into(), client_type: 0, input_muted: false, output_muted: false, away: false, away_message: "".into(), is_talking: true, server_groups: vec![8, 9], channel_group_id: 5, idle_time_ms: 450, connected_time_ms: 3600_000, ip: "10.24.9.14".into(), country: "SG".into(), version: "3.6.2 [Build: 1695203293]".into(), platform: "Windows".into(), ping_ms: 31.2 },
        OnlineClient { clid: 4, cid: 3, cldbid: 5, uid: db_clients[4].uid.clone(), nickname: "Echo_7".into(), client_type: 0, input_muted: true, output_muted: false, away: false, away_message: "".into(), is_talking: false, server_groups: vec![9], channel_group_id: 7, idle_time_ms: 21000, connected_time_ms: 2800_000, ip: "10.24.10.5".into(), country: "JP".into(), version: "3.6.2".into(), platform: "macOS".into(), ping_ms: 42.7 },
        OnlineClient { clid: 5, cid: 4, cldbid: 6, uid: db_clients[5].uid.clone(), nickname: "NightOwl".into(), client_type: 0, input_muted: false, output_muted: false, away: false, away_message: "".into(), is_talking: false, server_groups: vec![9], channel_group_id: 7, idle_time_ms: 15000, connected_time_ms: 1900_000, ip: "10.24.10.9".into(), country: "CN".into(), version: "3.6.2".into(), platform: "Windows".into(), ping_ms: 19.6 },
        OnlineClient { clid: 6, cid: 5, cldbid: 7, uid: db_clients[6].uid.clone(), nickname: "Medic_Sora".into(), client_type: 0, input_muted: false, output_muted: false, away: false, away_message: "".into(), is_talking: false, server_groups: vec![9], channel_group_id: 7, idle_time_ms: 8200, connected_time_ms: 1500_000, ip: "10.24.11.3".into(), country: "KR".into(), version: "6.0.0-beta".into(), platform: "Windows".into(), ping_ms: 35.0 },
        OnlineClient { clid: 7, cid: 1, cldbid: 4, uid: db_clients[3].uid.clone(), nickname: "SilentFox".into(), client_type: 0, input_muted: true, output_muted: true, away: true, away_message: "Stepped out 10m".into(), is_talking: false, server_groups: vec![9], channel_group_id: 7, idle_time_ms: 420_000, connected_time_ms: 3900_000, ip: "10.24.9.45".into(), country: "CN".into(), version: "3.6.2".into(), platform: "Windows".into(), ping_ms: 22.8 },
        OnlineClient { clid: 8, cid: 1, cldbid: 8, uid: db_clients[7].uid.clone(), nickname: "Guest_994".into(), client_type: 0, input_muted: false, output_muted: false, away: false, away_message: "".into(), is_talking: false, server_groups: vec![10], channel_group_id: 7, idle_time_ms: 42000, connected_time_ms: 600_000, ip: "10.24.15.77".into(), country: "TW".into(), version: "3.6.2".into(), platform: "Windows".into(), ping_ms: 28.5 },
    ];

    let channels_s1 = vec![
        ChannelNode { cid: 1, pid: 0, channel_order: 0, channel_name: "Lobby · 大厅".into(), channel_topic: "Welcome to Apex Tactical Comms".into(), total_clients: 2, max_clients: -1, is_permanent: true, is_semi_permanent: false, is_default: true, has_password: false, codec: 4, codec_quality: 6, needed_talk_power: 0 },
        ChannelNode { cid: 2, pid: 0, channel_order: 1, channel_name: "Command Room · 指挥室".into(), channel_topic: "Ops Briefing & Officers".into(), total_clients: 2, max_clients: 16, is_permanent: true, is_semi_permanent: false, is_default: false, has_password: true, codec: 5, codec_quality: 8, needed_talk_power: 25 },
        ChannelNode { cid: 3, pid: 2, channel_order: 0, channel_name: "Squad Alpha".into(), channel_topic: "Primary Assault".into(), total_clients: 2, max_clients: 12, is_permanent: true, is_semi_permanent: false, is_default: false, has_password: false, codec: 4, codec_quality: 7, needed_talk_power: 0 },
        ChannelNode { cid: 4, pid: 2, channel_order: 3, channel_name: "Squad Bravo".into(), channel_topic: "Support & Recon".into(), total_clients: 1, max_clients: 12, is_permanent: true, is_semi_permanent: false, is_default: false, has_password: false, codec: 4, codec_quality: 7, needed_talk_power: 0 },
        ChannelNode { cid: 5, pid: 0, channel_order: 2, channel_name: "Raid Room · 攻坚频道".into(), channel_topic: "High-bitrate Opus Music/Voice".into(), total_clients: 1, max_clients: 40, is_permanent: true, is_semi_permanent: false, is_default: false, has_password: false, codec: 4, codec_quality: 10, needed_talk_power: 50 },
        ChannelNode { cid: 6, pid: 0, channel_order: 5, channel_name: "AFK Lounge · 挂机休息区".into(), channel_topic: "Auto-moved idle users".into(), total_clients: 0, max_clients: -1, is_permanent: true, is_semi_permanent: false, is_default: false, has_password: false, codec: 4, codec_quality: 4, needed_talk_power: 100 },
    ];

    let server_groups = vec![
        ServerGroup { sgid: 6, name: "Server Admin".into(), group_type: 1, icon_id: 300, savedb: true, sortid: 1, namemode: 0 },
        ServerGroup { sgid: 7, name: "Moderator".into(), group_type: 1, icon_id: 200, savedb: true, sortid: 2, namemode: 0 },
        ServerGroup { sgid: 8, name: "Raid Leader".into(), group_type: 1, icon_id: 150, savedb: true, sortid: 3, namemode: 0 },
        ServerGroup { sgid: 9, name: "Verified Member".into(), group_type: 1, icon_id: 100, savedb: true, sortid: 4, namemode: 0 },
        ServerGroup { sgid: 10, name: "Guest".into(), group_type: 1, icon_id: 0, savedb: false, sortid: 5, namemode: 0 },
    ];

    let mut group_members = HashMap::new();
    group_members.insert(6, vec![1]);
    group_members.insert(7, vec![2]);
    group_members.insert(8, vec![3]);
    group_members.insert(9, vec![1, 2, 3, 4, 5, 6, 7]);
    group_members.insert(10, vec![8]);

    let mut group_perms = HashMap::new();
    group_perms.insert(6, default_permissions(true, true));
    group_perms.insert(7, default_permissions(false, true));
    group_perms.insert(8, default_permissions(false, false));
    group_perms.insert(9, default_permissions(false, false));
    group_perms.insert(10, default_permissions(false, false));

    let s1 = MockVirtualServer {
        id: 1,
        port: 9987,
        status: "online".into(),
        name: "Apex Tactical Comms [CN-East]".into(),
        welcome_message: "Welcome to Apex Tactical TeamSpeak! Observe channel discipline.".into(),
        max_clients: 64,
        uptime_secs: 418_920,
        password: "".into(),
        hostbanner_url: "https://teamspeak.example.local".into(),
        hostbanner_gfx_url: "".into(),
        hostbutton_tooltip: "TSS Manager Portal".into(),
        hostbutton_url: "https://teamspeak.example.local/portal".into(),
        avg_ping_ms: 26.8,
        packet_loss_ratio: 0.0042,
        bw_in_bps: 48_200,
        bw_out_bps: 124_600,
        channels: channels_s1,
        clients: clients_s1,
        server_groups: server_groups.clone(),
        channel_groups: vec![
            ChannelGroup { cgid: 5, name: "Channel Admin".into(), group_type: 1, icon_id: 100, savedb: true, sortid: 1, namemode: 0 },
            ChannelGroup { cgid: 6, name: "Operator".into(), group_type: 1, icon_id: 50, savedb: true, sortid: 2, namemode: 0 },
            ChannelGroup { cgid: 7, name: "Voice".into(), group_type: 1, icon_id: 0, savedb: true, sortid: 3, namemode: 0 },
        ],
        group_members,
        group_perms,
        privilege_keys: vec![
            PrivilegeKey { token: "eA9P-TACTICAL-MOD-8841-XQ".into(), token_type: 0, token_id1: 7, token_id2: 0, created: now - 86400 * 3, description: "Onboard shift moderator".into() },
            PrivilegeKey { token: "vM4R-VERIFIED-9910-KL".into(), token_type: 0, token_id1: 9, token_id2: 0, created: now - 86400, description: "New squad recruit key".into() },
        ],
        bans: vec![
            BanRule { banid: 1, ip: "198.51.100.99".into(), name: "SpamBot.*".into(), uid: "".into(), created: now - 86400 * 5, duration_secs: 0, invoker_name: "Commander_Kael".into(), invoker_uid: db_clients[0].uid.clone(), reason: "Automated mic spam".into(), enforcements: 14 },
            BanRule { banid: 2, ip: "".into(), name: "".into(), uid: "badUidExample1234567890=".into(), created: now - 3600 * 4, duration_secs: 86400, invoker_name: "Valkyrie_Mod".into(), invoker_uid: db_clients[1].uid.clone(), reason: "Channel hopping harassment".into(), enforcements: 2 },
        ],
        db_clients,
        complaints: vec![
            ComplaintEntry { tcldbid: 8, tname: "Guest_994".into(), fcldbid: 5, fname: "Echo_7".into(), message: "Hot mic background noise in Lobby".into(), timestamp: now - 1800 },
        ],
        logs: vec![
            "2026-11-24 15:40:12.112000|INFO    |VirtualServerBase|1  |listening on 0.0.0.0:9987".into(),
            "2026-11-24 15:48:04.440120|INFO    |VirtualServerBase|1  |client connected 'Guest_994'(id:8) from 10.24.15.77".into(),
            "2026-11-24 15:52:19.005210|WARNING |VirtualServerBase|1  |complaint filed against 'Guest_994' by 'Echo_7'".into(),
        ],
        next_cid: 7,
        next_sgid: 11,
        next_banid: 3,
    };

    let s2 = MockVirtualServer {
        id: 2,
        port: 9988,
        status: "offline".into(),
        name: "Dev Staging Voice".into(),
        welcome_message: "Staging environment".into(),
        max_clients: 32,
        uptime_secs: 0,
        password: "stage".into(),
        hostbanner_url: "".into(),
        hostbanner_gfx_url: "".into(),
        hostbutton_tooltip: "".into(),
        hostbutton_url: "".into(),
        avg_ping_ms: 0.0,
        packet_loss_ratio: 0.0,
        bw_in_bps: 0,
        bw_out_bps: 0,
        channels: vec![
            ChannelNode { cid: 1, pid: 0, channel_order: 0, channel_name: "Staging Lobby".into(), channel_topic: "".into(), total_clients: 0, max_clients: -1, is_permanent: true, is_semi_permanent: false, is_default: true, has_password: false, codec: 4, codec_quality: 6, needed_talk_power: 0 },
        ],
        clients: vec![],
        server_groups,
        channel_groups: vec![
            ChannelGroup { cgid: 5, name: "Channel Admin".into(), group_type: 1, icon_id: 100, savedb: true, sortid: 1, namemode: 0 },
        ],
        group_members: HashMap::new(),
        group_perms: HashMap::new(),
        privilege_keys: vec![],
        bans: vec![],
        db_clients: vec![],
        complaints: vec![],
        logs: vec!["2026-11-24 12:00:00.000000|INFO    |VirtualServerBase|2  |virtualserver stopped".into()],
        next_cid: 2,
        next_sgid: 11,
        next_banid: 1,
    };

    let mut servers = BTreeMap::new();
    servers.insert(1, s1);
    servers.insert(2, s2);

    MockWorld {
        servers,
        next_sid: 3,
        username: "serveradmin".into(),
        password: "sandbox-secret".into(),
    }
}

pub struct MockTsServer {
    addr: SocketAddr,
    username: String,
    password: String,
    shutdown_tx: watch::Sender<bool>,
}

impl MockTsServer {
    pub async fn start() -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let world = Arc::new(RwLock::new(seed_world()));
        let (notify_tx, _) = broadcast::channel::<(u32, String)>(256);
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

        let username = "serveradmin".to_string();
        let password = "sandbox-secret".to_string();

        // Background simulator task (updates idle time and telemetry metrics)
        {
            let world = world.clone();
            let mut rx = shutdown_rx.clone();
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(3));
                tick.tick().await;
                let mut step: u64 = 0;
                loop {
                    tokio::select! {
                        _ = rx.changed() => {
                            if *rx.borrow() { break; }
                        }
                        _ = tick.tick() => {
                            step += 1;
                            let mut w = world.write().await;
                            for srv in w.servers.values_mut() {
                                if srv.status != "online" { continue; }
                                srv.uptime_secs += 3;
                                let wave = ((step as f64) * 0.45).sin();
                                srv.avg_ping_ms = (26.0 + wave * 3.5).max(12.0);
                                srv.packet_loss_ratio = (0.0038 + wave.abs() * 0.0025).min(0.05);
                                srv.bw_in_bps = (44_000.0 + wave * 8_000.0).max(10_000.0) as u64;
                                srv.bw_out_bps = (118_000.0 + wave * 22_000.0).max(25_000.0) as u64;
                                for cl in &mut srv.clients {
                                    cl.connected_time_ms += 3000;
                                    if cl.is_talking {
                                        cl.idle_time_ms = 0;
                                    } else {
                                        cl.idle_time_ms += 3000;
                                    }
                                }
                            }
                        }
                    }
                }
            });
        }

        // TCP Accept loop
        {
            let world = world.clone();
            let notify_tx = notify_tx.clone();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = shutdown_rx.changed() => {
                            if *shutdown_rx.borrow() { break; }
                        }
                        res = listener.accept() => {
                            match res {
                                Ok((socket, _)) => {
                                    let w = world.clone();
                                    let tx = notify_tx.clone();
                                    tokio::spawn(handle_conn(socket, w, tx));
                                }
                                Err(e) => {
                                    debug!("MockTsServer accept err: {e}");
                                    break;
                                }
                            }
                        }
                    }
                }
            });
        }

        Ok(Self {
            addr,
            username,
            password,
            shutdown_tx,
        })
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    pub fn host(&self) -> &'static str {
        "127.0.0.1"
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn password(&self) -> &str {
        &self.password
    }

    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }
}

fn format_kv(items: &[(&str, String)]) -> String {
    items
        .iter()
        .map(|(k, v)| format!("{k}={}", escape_ts(v)))
        .collect::<Vec<_>>()
        .join(" ")
}

async fn write_ts_line(writer: &mut tokio::net::tcp::OwnedWriteHalf, line: &str) -> std::io::Result<()> {
    writer.write_all(line.as_bytes()).await?;
    writer.write_all(b"\n\r").await?;
    writer.flush().await
}

fn recalc_channel_counts(srv: &mut MockVirtualServer) {
    for ch in &mut srv.channels {
        ch.total_clients = srv.clients.iter().filter(|c| c.cid == ch.cid).count() as u32;
    }
}

async fn handle_conn(
    socket: TcpStream,
    world: Arc<RwLock<MockWorld>>,
    notify_tx: broadcast::Sender<(u32, String)>,
) {
    let (rd, mut wr) = socket.into_split();
    let mut reader = BufReader::new(rd);

    if write_ts_line(&mut wr, "TS3").await.is_err() {
        return;
    }
    if write_ts_line(
        &mut wr,
        "Welcome to the TeamSpeak 3 ServerQuery interface, type \"help\" for a list of commands.",
    )
    .await
    .is_err()
    {
        return;
    }

    let mut authed = false;
    let mut active_sid: u32 = 1;
    let mut notify_rx: Option<broadcast::Receiver<(u32, String)>> = None;
    let mut buf = Vec::with_capacity(512);

    loop {
        buf.clear();
        tokio::select! {
            n_res = reader.read_until(b'\n', &mut buf) => {
                match n_res {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let line = trim_ts_line(&String::from_utf8_lossy(&buf)).trim().to_string();
                        if line.is_empty() {
                            continue;
                        }
                        let (cmd, rest) = match line.split_once(' ') {
                            Some((c, r)) => (c, r),
                            None => (line.as_str(), ""),
                        };
                        let params = parse_records(rest).into_iter().next().unwrap_or_default();

                        if cmd == "quit" {
                            let _ = write_ts_line(&mut wr, "error id=0 msg=ok").await;
                            break;
                        }

                        if cmd == "login" {
                            let u = params.get("client_login_name").map(String::as_str).unwrap_or("");
                            let p = params.get("client_login_password").map(String::as_str).unwrap_or("");
                            let w = world.read().await;
                            if u == w.username && p == w.password {
                                authed = true;
                                let _ = write_ts_line(&mut wr, "error id=0 msg=ok").await;
                            } else {
                                let _ = write_ts_line(&mut wr, "error id=520 msg=invalid\\sloginname\\sor\\spassword").await;
                            }
                            continue;
                        }

                        if cmd == "version" {
                            let _ = write_ts_line(&mut wr, "version=3.13.7-sandbox build=1700000000 platform=Linux").await;
                            let _ = write_ts_line(&mut wr, "error id=0 msg=ok").await;
                            continue;
                        }

                        if !authed {
                            let _ = write_ts_line(&mut wr, "error id=2568 msg=insufficient\\sclient\\spermissions").await;
                            continue;
                        }

                        if cmd == "servernotifyregister" {
                            if notify_rx.is_none() {
                                notify_rx = Some(notify_tx.subscribe());
                            }
                            let _ = write_ts_line(&mut wr, "error id=0 msg=ok").await;
                            continue;
                        }

                        let reply = dispatch_command(cmd, &params, &world, &notify_tx, &mut active_sid).await;
                        for rline in reply {
                            if write_ts_line(&mut wr, &rline).await.is_err() {
                                return;
                            }
                        }
                    }
                }
            }
            evt = async {
                if let Some(rx) = notify_rx.as_mut() {
                    rx.recv().await.ok()
                } else {
                    std::future::pending::<Option<(u32, String)>>().await
                }
            } => {
                if let Some((sid, msg)) = evt {
                    if sid == active_sid {
                        if write_ts_line(&mut wr, &msg).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
    }
}

async fn dispatch_command(
    cmd: &str,
    params: &BTreeMap<String, String>,
    world: &Arc<RwLock<MockWorld>>,
    notify_tx: &broadcast::Sender<(u32, String)>,
    active_sid: &mut u32,
) -> Vec<String> {
    let get_u32 = |k: &str| params.get(k).and_then(|v| v.parse::<u32>().ok());
    let get_i32 = |k: &str| params.get(k).and_then(|v| v.parse::<i32>().ok());
    let get_i64 = |k: &str| params.get(k).and_then(|v| v.parse::<i64>().ok());

    match cmd {
        "logout" => vec!["error id=0 msg=ok".into()],
        "whoami" => vec![
            format!("virtualserver_id={active_sid} virtualserver_status=online client_id=99 client_nickname=serveradmin"),
            "error id=0 msg=ok".into(),
        ],
        "use" => {
            let sid = get_u32("sid").unwrap_or(1);
            let w = world.read().await;
            if let Some(srv) = w.servers.get(&sid) {
                if srv.status != "online" && !params.contains_key("-virtual") {
                    return vec!["error id=1033 msg=virtual\\sserver\\sis\\snot\\srunning".into()];
                }
                *active_sid = sid;
                vec!["error id=0 msg=ok".into()]
            } else {
                vec!["error id=1024 msg=invalid\\sserverID".into()]
            }
        }
        "serverlist" => {
            let w = world.read().await;
            let entries: Vec<String> = w
                .servers
                .values()
                .map(|s| {
                    format_kv(&[
                        ("virtualserver_id", s.id.to_string()),
                        ("virtualserver_port", s.port.to_string()),
                        ("virtualserver_status", s.status.clone()),
                        ("virtualserver_clientsonline", s.clients.len().to_string()),
                        ("virtualserver_queryclientsonline", "1".into()),
                        ("virtualserver_maxclients", s.max_clients.to_string()),
                        ("virtualserver_uptime", s.uptime_secs.to_string()),
                        ("virtualserver_name", s.name.clone()),
                        ("virtualserver_autostart", "1".into()),
                    ])
                })
                .collect();
            vec![entries.join("|"), "error id=0 msg=ok".into()]
        }
        "serverstart" => {
            let sid = get_u32("sid").unwrap_or(*active_sid);
            let mut w = world.write().await;
            if let Some(srv) = w.servers.get_mut(&sid) {
                srv.status = "online".into();
                vec!["error id=0 msg=ok".into()]
            } else {
                vec!["error id=1024 msg=invalid\\sserverID".into()]
            }
        }
        "serverstop" => {
            let sid = get_u32("sid").unwrap_or(*active_sid);
            let mut w = world.write().await;
            if let Some(srv) = w.servers.get_mut(&sid) {
                srv.status = "offline".into();
                vec!["error id=0 msg=ok".into()]
            } else {
                vec!["error id=1024 msg=invalid\\sserverID".into()]
            }
        }
        "servercreate" => {
            let mut w = world.write().await;
            let sid = w.next_sid;
            w.next_sid += 1;
            let name = params.get("virtualserver_name").cloned().unwrap_or_else(|| format!("Virtual Server {sid}"));
            let max_c = get_u32("virtualserver_maxclients").unwrap_or(32);
            let port = get_u32("virtualserver_port").unwrap_or(9986 + sid) as u16;
            let mut template = w.servers.get(&1).expect("seed server 1").clone();
            template.id = sid;
            template.name = name;
            template.max_clients = max_c;
            template.port = port;
            template.clients.clear();
            template.channels.truncate(1);
            recalc_channel_counts(&mut template);
            w.servers.insert(sid, template);
            vec![format!("sid={sid} virtualserver_port={port}"), "error id=0 msg=ok".into()]
        }
        "serverdelete" => {
            let sid = get_u32("sid").unwrap_or(0);
            let mut w = world.write().await;
            if sid <= 1 || w.servers.remove(&sid).is_none() {
                vec!["error id=1024 msg=invalid\\sserverID".into()]
            } else {
                vec!["error id=0 msg=ok".into()]
            }
        }
        "serverinfo" => {
            let w = world.read().await;
            let Some(srv) = w.servers.get(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            let line = format_kv(&[
                ("virtualserver_id", srv.id.to_string()),
                ("virtualserver_unique_identifier", format!("mock-uid-server-{}", srv.id)),
                ("virtualserver_name", srv.name.clone()),
                ("virtualserver_welcomemessage", srv.welcome_message.clone()),
                ("virtualserver_platform", "Linux (Sandbox)".into()),
                ("virtualserver_version", "6.0.0-sandbox [Build: 20261124]".into()),
                ("virtualserver_maxclients", srv.max_clients.to_string()),
                ("virtualserver_clientsonline", srv.clients.len().to_string()),
                ("virtualserver_queryclientsonline", "1".into()),
                ("virtualserver_channelsonline", srv.channels.len().to_string()),
                ("virtualserver_uptime", srv.uptime_secs.to_string()),
                ("virtualserver_total_ping", format!("{:.2}", srv.avg_ping_ms)),
                ("virtualserver_total_packetloss_total", format!("{:.4}", srv.packet_loss_ratio)),
                ("connection_bandwidth_received_last_second_total", srv.bw_in_bps.to_string()),
                ("connection_bandwidth_sent_last_second_total", srv.bw_out_bps.to_string()),
                ("virtualserver_flag_password", if srv.password.is_empty() { "0" } else { "1" }.into()),
                ("virtualserver_hostbanner_url", srv.hostbanner_url.clone()),
                ("virtualserver_hostbanner_gfx_url", srv.hostbanner_gfx_url.clone()),
                ("virtualserver_hostbutton_tooltip", srv.hostbutton_tooltip.clone()),
                ("virtualserver_hostbutton_url", srv.hostbutton_url.clone()),
                ("virtualserver_status", srv.status.clone()),
            ]);
            vec![line, "error id=0 msg=ok".into()]
        }
        "serveredit" => {
            let mut w = world.write().await;
            let Some(srv) = w.servers.get_mut(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            if let Some(v) = params.get("virtualserver_name") { srv.name = v.clone(); }
            if let Some(v) = params.get("virtualserver_welcomemessage") { srv.welcome_message = v.clone(); }
            if let Some(v) = get_u32("virtualserver_maxclients") { srv.max_clients = v; }
            if let Some(v) = params.get("virtualserver_password") { srv.password = v.clone(); }
            if let Some(v) = params.get("virtualserver_hostbanner_url") { srv.hostbanner_url = v.clone(); }
            if let Some(v) = params.get("virtualserver_hostbanner_gfx_url") { srv.hostbanner_gfx_url = v.clone(); }
            if let Some(v) = params.get("virtualserver_hostbutton_tooltip") { srv.hostbutton_tooltip = v.clone(); }
            if let Some(v) = params.get("virtualserver_hostbutton_url") { srv.hostbutton_url = v.clone(); }
            let _ = notify_tx.send((*active_sid, "notifyserveredited".into()));
            vec!["error id=0 msg=ok".into()]
        }
        "channellist" => {
            let w = world.read().await;
            let Some(srv) = w.servers.get(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            let items: Vec<String> = srv
                .channels
                .iter()
                .map(|c| {
                    format_kv(&[
                        ("cid", c.cid.to_string()),
                        ("pid", c.pid.to_string()),
                        ("channel_order", c.channel_order.to_string()),
                        ("channel_name", c.channel_name.clone()),
                        ("channel_topic", c.channel_topic.clone()),
                        ("total_clients", c.total_clients.to_string()),
                        ("channel_maxclients", c.max_clients.to_string()),
                        ("channel_flag_permanent", if c.is_permanent { "1" } else { "0" }.into()),
                        ("channel_flag_semi_permanent", if c.is_semi_permanent { "1" } else { "0" }.into()),
                        ("channel_flag_default", if c.is_default { "1" } else { "0" }.into()),
                        ("channel_flag_password", if c.has_password { "1" } else { "0" }.into()),
                        ("channel_codec", c.codec.to_string()),
                        ("channel_codec_quality", c.codec_quality.to_string()),
                        ("channel_needed_talk_power", c.needed_talk_power.to_string()),
                    ])
                })
                .collect();
            vec![items.join("|"), "error id=0 msg=ok".into()]
        }
        "channelcreate" => {
            let mut w = world.write().await;
            let Some(srv) = w.servers.get_mut(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            let cid = srv.next_cid;
            srv.next_cid += 1;
            let pid = get_u32("cpid").unwrap_or(0);
            let name = params.get("channel_name").cloned().unwrap_or_else(|| format!("Channel {cid}"));
            let node = ChannelNode {
                cid,
                pid,
                channel_order: get_u32("channel_order").unwrap_or(0),
                channel_name: name.clone(),
                channel_topic: params.get("channel_topic").cloned().unwrap_or_default(),
                total_clients: 0,
                max_clients: get_i32("channel_maxclients").unwrap_or(-1),
                is_permanent: params.get("channel_flag_permanent").map(|v| v == "1").unwrap_or(true),
                is_semi_permanent: params.get("channel_flag_semi_permanent").map(|v| v == "1").unwrap_or(false),
                is_default: params.get("channel_flag_default").map(|v| v == "1").unwrap_or(false),
                has_password: params.get("channel_password").map(|v| !v.is_empty()).unwrap_or(false),
                codec: get_u32("channel_codec").unwrap_or(4),
                codec_quality: get_u32("channel_codec_quality").unwrap_or(6),
                needed_talk_power: get_i32("channel_needed_talk_power").unwrap_or(0),
            };
            srv.channels.push(node);
            let _ = notify_tx.send((
                *active_sid,
                format!("notifychannelcreated cid={cid} cpid={pid} channel_name={}", escape_ts(&name)),
            ));
            vec![format!("cid={cid}"), "error id=0 msg=ok".into()]
        }
        "channeledit" => {
            let cid = get_u32("cid").unwrap_or(0);
            let mut w = world.write().await;
            let Some(srv) = w.servers.get_mut(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            let Some(ch) = srv.channels.iter_mut().find(|c| c.cid == cid) else {
                return vec!["error id=768 msg=invalid\\schannelID".into()];
            };
            if let Some(v) = params.get("channel_name") { ch.channel_name = v.clone(); }
            if let Some(v) = params.get("channel_topic") { ch.channel_topic = v.clone(); }
            if let Some(v) = get_i32("channel_maxclients") { ch.max_clients = v; }
            if let Some(v) = params.get("channel_flag_permanent") { ch.is_permanent = v == "1"; }
            if let Some(v) = params.get("channel_flag_semi_permanent") { ch.is_semi_permanent = v == "1"; }
            if let Some(v) = params.get("channel_flag_default") { ch.is_default = v == "1"; }
            if let Some(v) = get_u32("channel_codec") { ch.codec = v; }
            if let Some(v) = get_u32("channel_codec_quality") { ch.codec_quality = v; }
            if let Some(v) = get_i32("channel_needed_talk_power") { ch.needed_talk_power = v; }
            if let Some(v) = get_u32("channel_order") { ch.channel_order = v; }
            let _ = notify_tx.send((*active_sid, format!("notifychanneledited cid={cid} invokername=serveradmin")));
            vec!["error id=0 msg=ok".into()]
        }
        "channelmove" => {
            let cid = get_u32("cid").unwrap_or(0);
            let cpid = get_u32("cpid").unwrap_or(0);
            let order = get_u32("order").unwrap_or(0);
            let mut w = world.write().await;
            let Some(srv) = w.servers.get_mut(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            if let Some(ch) = srv.channels.iter_mut().find(|c| c.cid == cid) {
                ch.pid = cpid;
                ch.channel_order = order;
                let _ = notify_tx.send((*active_sid, format!("notifychannelmoved cid={cid} cpid={cpid} order={order}")));
                vec!["error id=0 msg=ok".into()]
            } else {
                vec!["error id=768 msg=invalid\\schannelID".into()]
            }
        }
        "channeldelete" => {
            let cid = get_u32("cid").unwrap_or(0);
            let force = params.get("force").map(|v| v == "1").unwrap_or(false);
            let mut w = world.write().await;
            let Some(srv) = w.servers.get_mut(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            let has_clients = srv.clients.iter().any(|c| c.cid == cid);
            if has_clients && !force {
                return vec!["error id=771 msg=channel\\sis\\snot\\sempty".into()];
            }
            if has_clients {
                let default_cid = srv.channels.iter().find(|c| c.is_default).map(|c| c.cid).unwrap_or(1);
                for cl in &mut srv.clients {
                    if cl.cid == cid { cl.cid = default_cid; }
                }
            }
            srv.channels.retain(|c| c.cid != cid && c.pid != cid);
            recalc_channel_counts(srv);
            let _ = notify_tx.send((*active_sid, format!("notifychanneldeleted cid={cid}")));
            vec!["error id=0 msg=ok".into()]
        }
        "clientlist" => {
            let w = world.read().await;
            let Some(srv) = w.servers.get(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            let items: Vec<String> = srv
                .clients
                .iter()
                .map(|c| {
                    let groups = c.server_groups.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
                    format_kv(&[
                        ("clid", c.clid.to_string()),
                        ("cid", c.cid.to_string()),
                        ("client_database_id", c.cldbid.to_string()),
                        ("client_unique_identifier", c.uid.clone()),
                        ("client_nickname", c.nickname.clone()),
                        ("client_type", c.client_type.to_string()),
                        ("client_input_muted", if c.input_muted { "1" } else { "0" }.into()),
                        ("client_output_muted", if c.output_muted { "1" } else { "0" }.into()),
                        ("client_away", if c.away { "1" } else { "0" }.into()),
                        ("client_away_message", c.away_message.clone()),
                        ("client_flag_talking", if c.is_talking { "1" } else { "0" }.into()),
                        ("client_servergroups", groups),
                        ("client_channel_group_id", c.channel_group_id.to_string()),
                        ("client_idle_time", c.idle_time_ms.to_string()),
                        ("client_connected_time", c.connected_time_ms.to_string()),
                        ("connection_client_ip", c.ip.clone()),
                        ("client_country", c.country.clone()),
                        ("client_version", c.version.clone()),
                        ("client_platform", c.platform.clone()),
                        ("client_ping", format!("{:.1}", c.ping_ms)),
                    ])
                })
                .collect();
            vec![items.join("|"), "error id=0 msg=ok".into()]
        }
        "clientmove" => {
            let clid = get_u32("clid").unwrap_or(0);
            let cid = get_u32("cid").unwrap_or(0);
            let mut w = world.write().await;
            let Some(srv) = w.servers.get_mut(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            if !srv.channels.iter().any(|c| c.cid == cid) {
                return vec!["error id=768 msg=invalid\\schannelID".into()];
            }
            if let Some(cl) = srv.clients.iter_mut().find(|c| c.clid == clid) {
                cl.cid = cid;
                cl.idle_time_ms = 0;
                recalc_channel_counts(srv);
                let _ = notify_tx.send((*active_sid, format!("notifyclientmoved ctid={cid} reasonid=1 invokername=serveradmin clid={clid}")));
                vec!["error id=0 msg=ok".into()]
            } else {
                vec!["error id=512 msg=invalid\\sclientID".into()]
            }
        }
        "clientkick" => {
            let clid = get_u32("clid").unwrap_or(0);
            let reasonid = get_u32("reasonid").unwrap_or(5);
            let reason = params.get("reasonmsg").cloned().unwrap_or_else(|| "Kicked by admin".into());
            let mut w = world.write().await;
            let Some(srv) = w.servers.get_mut(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            let default_cid = srv.channels.iter().find(|c| c.is_default).map(|c| c.cid).unwrap_or(1);
            if let Some(pos) = srv.clients.iter().position(|c| c.clid == clid) {
                let cfid = srv.clients[pos].cid;
                if reasonid == 4 {
                    srv.clients[pos].cid = default_cid;
                    recalc_channel_counts(srv);
                    let _ = notify_tx.send((*active_sid, format!("notifyclientmoved ctid={default_cid} reasonid=4 invokername=serveradmin clid={clid}")));
                } else {
                    srv.clients.remove(pos);
                    recalc_channel_counts(srv);
                    let _ = notify_tx.send((*active_sid, format!("notifyclientleftview cfid={cfid} reasonid=5 reasonmsg={} clid={clid}", escape_ts(&reason))));
                }
                vec!["error id=0 msg=ok".into()]
            } else {
                vec!["error id=512 msg=invalid\\sclientID".into()]
            }
        }
        "clientpoke" | "sendtextmessage" => {
            let msg = params.get("msg").cloned().unwrap_or_default();
            let tm = get_u32("targetmode").unwrap_or(3);
            let mut w = world.write().await;
            if let Some(srv) = w.servers.get_mut(active_sid) {
                srv.logs.insert(0, format!("2026-11-24 16:00:00.000000|INFO    |VirtualServer |{}  |{cmd}: {msg}", srv.id));
            }
            let _ = notify_tx.send((*active_sid, format!("notifytextmessage targetmode={tm} invokerid=99 invokername=serveradmin msg={}", escape_ts(&msg))));
            vec!["error id=0 msg=ok".into()]
        }
        "banclient" => {
            let clid = get_u32("clid").unwrap_or(0);
            let dur = get_i64("time").unwrap_or(0);
            let reason = params.get("banreason").cloned().unwrap_or_else(|| "Banned by administrator".into());
            let mut w = world.write().await;
            let Some(srv) = w.servers.get_mut(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            if let Some(pos) = srv.clients.iter().position(|c| c.clid == clid) {
                let cl = srv.clients.remove(pos);
                let banid = srv.next_banid;
                srv.next_banid += 1;
                srv.bans.push(BanRule {
                    banid,
                    ip: cl.ip.clone(),
                    name: cl.nickname.clone(),
                    uid: cl.uid.clone(),
                    created: unix_now(),
                    duration_secs: dur,
                    invoker_name: "serveradmin".into(),
                    invoker_uid: "serveradmin".into(),
                    reason: reason.clone(),
                    enforcements: 1,
                });
                recalc_channel_counts(srv);
                let _ = notify_tx.send((*active_sid, format!("notifyclientleftview cfid={} reasonid=6 reasonmsg={} clid={clid}", cl.cid, escape_ts(&reason))));
                vec![format!("banid={banid}"), "error id=0 msg=ok".into()]
            } else {
                vec!["error id=512 msg=invalid\\sclientID".into()]
            }
        }
        "servergrouplist" => {
            let w = world.read().await;
            let Some(srv) = w.servers.get(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            let items: Vec<String> = srv
                .server_groups
                .iter()
                .map(|g| {
                    format_kv(&[
                        ("sgid", g.sgid.to_string()),
                        ("name", g.name.clone()),
                        ("type", g.group_type.to_string()),
                        ("iconid", g.icon_id.to_string()),
                        ("savedb", if g.savedb { "1" } else { "0" }.into()),
                        ("sortid", g.sortid.to_string()),
                        ("namemode", g.namemode.to_string()),
                    ])
                })
                .collect();
            vec![items.join("|"), "error id=0 msg=ok".into()]
        }
        "servergroupadd" => {
            let name = params.get("name").cloned().unwrap_or_else(|| "New Group".into());
            let gtype = get_u32("type").unwrap_or(1) as u8;
            let mut w = world.write().await;
            let Some(srv) = w.servers.get_mut(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            let sgid = srv.next_sgid;
            srv.next_sgid += 1;
            srv.server_groups.push(ServerGroup {
                sgid,
                name,
                group_type: gtype,
                icon_id: 0,
                savedb: true,
                sortid: sgid as i32,
                namemode: 0,
            });
            srv.group_perms.insert(sgid, default_permissions(false, false));
            vec![format!("sgid={sgid}"), "error id=0 msg=ok".into()]
        }
        "servergrouprename" => {
            let sgid = get_u32("sgid").unwrap_or(0);
            let name = params.get("name").cloned().unwrap_or_default();
            let mut w = world.write().await;
            let Some(srv) = w.servers.get_mut(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            if let Some(g) = srv.server_groups.iter_mut().find(|g| g.sgid == sgid) {
                g.name = name;
                vec!["error id=0 msg=ok".into()]
            } else {
                vec!["error id=2560 msg=invalid\\sgroup\\sID".into()]
            }
        }
        "servergroupdel" => {
            let sgid = get_u32("sgid").unwrap_or(0);
            let mut w = world.write().await;
            let Some(srv) = w.servers.get_mut(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            srv.server_groups.retain(|g| g.sgid != sgid);
            srv.group_members.remove(&sgid);
            srv.group_perms.remove(&sgid);
            for cl in &mut srv.clients {
                cl.server_groups.retain(|&id| id != sgid);
            }
            vec!["error id=0 msg=ok".into()]
        }
        "servergroupclientlist" => {
            let sgid = get_u32("sgid").unwrap_or(0);
            let w = world.read().await;
            let Some(srv) = w.servers.get(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            let ids = srv.group_members.get(&sgid).cloned().unwrap_or_default();
            if ids.is_empty() {
                return vec!["error id=1281 msg=database\\sempty\\sresult\\sset".into()];
            }
            let items: Vec<String> = ids
                .into_iter()
                .filter_map(|id| srv.db_clients.iter().find(|c| c.cldbid == id))
                .map(|c| {
                    format_kv(&[
                        ("cldbid", c.cldbid.to_string()),
                        ("client_nickname", c.nickname.clone()),
                        ("client_unique_identifier", c.uid.clone()),
                    ])
                })
                .collect();
            vec![items.join("|"), "error id=0 msg=ok".into()]
        }
        "servergroupaddclient" => {
            let sgid = get_u32("sgid").unwrap_or(0);
            let cldbid = get_u32("cldbid").unwrap_or(0);
            let mut w = world.write().await;
            let Some(srv) = w.servers.get_mut(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            let entry = srv.group_members.entry(sgid).or_default();
            if !entry.contains(&cldbid) {
                entry.push(cldbid);
            }
            for cl in &mut srv.clients {
                if cl.cldbid == cldbid && !cl.server_groups.contains(&sgid) {
                    cl.server_groups.push(sgid);
                }
            }
            vec!["error id=0 msg=ok".into()]
        }
        "servergroupdelclient" => {
            let sgid = get_u32("sgid").unwrap_or(0);
            let cldbid = get_u32("cldbid").unwrap_or(0);
            let mut w = world.write().await;
            let Some(srv) = w.servers.get_mut(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            if let Some(entry) = srv.group_members.get_mut(&sgid) {
                entry.retain(|&id| id != cldbid);
            }
            for cl in &mut srv.clients {
                if cl.cldbid == cldbid {
                    cl.server_groups.retain(|&id| id != sgid);
                }
            }
            vec!["error id=0 msg=ok".into()]
        }
        "servergrouppermlist" => {
            let sgid = get_u32("sgid").unwrap_or(0);
            let w = world.read().await;
            let Some(srv) = w.servers.get(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            let perms = srv.group_perms.get(&sgid).cloned().unwrap_or_default();
            if perms.is_empty() {
                return vec!["error id=1281 msg=database\\sempty\\sresult\\sset".into()];
            }
            let items: Vec<String> = perms
                .iter()
                .map(|p| {
                    format_kv(&[
                        ("permid", p.permid.to_string()),
                        ("permsid", p.permname.clone()),
                        ("permvalue", p.permvalue.to_string()),
                        ("permnegated", if p.permnegated { "1" } else { "0" }.into()),
                        ("permskip", if p.permskip { "1" } else { "0" }.into()),
                    ])
                })
                .collect();
            vec![items.join("|"), "error id=0 msg=ok".into()]
        }
        "servergroupaddperm" => {
            let sgid = get_u32("sgid").unwrap_or(0);
            let permsid = params.get("permsid").cloned().unwrap_or_default();
            let permvalue = get_i64("permvalue").unwrap_or(0);
            let permnegated = params.get("permnegated").map(|v| v == "1").unwrap_or(false);
            let permskip = params.get("permskip").map(|v| v == "1").unwrap_or(false);
            let mut w = world.write().await;
            let Some(srv) = w.servers.get_mut(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            let list = srv.group_perms.entry(sgid).or_default();
            if let Some(existing) = list.iter_mut().find(|p| p.permname == permsid) {
                existing.permvalue = permvalue;
                existing.permnegated = permnegated;
                existing.permskip = permskip;
            } else {
                let next_id = list.len() as u32 + 100;
                list.push(PermissionEntry {
                    permid: next_id,
                    permname: permsid,
                    permvalue,
                    permnegated,
                    permskip,
                });
            }
            vec!["error id=0 msg=ok".into()]
        }
        "servergroupdelperm" => {
            let sgid = get_u32("sgid").unwrap_or(0);
            let permsid = params.get("permsid").cloned().unwrap_or_default();
            let mut w = world.write().await;
            let Some(srv) = w.servers.get_mut(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            if let Some(list) = srv.group_perms.get_mut(&sgid) {
                list.retain(|p| p.permname != permsid);
            }
            vec!["error id=0 msg=ok".into()]
        }
        "channelgrouplist" => {
            let w = world.read().await;
            let Some(srv) = w.servers.get(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            let items: Vec<String> = srv
                .channel_groups
                .iter()
                .map(|g| {
                    format_kv(&[
                        ("cgid", g.cgid.to_string()),
                        ("name", g.name.clone()),
                        ("type", g.group_type.to_string()),
                        ("iconid", g.icon_id.to_string()),
                        ("savedb", if g.savedb { "1" } else { "0" }.into()),
                        ("sortid", g.sortid.to_string()),
                        ("namemode", g.namemode.to_string()),
                    ])
                })
                .collect();
            vec![items.join("|"), "error id=0 msg=ok".into()]
        }
        "privilegekeylist" => {
            let w = world.read().await;
            let Some(srv) = w.servers.get(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            if srv.privilege_keys.is_empty() {
                return vec!["error id=1281 msg=database\\sempty\\sresult\\sset".into()];
            }
            let items: Vec<String> = srv
                .privilege_keys
                .iter()
                .map(|t| {
                    format_kv(&[
                        ("token", t.token.clone()),
                        ("token_type", t.token_type.to_string()),
                        ("token_id1", t.token_id1.to_string()),
                        ("token_id2", t.token_id2.to_string()),
                        ("token_created", t.created.to_string()),
                        ("token_description", t.description.clone()),
                    ])
                })
                .collect();
            vec![items.join("|"), "error id=0 msg=ok".into()]
        }
        "privilegekeyadd" => {
            let token_type = get_u32("tokentype").unwrap_or(0) as u8;
            let id1 = get_u32("tokenid1").unwrap_or(6);
            let id2 = get_u32("tokenid2").unwrap_or(0);
            let desc = params.get("tokendescription").cloned().unwrap_or_default();
            let token = format!("TSS-{:04X}-{:04X}-{:04X}", rand::random::<u16>(), id1, rand::random::<u16>());
            let mut w = world.write().await;
            let Some(srv) = w.servers.get_mut(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            srv.privilege_keys.push(PrivilegeKey {
                token: token.clone(),
                token_type,
                token_id1: id1,
                token_id2: id2,
                created: unix_now(),
                description: desc,
            });
            vec![format!("token={token}"), "error id=0 msg=ok".into()]
        }
        "privilegekeydelete" => {
            let token = params.get("token").cloned().unwrap_or_default();
            let mut w = world.write().await;
            let Some(srv) = w.servers.get_mut(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            srv.privilege_keys.retain(|t| t.token != token);
            vec!["error id=0 msg=ok".into()]
        }
        "banlist" => {
            let w = world.read().await;
            let Some(srv) = w.servers.get(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            if srv.bans.is_empty() {
                return vec!["error id=1281 msg=database\\sempty\\sresult\\sset".into()];
            }
            let items: Vec<String> = srv
                .bans
                .iter()
                .map(|b| {
                    format_kv(&[
                        ("banid", b.banid.to_string()),
                        ("ip", b.ip.clone()),
                        ("name", b.name.clone()),
                        ("uid", b.uid.clone()),
                        ("created", b.created.to_string()),
                        ("duration", b.duration_secs.to_string()),
                        ("invokername", b.invoker_name.clone()),
                        ("invokeruid", b.invoker_uid.clone()),
                        ("reason", b.reason.clone()),
                        ("enforcements", b.enforcements.to_string()),
                    ])
                })
                .collect();
            vec![items.join("|"), "error id=0 msg=ok".into()]
        }
        "banadd" => {
            let mut w = world.write().await;
            let Some(srv) = w.servers.get_mut(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            let banid = srv.next_banid;
            srv.next_banid += 1;
            srv.bans.push(BanRule {
                banid,
                ip: params.get("ip").cloned().unwrap_or_default(),
                name: params.get("name").cloned().unwrap_or_default(),
                uid: params.get("uid").cloned().unwrap_or_default(),
                created: unix_now(),
                duration_secs: get_i64("time").unwrap_or(0),
                invoker_name: "serveradmin".into(),
                invoker_uid: "serveradmin".into(),
                reason: params.get("banreason").cloned().unwrap_or_default(),
                enforcements: 0,
            });
            vec![format!("banid={banid}"), "error id=0 msg=ok".into()]
        }
        "bandel" => {
            let banid = get_u32("banid").unwrap_or(0);
            let mut w = world.write().await;
            let Some(srv) = w.servers.get_mut(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            srv.bans.retain(|b| b.banid != banid);
            vec!["error id=0 msg=ok".into()]
        }
        "clientdblist" => {
            let start = get_u32("start").unwrap_or(0) as usize;
            let dur = get_u32("duration").unwrap_or(50) as usize;
            let w = world.read().await;
            let Some(srv) = w.servers.get(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            let slice: Vec<&DbClient> = srv.db_clients.iter().skip(start).take(dur).collect();
            if slice.is_empty() {
                return vec!["error id=1281 msg=database\\sempty\\sresult\\sset".into()];
            }
            let items: Vec<String> = slice
                .into_iter()
                .map(|c| {
                    format_kv(&[
                        ("cldbid", c.cldbid.to_string()),
                        ("client_unique_identifier", c.uid.clone()),
                        ("client_nickname", c.nickname.clone()),
                        ("client_created", c.created.to_string()),
                        ("client_lastconnected", c.last_connected.to_string()),
                        ("client_totalconnections", c.total_connections.to_string()),
                        ("client_lastip", c.last_ip.clone()),
                        ("client_description", c.description.clone()),
                    ])
                })
                .collect();
            vec![items.join("|"), "error id=0 msg=ok".into()]
        }
        "clientdbfind" => {
            let pat = params.get("pattern").cloned().unwrap_or_default().to_lowercase();
            let w = world.read().await;
            let Some(srv) = w.servers.get(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            let items: Vec<String> = srv
                .db_clients
                .iter()
                .filter(|c| c.nickname.to_lowercase().contains(&pat) || c.uid.to_lowercase().contains(&pat))
                .map(|c| format!("cldbid={}", c.cldbid))
                .collect();
            if items.is_empty() {
                return vec!["error id=1281 msg=database\\sempty\\sresult\\sset".into()];
            }
            vec![items.join("|"), "error id=0 msg=ok".into()]
        }
        "clientdbinfo" => {
            let cldbid = get_u32("cldbid").unwrap_or(0);
            let w = world.read().await;
            let Some(srv) = w.servers.get(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            let Some(c) = srv.db_clients.iter().find(|c| c.cldbid == cldbid) else {
                return vec!["error id=1281 msg=database\\sempty\\sresult\\sset".into()];
            };
            let line = format_kv(&[
                ("cldbid", c.cldbid.to_string()),
                ("client_unique_identifier", c.uid.clone()),
                ("client_nickname", c.nickname.clone()),
                ("client_created", c.created.to_string()),
                ("client_lastconnected", c.last_connected.to_string()),
                ("client_totalconnections", c.total_connections.to_string()),
                ("client_lastip", c.last_ip.clone()),
                ("client_description", c.description.clone()),
            ]);
            vec![line, "error id=0 msg=ok".into()]
        }
        "clientdbdelete" => {
            let cldbid = get_u32("cldbid").unwrap_or(0);
            let mut w = world.write().await;
            let Some(srv) = w.servers.get_mut(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            srv.db_clients.retain(|c| c.cldbid != cldbid);
            vec!["error id=0 msg=ok".into()]
        }
        "complainlist" => {
            let w = world.read().await;
            let Some(srv) = w.servers.get(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            if srv.complaints.is_empty() {
                return vec!["error id=1281 msg=database\\sempty\\sresult\\sset".into()];
            }
            let items: Vec<String> = srv
                .complaints
                .iter()
                .map(|c| {
                    format_kv(&[
                        ("tcldbid", c.tcldbid.to_string()),
                        ("tname", c.tname.clone()),
                        ("fcldbid", c.fcldbid.to_string()),
                        ("fname", c.fname.clone()),
                        ("message", c.message.clone()),
                        ("timestamp", c.timestamp.to_string()),
                    ])
                })
                .collect();
            vec![items.join("|"), "error id=0 msg=ok".into()]
        }
        "complaindel" => {
            let tcldbid = get_u32("tcldbid").unwrap_or(0);
            let fcldbid = get_u32("fcldbid").unwrap_or(0);
            let mut w = world.write().await;
            let Some(srv) = w.servers.get_mut(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            srv.complaints.retain(|c| !(c.tcldbid == tcldbid && c.fcldbid == fcldbid));
            vec!["error id=0 msg=ok".into()]
        }
        "logview" => {
            let lines = get_u32("lines").unwrap_or(30) as usize;
            let w = world.read().await;
            let Some(srv) = w.servers.get(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            if srv.logs.is_empty() {
                return vec!["error id=1281 msg=database\\sempty\\sresult\\sset".into()];
            }
            let items: Vec<String> = srv
                .logs
                .iter()
                .take(lines)
                .map(|l| format!("l={}", escape_ts(l)))
                .collect();
            vec![items.join("|"), "error id=0 msg=ok".into()]
        }
        "serversnapshotcreate" => {
            let w = world.read().await;
            let Some(srv) = w.servers.get(active_sid) else {
                return vec!["error id=1024 msg=invalid\\sserverID".into()];
            };
            let json_bytes = serde_json::to_vec(srv).unwrap_or_default();
            let encoded = base64::engine::general_purpose::STANDARD.encode(json_bytes);
            vec![format!("version=1 data={encoded}"), "error id=0 msg=ok".into()]
        }
        "serversnapshotdeploy" => {
            let data = params.get("data").cloned().unwrap_or_default();
            let Ok(raw) = base64::engine::general_purpose::STANDARD.decode(data.as_bytes()) else {
                return vec!["error id=1538 msg=invalid\\sparameter".into()];
            };
            let Ok(mut restored) = serde_json::from_slice::<MockVirtualServer>(&raw) else {
                return vec!["error id=1538 msg=invalid\\sparameter".into()];
            };
            let mut w = world.write().await;
            let sid = *active_sid;
            if let Some(current) = w.servers.get(&sid) {
                // Keep live connected clients mapped onto valid restored channels
                restored.id = sid;
                restored.clients = current
                    .clients
                    .iter()
                    .cloned()
                    .map(|mut cl| {
                        if !restored.channels.iter().any(|c| c.cid == cl.cid) {
                            cl.cid = restored.channels.first().map(|c| c.cid).unwrap_or(1);
                        }
                        cl
                    })
                    .collect();
                recalc_channel_counts(&mut restored);
            }
            w.servers.insert(sid, restored);
            let _ = notify_tx.send((sid, "notifyserveredited".into()));
            vec!["error id=0 msg=ok".into()]
        }
        _ => vec!["error id=256 msg=command\\snot\\sfound".into()],
    }
}
