use crate::codec::{
    build_command, parse_error_line, parse_notify_line, parse_records, trim_ts_line,
};
use crate::driver::{
    map_ban_rule, map_channel, map_channel_group, map_client, map_complaint, map_db_client,
    map_group_member, map_log_entry, map_permission, map_privilege_key, map_server_group,
    map_server_info, map_virtual_server, RecordMap, TsProtocolDriver,
};
use crate::types::{
    AddBanParams, BanRule, ChannelGroup, ChannelNode, ComplaintEntry, CreateChannelParams,
    CreatePrivilegeKeyParams, DbClient, EditChannelParams, EditServerInfoParams, GroupMember,
    OnlineClient, PermissionEntry, PrivilegeKey, Result, ServerGroup, ServerInfoSnapshot,
    SetPermissionParams, TsConnectionConfig, TsLogEntry, TsNotifyEvent, TsProtoError,
    TsSnapshotData, VirtualServerSummary,
};
use async_trait::async_trait;
use russh::client;
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, ReadBuf};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio::time::{timeout, Instant};
use tracing::{debug, warn};

pub trait AsyncByteStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> AsyncByteStream for T {}

#[async_trait]
pub trait QueryStreamConnector: Send + Sync {
    async fn open_stream(&self) -> Result<Box<dyn AsyncByteStream>>;
    fn requires_login_command(&self) -> bool;
    fn credentials(&self) -> (&str, &str);
}

pub struct RawTcpConnector {
    host: String,
    port: u16,
    username: String,
    secret: String,
}

impl RawTcpConnector {
    pub fn new(cfg: &TsConnectionConfig) -> Self {
        Self {
            host: cfg.host.clone(),
            port: cfg.port,
            username: cfg.username.clone(),
            secret: cfg.secret.clone(),
        }
    }
}

#[async_trait]
impl QueryStreamConnector for RawTcpConnector {
    async fn open_stream(&self) -> Result<Box<dyn AsyncByteStream>> {
        let addr = format!("{}:{}", self.host, self.port);
        let stream = timeout(Duration::from_secs(8), TcpStream::connect(&addr))
            .await
            .map_err(|_| TsProtoError::Timeout)??;
        let _ = stream.set_nodelay(true);
        Ok(Box::new(stream))
    }

    fn requires_login_command(&self) -> bool {
        true
    }

    fn credentials(&self) -> (&str, &str) {
        (&self.username, &self.secret)
    }
}

pub struct SshConnector {
    host: String,
    port: u16,
    username: String,
    secret: String,
}

impl SshConnector {
    pub fn new(cfg: &TsConnectionConfig) -> Self {
        Self {
            host: cfg.host.clone(),
            port: cfg.port,
            username: cfg.username.clone(),
            secret: cfg.secret.clone(),
        }
    }
}

struct PermissiveSshHandler;

#[async_trait]
impl client::Handler for PermissiveSshHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::key::PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        Ok(true)
    }
}

struct SshChannelStream {
    inner: russh::ChannelStream<client::Msg>,
    _handle: client::Handle<PermissiveSshHandler>,
}

impl AsyncRead for SshChannelStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for SshChannelStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

#[async_trait]
impl QueryStreamConnector for SshConnector {
    async fn open_stream(&self) -> Result<Box<dyn AsyncByteStream>> {
        let config = Arc::new(client::Config {
            inactivity_timeout: Some(Duration::from_secs(300)),
            ..Default::default()
        });
        let addr = format!("{}:{}", self.host, self.port);
        let mut handle = timeout(
            Duration::from_secs(10),
            client::connect(config, addr, PermissiveSshHandler),
        )
        .await
        .map_err(|_| TsProtoError::Timeout)?
        .map_err(|e| TsProtoError::Ssh(e.to_string()))?;

        let auth_ok = timeout(
            Duration::from_secs(10),
            handle.authenticate_password(&self.username, &self.secret),
        )
        .await
        .map_err(|_| TsProtoError::Timeout)?
        .map_err(|e| TsProtoError::Ssh(e.to_string()))?;
        if !auth_ok {
            return Err(TsProtoError::Ssh(
                "SSH password authentication rejected".to_string(),
            ));
        }

        let channel = timeout(Duration::from_secs(10), handle.channel_open_session())
            .await
            .map_err(|_| TsProtoError::Timeout)?
            .map_err(|e| TsProtoError::Ssh(e.to_string()))?;
        timeout(Duration::from_secs(10), channel.request_shell(true))
            .await
            .map_err(|_| TsProtoError::Timeout)?
            .map_err(|e| TsProtoError::Ssh(e.to_string()))?;

        let stream = channel.into_stream();
        Ok(Box::new(SshChannelStream {
            inner: stream,
            _handle: handle,
        }))
    }

    fn requires_login_command(&self) -> bool {
        false
    }

    fn credentials(&self) -> (&str, &str) {
        (&self.username, &self.secret)
    }
}

const FLOOD_WINDOW: Duration = Duration::from_millis(3200);
const MAX_CMDS_PER_WINDOW: usize = 8;

/// Sliding-window command pacer so non-whitelisted IPs stay below TeamSpeak's
/// default ServerQuery anti-flood threshold (10 commands per 3 seconds).
struct FloodPacer {
    sent_at: VecDeque<Instant>,
}

impl FloodPacer {
    fn new() -> Self {
        Self {
            sent_at: VecDeque::with_capacity(MAX_CMDS_PER_WINDOW + 2),
        }
    }

    async fn wait_slot(&mut self) {
        loop {
            let now = Instant::now();
            while let Some(&front) = self.sent_at.front() {
                if now.saturating_duration_since(front) >= FLOOD_WINDOW {
                    self.sent_at.pop_front();
                } else {
                    break;
                }
            }
            if self.sent_at.len() < MAX_CMDS_PER_WINDOW {
                self.sent_at.push_back(Instant::now());
                return;
            }
            if let Some(&oldest) = self.sent_at.front() {
                tokio::time::sleep_until(oldest + FLOOD_WINDOW).await;
            } else {
                return;
            }
        }
    }
}

struct QueryConnection {
    reader: BufReader<tokio::io::ReadHalf<Box<dyn AsyncByteStream>>>,
    writer: tokio::io::WriteHalf<Box<dyn AsyncByteStream>>,
    line_buf: Vec<u8>,
    active_sid: Option<u32>,
    notify_registered: bool,
    last_io: Instant,
}

impl QueryConnection {
    async fn connect(
        connector: &dyn QueryStreamConnector,
        pacer: &mut FloodPacer,
        events_tx: &broadcast::Sender<TsNotifyEvent>,
    ) -> Result<Self> {
        let stream = connector.open_stream().await?;
        let (rd, wr) = tokio::io::split(stream);
        let mut conn = Self {
            reader: BufReader::new(rd),
            writer: wr,
            line_buf: Vec::with_capacity(512),
            active_sid: None,
            notify_registered: false,
            last_io: Instant::now(),
        };
        conn.read_greeting().await?;

        if connector.requires_login_command() {
            let (user, pass) = connector.credentials();
            if !user.trim().is_empty() {
                conn.send_raw(
                    &build_command(
                        "login",
                        &[],
                        &[("client_login_name", user), ("client_login_password", pass)],
                        &[],
                    ),
                    pacer,
                    events_tx,
                )
                .await?;
            }
        }

        Ok(conn)
    }

    /// Cancel-safe line reader handling `\n\r` (TS3 Raw TCP), `\r\n` (SSH / TS6), and `\n`
    /// without blocking on trailing bytes after `\n`.
    async fn read_raw_line(&mut self) -> Result<String> {
        loop {
            let n = self.reader.read_until(b'\n', &mut self.line_buf).await?;
            if n == 0 && self.line_buf.is_empty() {
                return Err(TsProtoError::ConnectionClosed);
            }
            if self.line_buf.ends_with(b"\n") || n == 0 {
                let bytes = std::mem::take(&mut self.line_buf);
                self.last_io = Instant::now();
                let s = String::from_utf8_lossy(&bytes);
                return Ok(trim_ts_line(&s).to_string());
            }
        }
    }

    async fn read_line_with_timeout(&mut self, wait: Duration) -> Result<String> {
        timeout(wait, self.read_raw_line())
            .await
            .map_err(|_| TsProtoError::Timeout)?
    }

    async fn read_greeting(&mut self) -> Result<()> {
        let first = loop {
            let line = self.read_line_with_timeout(Duration::from_secs(8)).await?;
            if !line.trim().is_empty() {
                break line;
            }
        };

        if let Some(status) = parse_error_line(&first) {
            if status.id != 0 {
                return Err(TsProtoError::QueryError {
                    id: status.id,
                    msg: status.msg,
                    extra_msg: status.extra_msg,
                });
            }
        } else if !first.contains("TS3") && !first.starts_with("Welcome") {
            debug!("Non-standard ServerQuery greeting line: {first}");
        }

        // Read optional second Welcome line without losing partial bytes if single-line
        if let Ok(second_res) = timeout(Duration::from_millis(300), self.read_raw_line()).await {
            if let Ok(second) = second_res {
                if let Some(status) = parse_error_line(&second) {
                    if status.id != 0 {
                        return Err(TsProtoError::QueryError {
                            id: status.id,
                            msg: status.msg,
                            extra_msg: status.extra_msg,
                        });
                    }
                }
            }
        }
        Ok(())
    }

    async fn send_raw(
        &mut self,
        cmd_line: &str,
        pacer: &mut FloodPacer,
        events_tx: &broadcast::Sender<TsNotifyEvent>,
    ) -> Result<Vec<RecordMap>> {
        pacer.wait_slot().await;

        timeout(
            Duration::from_secs(8),
            self.writer.write_all(cmd_line.as_bytes()),
        )
        .await
        .map_err(|_| TsProtoError::Timeout)??;
        timeout(Duration::from_secs(8), self.writer.flush())
            .await
            .map_err(|_| TsProtoError::Timeout)??;
        self.last_io = Instant::now();

        let cmd_trimmed = cmd_line.trim();
        let mut records = Vec::new();
        loop {
            let line = self.read_line_with_timeout(Duration::from_secs(12)).await?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed == cmd_trimmed
                || trimmed.eq_ignore_ascii_case("TS3")
                || trimmed.starts_with("Welcome to the ")
            {
                // Ignore echoed SSH shell command lines or late greeting banners
                continue;
            }
            if let Some(status) = parse_error_line(trimmed) {
                if status.id == 0 {
                    return Ok(records);
                }
                return Err(TsProtoError::QueryError {
                    id: status.id,
                    msg: status.msg,
                    extra_msg: status.extra_msg,
                });
            }
            if trimmed.starts_with("notify") {
                if let Some(evt) = parse_notify_line(trimmed) {
                    let _ = events_tx.send(evt);
                }
                continue;
            }
            records.extend(parse_records(trimmed));
        }
    }

    async fn ensure_sid(
        &mut self,
        sid: u32,
        default_sid: u32,
        enable_notify: bool,
        pacer: &mut FloodPacer,
        events_tx: &broadcast::Sender<TsNotifyEvent>,
    ) -> Result<()> {
        if self.active_sid != Some(sid) {
            let cmd = build_command("use", &[], &[("sid", &sid.to_string())], &[]);
            match self.send_raw(&cmd, pacer, events_tx).await {
                Ok(_) => {
                    self.active_sid = Some(sid);
                    self.notify_registered = false;
                }
                Err(TsProtoError::QueryError { id: 1033, .. }) => {
                    let vcmd =
                        build_command("use", &["virtual"], &[("sid", &sid.to_string())], &[]);
                    self.send_raw(&vcmd, pacer, events_tx).await?;
                    self.active_sid = Some(sid);
                    self.notify_registered = false;
                }
                Err(e) => return Err(e),
            }
        }

        if enable_notify && !self.notify_registered && sid == default_sid {
            for reg_cmd in [
                "servernotifyregister event=server\n",
                "servernotifyregister event=channel id=0\n",
            ] {
                match self.send_raw(reg_cmd, pacer, events_tx).await {
                    Ok(_) => {}
                    Err(TsProtoError::QueryError { id, .. }) if id != 524 && id != 3329 => {
                        // Restricted Query accounts may lack notify permission; continue without failing
                        break;
                    }
                    Err(e) => return Err(e),
                }
            }
            self.notify_registered = true;
        }

        Ok(())
    }
}

struct DriverCommand {
    sid: Option<u32>,
    cmd_line: String,
    reply_tx: oneshot::Sender<Result<Vec<RecordMap>>>,
}

pub struct ServerQueryDriver {
    cmd_tx: mpsc::Sender<DriverCommand>,
    events_tx: broadcast::Sender<TsNotifyEvent>,
    shutdown_tx: watch::Sender<bool>,
    enable_notify: bool,
}

impl ServerQueryDriver {
    pub fn new(
        connector: Arc<dyn QueryStreamConnector>,
        default_sid: u32,
        start_notify: bool,
    ) -> Self {
        let (events_tx, _) = broadcast::channel(256);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (cmd_tx, cmd_rx) = mpsc::channel(64);

        {
            let events_tx = events_tx.clone();
            tokio::spawn(async move {
                Self::run_session_actor(
                    connector,
                    default_sid,
                    start_notify,
                    cmd_rx,
                    events_tx,
                    shutdown_rx,
                )
                .await;
            });
        }

        Self {
            cmd_tx,
            events_tx,
            shutdown_tx,
            enable_notify: start_notify,
        }
    }

    async fn run_session_actor(
        connector: Arc<dyn QueryStreamConnector>,
        default_sid: u32,
        enable_notify: bool,
        mut cmd_rx: mpsc::Receiver<DriverCommand>,
        events_tx: broadcast::Sender<TsNotifyEvent>,
        mut shutdown_rx: watch::Receiver<bool>,
    ) {
        let mut conn: Option<QueryConnection> = None;
        let mut pacer = FloodPacer::new();
        let mut keepalive_tick = tokio::time::interval(Duration::from_secs(30));
        keepalive_tick.tick().await;

        loop {
            tokio::select! {
                biased;

                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        if let Some(mut c) = conn.take() {
                            let _ = timeout(Duration::from_secs(2), c.writer.write_all(b"quit\n")).await;
                            let _ = timeout(Duration::from_secs(1), c.writer.flush()).await;
                        }
                        break;
                    }
                }

                maybe_req = cmd_rx.recv() => {
                    let Some(req) = maybe_req else {
                        if let Some(mut c) = conn.take() {
                            let _ = timeout(Duration::from_secs(2), c.writer.write_all(b"quit\n")).await;
                        }
                        break;
                    };
                    let res = Self::execute_on_actor(
                        &mut conn,
                        &*connector,
                        default_sid,
                        enable_notify,
                        &mut pacer,
                        &events_tx,
                        req.sid,
                        &req.cmd_line,
                    )
                    .await;
                    let _ = req.reply_tx.send(res);
                }

                idle_res = async {
                    match conn.as_mut() {
                        Some(c) => c.read_raw_line().await,
                        None => std::future::pending().await,
                    }
                } => {
                    match idle_res {
                        Ok(line) => {
                            let trimmed = line.trim();
                            if !trimmed.is_empty() {
                                if let Some(evt) = parse_notify_line(trimmed) {
                                    let _ = events_tx.send(evt);
                                } else if let Some(status) = parse_error_line(trimmed) {
                                    if status.id == 524 || status.id == 3329 {
                                        warn!("TeamSpeak dropped idle session (id={}): {}", status.id, status.msg);
                                        conn = None;
                                    }
                                }
                            }
                        }
                        Err(_) => {
                            // Server closed idle connection; drop immediately so next command reconnects cleanly
                            conn = None;
                        }
                    }
                }

                _ = keepalive_tick.tick() => {
                    if let Some(c) = conn.as_mut() {
                        if c.last_io.elapsed() >= Duration::from_secs(110) {
                            if c.send_raw("whoami\n", &mut pacer, &events_tx).await.is_err() {
                                conn = None;
                            }
                        }
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_on_actor(
        conn_slot: &mut Option<QueryConnection>,
        connector: &dyn QueryStreamConnector,
        default_sid: u32,
        enable_notify: bool,
        pacer: &mut FloodPacer,
        events_tx: &broadcast::Sender<TsNotifyEvent>,
        sid: Option<u32>,
        cmd_line: &str,
    ) -> Result<Vec<RecordMap>> {
        for attempt in 0..2 {
            let had_existing_conn = conn_slot.is_some();
            if conn_slot.is_none() {
                let fresh = QueryConnection::connect(connector, pacer, events_tx).await?;
                *conn_slot = Some(fresh);
            }

            let res: Result<Vec<RecordMap>> = async {
                let conn = conn_slot.as_mut().expect("connected");
                if let Some(target_sid) = sid {
                    conn.ensure_sid(target_sid, default_sid, enable_notify, pacer, events_tx)
                        .await?;
                }
                conn.send_raw(cmd_line, pacer, events_tx).await
            }
            .await;

            match res {
                Ok(recs) => return Ok(recs),
                Err(e) => {
                    let is_flood = matches!(&e, TsProtoError::QueryError { id: 524, .. });
                    let is_transport = matches!(
                        &e,
                        TsProtoError::Io(_)
                            | TsProtoError::ConnectionClosed
                            | TsProtoError::Timeout
                            | TsProtoError::Ssh(_)
                    );
                    if is_transport || is_flood {
                        *conn_slot = None;
                    }
                    if is_flood && attempt == 0 {
                        tokio::time::sleep(Duration::from_millis(1600)).await;
                        continue;
                    }
                    if is_transport && had_existing_conn && attempt == 0 {
                        // Existing idle socket was closed by server/NAT; transparently reconnect once
                        continue;
                    }
                    return Err(e);
                }
            }
        }
        Err(TsProtoError::ConnectionClosed)
    }

    async fn exec(
        &self,
        sid: Option<u32>,
        cmd: &str,
        options: &[&str],
        params: &[(&str, &str)],
    ) -> Result<Vec<RecordMap>> {
        let cmd_line = build_command(cmd, options, params, &[]);
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(DriverCommand {
                sid,
                cmd_line,
                reply_tx,
            })
            .await
            .map_err(|_| TsProtoError::ConnectionClosed)?;

        timeout(Duration::from_secs(25), reply_rx)
            .await
            .map_err(|_| TsProtoError::Timeout)?
            .map_err(|_| TsProtoError::ConnectionClosed)?
    }
}

fn flag_str(b: bool) -> &'static str {
    if b { "1" } else { "0" }
}

#[async_trait]
impl TsProtocolDriver for ServerQueryDriver {
    fn supports_push_events(&self) -> bool {
        self.enable_notify
    }

    fn subscribe_events(&self) -> broadcast::Receiver<TsNotifyEvent> {
        self.events_tx.subscribe()
    }

    async fn test_connection(&self) -> Result<()> {
        self.exec(None, "version", &[], &[]).await?;
        Ok(())
    }

    async fn list_virtual_servers(&self) -> Result<Vec<VirtualServerSummary>> {
        let recs = self.exec(None, "serverlist", &[], &[]).await?;
        Ok(recs.iter().map(map_virtual_server).collect())
    }

    async fn start_virtual_server(&self, sid: u32) -> Result<()> {
        let s = sid.to_string();
        self.exec(None, "serverstart", &[], &[("sid", &s)]).await?;
        Ok(())
    }

    async fn stop_virtual_server(&self, sid: u32) -> Result<()> {
        let s = sid.to_string();
        self.exec(None, "serverstop", &[], &[("sid", &s)]).await?;
        Ok(())
    }

    async fn create_virtual_server(
        &self,
        name: &str,
        max_clients: u32,
        port: Option<u16>,
    ) -> Result<u32> {
        let max_s = max_clients.to_string();
        let port_s = port.map(|p| p.to_string());
        let mut params = vec![
            ("virtualserver_name", name),
            ("virtualserver_maxclients", max_s.as_str()),
        ];
        if let Some(ps) = port_s.as_deref() {
            params.push(("virtualserver_port", ps));
        }
        let recs = self.exec(None, "servercreate", &[], &params).await?;
        Ok(recs
            .first()
            .and_then(|r| r.get("sid"))
            .and_then(|s| s.parse().ok())
            .unwrap_or(1))
    }

    async fn delete_virtual_server(&self, sid: u32) -> Result<()> {
        let s = sid.to_string();
        self.exec(None, "serverdelete", &[], &[("sid", &s)]).await?;
        Ok(())
    }

    async fn get_server_info(&self, sid: u32) -> Result<ServerInfoSnapshot> {
        let recs = self.exec(Some(sid), "serverinfo", &[], &[]).await?;
        let rec = recs.first().ok_or_else(|| {
            TsProtoError::Parse("serverinfo returned empty response".to_string())
        })?;
        Ok(map_server_info(sid, rec))
    }

    async fn edit_server_info(&self, sid: u32, p: &EditServerInfoParams) -> Result<()> {
        let max_s = p.max_clients.map(|v| v.to_string());
        let mut params: Vec<(&str, &str)> = Vec::new();
        if let Some(v) = p.name.as_deref() {
            params.push(("virtualserver_name", v));
        }
        if let Some(v) = p.welcome_message.as_deref() {
            params.push(("virtualserver_welcomemessage", v));
        }
        if let Some(v) = max_s.as_deref() {
            params.push(("virtualserver_maxclients", v));
        }
        if let Some(v) = p.password.as_deref() {
            params.push(("virtualserver_password", v));
        }
        if let Some(v) = p.hostbanner_url.as_deref() {
            params.push(("virtualserver_hostbanner_url", v));
        }
        if let Some(v) = p.hostbanner_gfx_url.as_deref() {
            params.push(("virtualserver_hostbanner_gfx_url", v));
        }
        if let Some(v) = p.hostbutton_tooltip.as_deref() {
            params.push(("virtualserver_hostbutton_tooltip", v));
        }
        if let Some(v) = p.hostbutton_url.as_deref() {
            params.push(("virtualserver_hostbutton_url", v));
        }
        if !params.is_empty() {
            self.exec(Some(sid), "serveredit", &[], &params).await?;
        }
        Ok(())
    }

    async fn list_channels(&self, sid: u32) -> Result<Vec<ChannelNode>> {
        let recs = self
            .exec(
                Some(sid),
                "channellist",
                &["topic", "flags", "voice", "limits"],
                &[],
            )
            .await?;
        Ok(recs.iter().map(map_channel).collect())
    }

    async fn create_channel(&self, sid: u32, p: &CreateChannelParams) -> Result<u32> {
        let pid_s = p.parent_id.map(|v| v.to_string());
        let max_s = p.max_clients.map(|v| v.to_string());
        let codec_s = p.codec.map(|v| v.to_string());
        let cq_s = p.codec_quality.map(|v| v.to_string());
        let tp_s = p.needed_talk_power.map(|v| v.to_string());
        let ord_s = p.order.map(|v| v.to_string());

        let mut params = vec![("channel_name", p.name.as_str())];
        if let Some(v) = pid_s.as_deref() {
            params.push(("cpid", v));
        }
        if let Some(v) = p.topic.as_deref() {
            params.push(("channel_topic", v));
        }
        if let Some(v) = p.description.as_deref() {
            params.push(("channel_description", v));
        }
        if let Some(v) = p.password.as_deref() {
            params.push(("channel_password", v));
        }
        if let Some(v) = max_s.as_deref() {
            params.push(("channel_maxclients", v));
        }
        if let Some(b) = p.is_permanent {
            params.push(("channel_flag_permanent", flag_str(b)));
        }
        if let Some(b) = p.is_semi_permanent {
            params.push(("channel_flag_semi_permanent", flag_str(b)));
        }
        if let Some(b) = p.is_default {
            params.push(("channel_flag_default", flag_str(b)));
        }
        if let Some(v) = codec_s.as_deref() {
            params.push(("channel_codec", v));
        }
        if let Some(v) = cq_s.as_deref() {
            params.push(("channel_codec_quality", v));
        }
        if let Some(v) = tp_s.as_deref() {
            params.push(("channel_needed_talk_power", v));
        }
        if let Some(v) = ord_s.as_deref() {
            params.push(("channel_order", v));
        }

        let recs = self.exec(Some(sid), "channelcreate", &[], &params).await?;
        Ok(recs
            .first()
            .and_then(|r| r.get("cid"))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0))
    }

    async fn edit_channel(&self, sid: u32, cid: u32, p: &EditChannelParams) -> Result<()> {
        let cid_s = cid.to_string();
        let max_s = p.max_clients.map(|v| v.to_string());
        let codec_s = p.codec.map(|v| v.to_string());
        let cq_s = p.codec_quality.map(|v| v.to_string());
        let tp_s = p.needed_talk_power.map(|v| v.to_string());
        let ord_s = p.order.map(|v| v.to_string());

        let mut params = vec![("cid", cid_s.as_str())];
        if let Some(v) = p.name.as_deref() {
            params.push(("channel_name", v));
        }
        if let Some(v) = p.topic.as_deref() {
            params.push(("channel_topic", v));
        }
        if let Some(v) = p.description.as_deref() {
            params.push(("channel_description", v));
        }
        if let Some(v) = p.password.as_deref() {
            params.push(("channel_password", v));
        }
        if let Some(v) = max_s.as_deref() {
            params.push(("channel_maxclients", v));
        }
        if let Some(b) = p.is_permanent {
            params.push(("channel_flag_permanent", flag_str(b)));
        }
        if let Some(b) = p.is_semi_permanent {
            params.push(("channel_flag_semi_permanent", flag_str(b)));
        }
        if let Some(b) = p.is_default {
            params.push(("channel_flag_default", flag_str(b)));
        }
        if let Some(v) = codec_s.as_deref() {
            params.push(("channel_codec", v));
        }
        if let Some(v) = cq_s.as_deref() {
            params.push(("channel_codec_quality", v));
        }
        if let Some(v) = tp_s.as_deref() {
            params.push(("channel_needed_talk_power", v));
        }
        if let Some(v) = ord_s.as_deref() {
            params.push(("channel_order", v));
        }
        self.exec(Some(sid), "channeledit", &[], &params).await?;
        Ok(())
    }

    async fn move_channel(&self, sid: u32, cid: u32, cpid: u32, order: u32) -> Result<()> {
        let cid_s = cid.to_string();
        let cpid_s = cpid.to_string();
        let ord_s = order.to_string();
        self.exec(
            Some(sid),
            "channelmove",
            &[],
            &[("cid", &cid_s), ("cpid", &cpid_s), ("order", &ord_s)],
        )
        .await?;
        Ok(())
    }

    async fn delete_channel(&self, sid: u32, cid: u32, force: bool) -> Result<()> {
        let cid_s = cid.to_string();
        self.exec(
            Some(sid),
            "channeldelete",
            &[],
            &[("cid", &cid_s), ("force", flag_str(force))],
        )
        .await?;
        Ok(())
    }

    async fn list_clients(&self, sid: u32) -> Result<Vec<OnlineClient>> {
        let recs = self
            .exec(
                Some(sid),
                "clientlist",
                &["uid", "away", "voice", "times", "groups", "info", "country", "ip"],
                &[],
            )
            .await?;
        Ok(recs.iter().map(map_client).collect())
    }

    async fn kick_client(
        &self,
        sid: u32,
        clid: u32,
        from_server: bool,
        reason: Option<&str>,
    ) -> Result<()> {
        let clid_s = clid.to_string();
        let reason_id = if from_server { "5" } else { "4" };
        let mut params = vec![("clid", clid_s.as_str()), ("reasonid", reason_id)];
        if let Some(r) = reason {
            params.push(("reasonmsg", r));
        }
        self.exec(Some(sid), "clientkick", &[], &params).await?;
        Ok(())
    }

    async fn move_client(
        &self,
        sid: u32,
        clid: u32,
        cid: u32,
        password: Option<&str>,
    ) -> Result<()> {
        let clid_s = clid.to_string();
        let cid_s = cid.to_string();
        let mut params = vec![("clid", clid_s.as_str()), ("cid", cid_s.as_str())];
        if let Some(pw) = password {
            params.push(("cpw", pw));
        }
        self.exec(Some(sid), "clientmove", &[], &params).await?;
        Ok(())
    }

    async fn poke_client(&self, sid: u32, clid: u32, msg: &str) -> Result<()> {
        let clid_s = clid.to_string();
        self.exec(
            Some(sid),
            "clientpoke",
            &[],
            &[("clid", &clid_s), ("msg", msg)],
        )
        .await?;
        Ok(())
    }

    async fn ban_client(
        &self,
        sid: u32,
        clid: u32,
        duration_secs: i64,
        reason: Option<&str>,
    ) -> Result<()> {
        let clid_s = clid.to_string();
        let time_s = duration_secs.to_string();
        let mut params = vec![("clid", clid_s.as_str()), ("time", time_s.as_str())];
        if let Some(r) = reason {
            params.push(("banreason", r));
        }
        self.exec(Some(sid), "banclient", &[], &params).await?;
        Ok(())
    }

    async fn send_text_message(
        &self,
        sid: u32,
        target_mode: u8,
        target: u32,
        msg: &str,
    ) -> Result<()> {
        let tm_s = target_mode.to_string();
        let t_s = target.to_string();
        self.exec(
            Some(sid),
            "sendtextmessage",
            &[],
            &[("targetmode", &tm_s), ("target", &t_s), ("msg", msg)],
        )
        .await?;
        Ok(())
    }

    async fn list_server_groups(&self, sid: u32) -> Result<Vec<ServerGroup>> {
        let recs = self.exec(Some(sid), "servergrouplist", &[], &[]).await?;
        Ok(recs.iter().map(map_server_group).collect())
    }

    async fn create_server_group(
        &self,
        sid: u32,
        name: &str,
        group_type: Option<u8>,
    ) -> Result<u32> {
        let t_s = group_type.map(|t| t.to_string());
        let mut params = vec![("name", name)];
        if let Some(t) = t_s.as_deref() {
            params.push(("type", t));
        }
        let recs = self.exec(Some(sid), "servergroupadd", &[], &params).await?;
        Ok(recs
            .first()
            .and_then(|r| r.get("sgid"))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0))
    }

    async fn rename_server_group(&self, sid: u32, sgid: u32, name: &str) -> Result<()> {
        let sg_s = sgid.to_string();
        self.exec(
            Some(sid),
            "servergrouprename",
            &[],
            &[("sgid", &sg_s), ("name", name)],
        )
        .await?;
        Ok(())
    }

    async fn delete_server_group(&self, sid: u32, sgid: u32, force: bool) -> Result<()> {
        let sg_s = sgid.to_string();
        self.exec(
            Some(sid),
            "servergroupdel",
            &[],
            &[("sgid", &sg_s), ("force", flag_str(force))],
        )
        .await?;
        Ok(())
    }

    async fn list_server_group_clients(&self, sid: u32, sgid: u32) -> Result<Vec<GroupMember>> {
        let sg_s = sgid.to_string();
        match self
            .exec(Some(sid), "servergroupclientlist", &["names"], &[("sgid", &sg_s)])
            .await
        {
            Ok(recs) => Ok(recs.iter().map(map_group_member).collect()),
            Err(TsProtoError::QueryError { id: 1281, .. }) => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    async fn add_client_to_server_group(&self, sid: u32, sgid: u32, cldbid: u32) -> Result<()> {
        let sg_s = sgid.to_string();
        let db_s = cldbid.to_string();
        self.exec(
            Some(sid),
            "servergroupaddclient",
            &[],
            &[("sgid", &sg_s), ("cldbid", &db_s)],
        )
        .await?;
        Ok(())
    }

    async fn remove_client_from_server_group(
        &self,
        sid: u32,
        sgid: u32,
        cldbid: u32,
    ) -> Result<()> {
        let sg_s = sgid.to_string();
        let db_s = cldbid.to_string();
        self.exec(
            Some(sid),
            "servergroupdelclient",
            &[],
            &[("sgid", &sg_s), ("cldbid", &db_s)],
        )
        .await?;
        Ok(())
    }

    async fn list_server_group_permissions(
        &self,
        sid: u32,
        sgid: u32,
    ) -> Result<Vec<PermissionEntry>> {
        let sg_s = sgid.to_string();
        match self
            .exec(Some(sid), "servergrouppermlist", &["permsid"], &[("sgid", &sg_s)])
            .await
        {
            Ok(recs) => Ok(recs.iter().map(map_permission).collect()),
            Err(TsProtoError::QueryError { id: 1281, .. }) => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    async fn set_server_group_permission(
        &self,
        sid: u32,
        sgid: u32,
        perm: &SetPermissionParams,
    ) -> Result<()> {
        let sg_s = sgid.to_string();
        let val_s = perm.permvalue.to_string();
        self.exec(
            Some(sid),
            "servergroupaddperm",
            &[],
            &[
                ("sgid", &sg_s),
                ("permsid", &perm.permname),
                ("permvalue", &val_s),
                ("permnegated", flag_str(perm.permnegated)),
                ("permskip", flag_str(perm.permskip)),
            ],
        )
        .await?;
        Ok(())
    }

    async fn remove_server_group_permission(
        &self,
        sid: u32,
        sgid: u32,
        permname: &str,
    ) -> Result<()> {
        let sg_s = sgid.to_string();
        self.exec(
            Some(sid),
            "servergroupdelperm",
            &[],
            &[("sgid", &sg_s), ("permsid", permname)],
        )
        .await?;
        Ok(())
    }

    async fn list_channel_groups(&self, sid: u32) -> Result<Vec<ChannelGroup>> {
        let recs = self.exec(Some(sid), "channelgrouplist", &[], &[]).await?;
        Ok(recs.iter().map(map_channel_group).collect())
    }

    async fn list_privilege_keys(&self, sid: u32) -> Result<Vec<PrivilegeKey>> {
        match self.exec(Some(sid), "privilegekeylist", &[], &[]).await {
            Ok(recs) => Ok(recs.iter().map(map_privilege_key).collect()),
            Err(TsProtoError::QueryError { id: 1281, .. }) => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    async fn create_privilege_key(
        &self,
        sid: u32,
        p: &CreatePrivilegeKeyParams,
    ) -> Result<String> {
        let tt_s = p.token_type.to_string();
        let id1_s = p.group_id.to_string();
        let id2_s = p.channel_id.unwrap_or(0).to_string();
        let mut params = vec![
            ("tokentype", tt_s.as_str()),
            ("tokenid1", id1_s.as_str()),
            ("tokenid2", id2_s.as_str()),
        ];
        if let Some(d) = p.description.as_deref() {
            params.push(("tokendescription", d));
        }
        let recs = self.exec(Some(sid), "privilegekeyadd", &[], &params).await?;
        Ok(recs
            .first()
            .and_then(|r| r.get("token"))
            .cloned()
            .unwrap_or_default())
    }

    async fn delete_privilege_key(&self, sid: u32, token: &str) -> Result<()> {
        self.exec(Some(sid), "privilegekeydelete", &[], &[("token", token)])
            .await?;
        Ok(())
    }

    async fn list_bans(&self, sid: u32) -> Result<Vec<BanRule>> {
        match self.exec(Some(sid), "banlist", &[], &[]).await {
            Ok(recs) => Ok(recs.iter().map(map_ban_rule).collect()),
            Err(TsProtoError::QueryError { id: 1281, .. }) => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    async fn add_ban(&self, sid: u32, p: &AddBanParams) -> Result<u32> {
        let dur_s = p.duration_secs.map(|d| d.to_string());
        let mut params: Vec<(&str, &str)> = Vec::new();
        if let Some(v) = p.ip.as_deref() {
            params.push(("ip", v));
        }
        if let Some(v) = p.name.as_deref() {
            params.push(("name", v));
        }
        if let Some(v) = p.uid.as_deref() {
            params.push(("uid", v));
        }
        if let Some(v) = dur_s.as_deref() {
            params.push(("time", v));
        }
        if let Some(v) = p.reason.as_deref() {
            params.push(("banreason", v));
        }
        let recs = self.exec(Some(sid), "banadd", &[], &params).await?;
        Ok(recs
            .first()
            .and_then(|r| r.get("banid"))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0))
    }

    async fn delete_ban(&self, sid: u32, banid: u32) -> Result<()> {
        let b_s = banid.to_string();
        self.exec(Some(sid), "bandel", &[], &[("banid", &b_s)])
            .await?;
        Ok(())
    }

    async fn list_db_clients(
        &self,
        sid: u32,
        offset: u32,
        limit: u32,
        pattern: Option<&str>,
    ) -> Result<Vec<DbClient>> {
        if let Some(pat) = pattern.filter(|s| !s.trim().is_empty()) {
            let recs = match self
                .exec(Some(sid), "clientdbfind", &["uid"], &[("pattern", pat)])
                .await
            {
                Ok(r) => r,
                Err(TsProtoError::QueryError { id: 1281, .. }) => return Ok(Vec::new()),
                Err(e) => return Err(e),
            };
            let mut out = Vec::new();
            for r in recs.into_iter().take(limit as usize) {
                if let Some(cldbid) = r.get("cldbid") {
                    if let Ok(info) = self
                        .exec(Some(sid), "clientdbinfo", &[], &[("cldbid", cldbid)])
                        .await
                    {
                        if let Some(item) = info.first() {
                            out.push(map_db_client(item));
                        }
                    }
                }
            }
            return Ok(out);
        }
        let off_s = offset.to_string();
        let lim_s = limit.to_string();
        match self
            .exec(
                Some(sid),
                "clientdblist",
                &["count"],
                &[("start", &off_s), ("duration", &lim_s)],
            )
            .await
        {
            Ok(recs) => Ok(recs.iter().map(map_db_client).collect()),
            Err(TsProtoError::QueryError { id: 1281, .. }) => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    async fn delete_db_client(&self, sid: u32, cldbid: u32) -> Result<()> {
        let id_s = cldbid.to_string();
        self.exec(Some(sid), "clientdbdelete", &[], &[("cldbid", &id_s)])
            .await?;
        Ok(())
    }

    async fn list_complaints(&self, sid: u32) -> Result<Vec<ComplaintEntry>> {
        match self.exec(Some(sid), "complainlist", &[], &[]).await {
            Ok(recs) => Ok(recs.iter().map(map_complaint).collect()),
            Err(TsProtoError::QueryError { id: 1281, .. }) => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    async fn delete_complaint(&self, sid: u32, tcldbid: u32, fcldbid: u32) -> Result<()> {
        let t_s = tcldbid.to_string();
        let f_s = fcldbid.to_string();
        self.exec(
            Some(sid),
            "complaindel",
            &[],
            &[("tcldbid", &t_s), ("fcldbid", &f_s)],
        )
        .await?;
        Ok(())
    }

    async fn view_server_logs(&self, sid: u32, lines: u32) -> Result<Vec<TsLogEntry>> {
        let l_s = lines.to_string();
        match self
            .exec(
                Some(sid),
                "logview",
                &[],
                &[("lines", &l_s), ("reverse", "1")],
            )
            .await
        {
            Ok(recs) => Ok(recs.iter().map(map_log_entry).collect()),
            Err(TsProtoError::QueryError { id: 1281, .. }) => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    async fn create_snapshot(&self, sid: u32) -> Result<TsSnapshotData> {
        let recs = self
            .exec(Some(sid), "serversnapshotcreate", &[], &[])
            .await?;
        let rec = recs.first().ok_or_else(|| {
            TsProtoError::Parse("serversnapshotcreate returned empty response".to_string())
        })?;
        Ok(TsSnapshotData {
            version: rec.get("version").cloned().unwrap_or_else(|| "1".to_string()),
            data: rec.get("data").cloned().unwrap_or_default(),
        })
    }

    async fn deploy_snapshot(&self, sid: u32, snapshot: &TsSnapshotData) -> Result<()> {
        self.exec(
            Some(sid),
            "serversnapshotdeploy",
            &[],
            &[("version", &snapshot.version), ("data", &snapshot.data)],
        )
        .await?;
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        let _ = self.shutdown_tx.send(true);
        Ok(())
    }
}
