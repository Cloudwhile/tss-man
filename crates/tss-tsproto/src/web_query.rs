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
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;
use tokio::sync::broadcast;

#[derive(Debug, Deserialize)]
struct WebQueryEnvelope {
    status: WebQueryStatus,
    #[serde(default)]
    body: Option<Vec<Value>>,
}

#[derive(Debug, Deserialize)]
struct WebQueryStatus {
    code: u32,
    message: String,
    #[serde(default)]
    extra_message: Option<String>,
}

pub struct WebQueryDriver {
    client: Client,
    base_url: String,
    api_key: String,
    events_tx: broadcast::Sender<TsNotifyEvent>,
}

impl WebQueryDriver {
    pub fn new(config: &TsConnectionConfig) -> Result<Self> {
        let client = Client::builder()
            .danger_accept_invalid_certs(!config.tls_verify)
            .timeout(Duration::from_secs(12))
            .build()?;

        let raw_host = config.host.trim();
        let base_url = if raw_host.starts_with("http://") || raw_host.starts_with("https://") {
            let trimmed = raw_host.trim_end_matches('/');
            if trimmed.rsplit(':').next().and_then(|p| p.parse::<u16>().ok()).is_some() {
                trimmed.to_string()
            } else {
                format!("{trimmed}:{}", config.port)
            }
        } else {
            let scheme = if config.port == 10443 || config.port == 443 {
                "https"
            } else {
                "http"
            };
            format!("{scheme}://{raw_host}:{}", config.port)
        };

        let (events_tx, _) = broadcast::channel(64);
        Ok(Self {
            client,
            base_url,
            api_key: config.secret.clone(),
            events_tx,
        })
    }

    async fn call(
        &self,
        sid: Option<u32>,
        cmd: &str,
        params: Vec<(&str, String)>,
    ) -> Result<Vec<RecordMap>> {
        let url = match sid {
            Some(s) => format!("{}/{s}/{cmd}", self.base_url),
            None => format!("{}/{cmd}", self.base_url),
        };
        let mut req = self
            .client
            .get(&url)
            .header("x-api-key", &self.api_key);
        if !params.is_empty() {
            req = req.query(&params);
        }
        let resp = req.send().await?;
        let status_code = resp.status();
        let text = resp.text().await?;
        if !status_code.is_success() && text.trim().is_empty() {
            return Err(TsProtoError::Parse(format!(
                "WebQuery HTTP {status_code} on {cmd}"
            )));
        }
        let env: WebQueryEnvelope = serde_json::from_str(&text)
            .map_err(|e| TsProtoError::Parse(format!("Invalid WebQuery JSON: {e}: {text}")))?;
        if env.status.code != 0 {
            return Err(TsProtoError::QueryError {
                id: env.status.code,
                msg: env.status.message,
                extra_msg: env.status.extra_message,
            });
        }
        let rows = env.body.unwrap_or_default();
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            if let Value::Object(map) = row {
                let mut rec = RecordMap::new();
                for (k, v) in map {
                    let s = match v {
                        Value::Null => String::new(),
                        Value::Bool(b) => if b { "1" } else { "0" }.to_string(),
                        Value::Number(n) => n.to_string(),
                        Value::String(s) => s,
                        other => other.to_string(),
                    };
                    rec.insert(k, s);
                }
                out.push(rec);
            }
        }
        Ok(out)
    }
}

fn opt_flag(val: bool) -> String {
    if val { "1" } else { "0" }.to_string()
}

#[async_trait]
impl TsProtocolDriver for WebQueryDriver {
    fn supports_push_events(&self) -> bool {
        false
    }

    fn subscribe_events(&self) -> broadcast::Receiver<TsNotifyEvent> {
        self.events_tx.subscribe()
    }

    async fn test_connection(&self) -> Result<()> {
        self.call(None, "version", vec![]).await?;
        Ok(())
    }

    async fn list_virtual_servers(&self) -> Result<Vec<VirtualServerSummary>> {
        let rows = self.call(None, "serverlist", vec![]).await?;
        Ok(rows.iter().map(map_virtual_server).collect())
    }

    async fn start_virtual_server(&self, sid: u32) -> Result<()> {
        self.call(None, "serverstart", vec![("sid", sid.to_string())])
            .await?;
        Ok(())
    }

    async fn stop_virtual_server(&self, sid: u32) -> Result<()> {
        self.call(None, "serverstop", vec![("sid", sid.to_string())])
            .await?;
        Ok(())
    }

    async fn create_virtual_server(
        &self,
        name: &str,
        max_clients: u32,
        port: Option<u16>,
    ) -> Result<u32> {
        let mut params = vec![
            ("virtualserver_name", name.to_string()),
            ("virtualserver_maxclients", max_clients.to_string()),
        ];
        if let Some(p) = port {
            params.push(("virtualserver_port", p.to_string()));
        }
        let rows = self.call(None, "servercreate", params).await?;
        Ok(rows
            .first()
            .and_then(|r| r.get("sid"))
            .and_then(|s| s.parse().ok())
            .unwrap_or(1))
    }

    async fn delete_virtual_server(&self, sid: u32) -> Result<()> {
        self.call(None, "serverdelete", vec![("sid", sid.to_string())])
            .await?;
        Ok(())
    }

    async fn get_server_info(&self, sid: u32) -> Result<ServerInfoSnapshot> {
        let rows = self.call(Some(sid), "serverinfo", vec![]).await?;
        let rec = rows.first().ok_or_else(|| {
            TsProtoError::Parse("WebQuery serverinfo returned empty body".to_string())
        })?;
        Ok(map_server_info(sid, rec))
    }

    async fn edit_server_info(&self, sid: u32, p: &EditServerInfoParams) -> Result<()> {
        let mut params = Vec::new();
        if let Some(v) = &p.name {
            params.push(("virtualserver_name", v.clone()));
        }
        if let Some(v) = &p.welcome_message {
            params.push(("virtualserver_welcomemessage", v.clone()));
        }
        if let Some(v) = p.max_clients {
            params.push(("virtualserver_maxclients", v.to_string()));
        }
        if let Some(v) = &p.password {
            params.push(("virtualserver_password", v.clone()));
        }
        if let Some(v) = &p.hostbanner_url {
            params.push(("virtualserver_hostbanner_url", v.clone()));
        }
        if let Some(v) = &p.hostbanner_gfx_url {
            params.push(("virtualserver_hostbanner_gfx_url", v.clone()));
        }
        if let Some(v) = &p.hostbutton_tooltip {
            params.push(("virtualserver_hostbutton_tooltip", v.clone()));
        }
        if let Some(v) = &p.hostbutton_url {
            params.push(("virtualserver_hostbutton_url", v.clone()));
        }
        self.call(Some(sid), "serveredit", params).await?;
        Ok(())
    }

    async fn list_channels(&self, sid: u32) -> Result<Vec<ChannelNode>> {
        let rows = self
            .call(
                Some(sid),
                "channellist",
                vec![
                    ("-topic", String::new()),
                    ("-flags", String::new()),
                    ("-voice", String::new()),
                    ("-limits", String::new()),
                ],
            )
            .await?;
        Ok(rows.iter().map(map_channel).collect())
    }

    async fn create_channel(&self, sid: u32, p: &CreateChannelParams) -> Result<u32> {
        let mut params = vec![("channel_name", p.name.clone())];
        if let Some(v) = p.parent_id {
            params.push(("cpid", v.to_string()));
        }
        if let Some(v) = &p.topic {
            params.push(("channel_topic", v.clone()));
        }
        if let Some(v) = &p.description {
            params.push(("channel_description", v.clone()));
        }
        if let Some(v) = &p.password {
            params.push(("channel_password", v.clone()));
        }
        if let Some(v) = p.max_clients {
            params.push(("channel_maxclients", v.to_string()));
        }
        if let Some(v) = p.is_permanent {
            params.push(("channel_flag_permanent", opt_flag(v)));
        }
        if let Some(v) = p.is_semi_permanent {
            params.push(("channel_flag_semi_permanent", opt_flag(v)));
        }
        if let Some(v) = p.is_default {
            params.push(("channel_flag_default", opt_flag(v)));
        }
        if let Some(v) = p.codec {
            params.push(("channel_codec", v.to_string()));
        }
        if let Some(v) = p.codec_quality {
            params.push(("channel_codec_quality", v.to_string()));
        }
        if let Some(v) = p.needed_talk_power {
            params.push(("channel_needed_talk_power", v.to_string()));
        }
        if let Some(v) = p.order {
            params.push(("channel_order", v.to_string()));
        }
        let rows = self.call(Some(sid), "channelcreate", params).await?;
        Ok(rows
            .first()
            .and_then(|r| r.get("cid"))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0))
    }

    async fn edit_channel(&self, sid: u32, cid: u32, p: &EditChannelParams) -> Result<()> {
        let mut params = vec![("cid", cid.to_string())];
        if let Some(v) = &p.name {
            params.push(("channel_name", v.clone()));
        }
        if let Some(v) = &p.topic {
            params.push(("channel_topic", v.clone()));
        }
        if let Some(v) = &p.description {
            params.push(("channel_description", v.clone()));
        }
        if let Some(v) = &p.password {
            params.push(("channel_password", v.clone()));
        }
        if let Some(v) = p.max_clients {
            params.push(("channel_maxclients", v.to_string()));
        }
        if let Some(v) = p.is_permanent {
            params.push(("channel_flag_permanent", opt_flag(v)));
        }
        if let Some(v) = p.is_semi_permanent {
            params.push(("channel_flag_semi_permanent", opt_flag(v)));
        }
        if let Some(v) = p.is_default {
            params.push(("channel_flag_default", opt_flag(v)));
        }
        if let Some(v) = p.codec {
            params.push(("channel_codec", v.to_string()));
        }
        if let Some(v) = p.codec_quality {
            params.push(("channel_codec_quality", v.to_string()));
        }
        if let Some(v) = p.needed_talk_power {
            params.push(("channel_needed_talk_power", v.to_string()));
        }
        if let Some(v) = p.order {
            params.push(("channel_order", v.to_string()));
        }
        self.call(Some(sid), "channeledit", params).await?;
        Ok(())
    }

    async fn move_channel(&self, sid: u32, cid: u32, cpid: u32, order: u32) -> Result<()> {
        self.call(
            Some(sid),
            "channelmove",
            vec![
                ("cid", cid.to_string()),
                ("cpid", cpid.to_string()),
                ("order", order.to_string()),
            ],
        )
        .await?;
        Ok(())
    }

    async fn delete_channel(&self, sid: u32, cid: u32, force: bool) -> Result<()> {
        self.call(
            Some(sid),
            "channeldelete",
            vec![("cid", cid.to_string()), ("force", opt_flag(force))],
        )
        .await?;
        Ok(())
    }

    async fn list_clients(&self, sid: u32) -> Result<Vec<OnlineClient>> {
        let rows = self
            .call(
                Some(sid),
                "clientlist",
                vec![
                    ("-uid", String::new()),
                    ("-away", String::new()),
                    ("-voice", String::new()),
                    ("-times", String::new()),
                    ("-groups", String::new()),
                    ("-info", String::new()),
                    ("-country", String::new()),
                    ("-ip", String::new()),
                ],
            )
            .await?;
        Ok(rows.iter().map(map_client).collect())
    }

    async fn kick_client(
        &self,
        sid: u32,
        clid: u32,
        from_server: bool,
        reason: Option<&str>,
    ) -> Result<()> {
        let mut params = vec![
            ("clid", clid.to_string()),
            ("reasonid", if from_server { "5" } else { "4" }.to_string()),
        ];
        if let Some(r) = reason {
            params.push(("reasonmsg", r.to_string()));
        }
        self.call(Some(sid), "clientkick", params).await?;
        Ok(())
    }

    async fn move_client(
        &self,
        sid: u32,
        clid: u32,
        cid: u32,
        password: Option<&str>,
    ) -> Result<()> {
        let mut params = vec![("clid", clid.to_string()), ("cid", cid.to_string())];
        if let Some(pw) = password {
            params.push(("cpw", pw.to_string()));
        }
        self.call(Some(sid), "clientmove", params).await?;
        Ok(())
    }

    async fn poke_client(&self, sid: u32, clid: u32, msg: &str) -> Result<()> {
        self.call(
            Some(sid),
            "clientpoke",
            vec![("clid", clid.to_string()), ("msg", msg.to_string())],
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
        let mut params = vec![
            ("clid", clid.to_string()),
            ("time", duration_secs.to_string()),
        ];
        if let Some(r) = reason {
            params.push(("banreason", r.to_string()));
        }
        self.call(Some(sid), "banclient", params).await?;
        Ok(())
    }

    async fn send_text_message(
        &self,
        sid: u32,
        target_mode: u8,
        target: u32,
        msg: &str,
    ) -> Result<()> {
        self.call(
            Some(sid),
            "sendtextmessage",
            vec![
                ("targetmode", target_mode.to_string()),
                ("target", target.to_string()),
                ("msg", msg.to_string()),
            ],
        )
        .await?;
        Ok(())
    }

    async fn list_server_groups(&self, sid: u32) -> Result<Vec<ServerGroup>> {
        let rows = self.call(Some(sid), "servergrouplist", vec![]).await?;
        Ok(rows.iter().map(map_server_group).collect())
    }

    async fn create_server_group(
        &self,
        sid: u32,
        name: &str,
        group_type: Option<u8>,
    ) -> Result<u32> {
        let mut params = vec![("name", name.to_string())];
        if let Some(t) = group_type {
            params.push(("type", t.to_string()));
        }
        let rows = self.call(Some(sid), "servergroupadd", params).await?;
        Ok(rows
            .first()
            .and_then(|r| r.get("sgid"))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0))
    }

    async fn rename_server_group(&self, sid: u32, sgid: u32, name: &str) -> Result<()> {
        self.call(
            Some(sid),
            "servergrouprename",
            vec![("sgid", sgid.to_string()), ("name", name.to_string())],
        )
        .await?;
        Ok(())
    }

    async fn delete_server_group(&self, sid: u32, sgid: u32, force: bool) -> Result<()> {
        self.call(
            Some(sid),
            "servergroupdel",
            vec![("sgid", sgid.to_string()), ("force", opt_flag(force))],
        )
        .await?;
        Ok(())
    }

    async fn list_server_group_clients(&self, sid: u32, sgid: u32) -> Result<Vec<GroupMember>> {
        let rows = self
            .call(
                Some(sid),
                "servergroupclientlist",
                vec![("sgid", sgid.to_string()), ("-names", String::new())],
            )
            .await?;
        Ok(rows.iter().map(map_group_member).collect())
    }

    async fn add_client_to_server_group(&self, sid: u32, sgid: u32, cldbid: u32) -> Result<()> {
        self.call(
            Some(sid),
            "servergroupaddclient",
            vec![("sgid", sgid.to_string()), ("cldbid", cldbid.to_string())],
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
        self.call(
            Some(sid),
            "servergroupdelclient",
            vec![("sgid", sgid.to_string()), ("cldbid", cldbid.to_string())],
        )
        .await?;
        Ok(())
    }

    async fn list_server_group_permissions(
        &self,
        sid: u32,
        sgid: u32,
    ) -> Result<Vec<PermissionEntry>> {
        let rows = self
            .call(
                Some(sid),
                "servergrouppermlist",
                vec![("sgid", sgid.to_string()), ("-permsid", String::new())],
            )
            .await?;
        Ok(rows.iter().map(map_permission).collect())
    }

    async fn set_server_group_permission(
        &self,
        sid: u32,
        sgid: u32,
        perm: &SetPermissionParams,
    ) -> Result<()> {
        self.call(
            Some(sid),
            "servergroupaddperm",
            vec![
                ("sgid", sgid.to_string()),
                ("permsid", perm.permname.clone()),
                ("permvalue", perm.permvalue.to_string()),
                ("permnegated", opt_flag(perm.permnegated)),
                ("permskip", opt_flag(perm.permskip)),
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
        self.call(
            Some(sid),
            "servergroupdelperm",
            vec![("sgid", sgid.to_string()), ("permsid", permname.to_string())],
        )
        .await?;
        Ok(())
    }

    async fn list_channel_groups(&self, sid: u32) -> Result<Vec<ChannelGroup>> {
        let rows = self.call(Some(sid), "channelgrouplist", vec![]).await?;
        Ok(rows.iter().map(map_channel_group).collect())
    }

    async fn list_privilege_keys(&self, sid: u32) -> Result<Vec<PrivilegeKey>> {
        match self.call(Some(sid), "privilegekeylist", vec![]).await {
            Ok(rows) => Ok(rows.iter().map(map_privilege_key).collect()),
            Err(TsProtoError::QueryError { id: 1281, .. }) => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    async fn create_privilege_key(
        &self,
        sid: u32,
        p: &CreatePrivilegeKeyParams,
    ) -> Result<String> {
        let mut params = vec![
            ("tokentype", p.token_type.to_string()),
            ("tokenid1", p.group_id.to_string()),
            ("tokenid2", p.channel_id.unwrap_or(0).to_string()),
        ];
        if let Some(d) = &p.description {
            params.push(("tokendescription", d.clone()));
        }
        let rows = self.call(Some(sid), "privilegekeyadd", params).await?;
        Ok(rows
            .first()
            .and_then(|r| r.get("token"))
            .cloned()
            .unwrap_or_default())
    }

    async fn delete_privilege_key(&self, sid: u32, token: &str) -> Result<()> {
        self.call(
            Some(sid),
            "privilegekeydelete",
            vec![("token", token.to_string())],
        )
        .await?;
        Ok(())
    }

    async fn list_bans(&self, sid: u32) -> Result<Vec<BanRule>> {
        match self.call(Some(sid), "banlist", vec![]).await {
            Ok(rows) => Ok(rows.iter().map(map_ban_rule).collect()),
            Err(TsProtoError::QueryError { id: 1281, .. }) => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    async fn add_ban(&self, sid: u32, p: &AddBanParams) -> Result<u32> {
        let mut params = Vec::new();
        if let Some(v) = &p.ip {
            params.push(("ip", v.clone()));
        }
        if let Some(v) = &p.name {
            params.push(("name", v.clone()));
        }
        if let Some(v) = &p.uid {
            params.push(("uid", v.clone()));
        }
        if let Some(v) = p.duration_secs {
            params.push(("time", v.to_string()));
        }
        if let Some(v) = &p.reason {
            params.push(("banreason", v.clone()));
        }
        let rows = self.call(Some(sid), "banadd", params).await?;
        Ok(rows
            .first()
            .and_then(|r| r.get("banid"))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0))
    }

    async fn delete_ban(&self, sid: u32, banid: u32) -> Result<()> {
        self.call(Some(sid), "bandel", vec![("banid", banid.to_string())])
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
            let rows = match self
                .call(
                    Some(sid),
                    "clientdbfind",
                    vec![("pattern", pat.to_string()), ("-uid", String::new())],
                )
                .await
            {
                Ok(r) => r,
                Err(TsProtoError::QueryError { id: 1281, .. }) => return Ok(Vec::new()),
                Err(e) => return Err(e),
            };
            let mut out = Vec::new();
            for r in rows.into_iter().take(limit as usize) {
                if let Some(cldbid) = r.get("cldbid").and_then(|s| s.parse::<u32>().ok()) {
                    if let Ok(info) = self
                        .call(Some(sid), "clientdbinfo", vec![("cldbid", cldbid.to_string())])
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
        match self
            .call(
                Some(sid),
                "clientdblist",
                vec![
                    ("start", offset.to_string()),
                    ("duration", limit.to_string()),
                    ("-count", String::new()),
                ],
            )
            .await
        {
            Ok(rows) => Ok(rows.iter().map(map_db_client).collect()),
            Err(TsProtoError::QueryError { id: 1281, .. }) => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    async fn delete_db_client(&self, sid: u32, cldbid: u32) -> Result<()> {
        self.call(
            Some(sid),
            "clientdbdelete",
            vec![("cldbid", cldbid.to_string())],
        )
        .await?;
        Ok(())
    }

    async fn list_complaints(&self, sid: u32) -> Result<Vec<ComplaintEntry>> {
        match self.call(Some(sid), "complainlist", vec![]).await {
            Ok(rows) => Ok(rows.iter().map(map_complaint).collect()),
            Err(TsProtoError::QueryError { id: 1281, .. }) => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    async fn delete_complaint(&self, sid: u32, tcldbid: u32, fcldbid: u32) -> Result<()> {
        self.call(
            Some(sid),
            "complaindel",
            vec![
                ("tcldbid", tcldbid.to_string()),
                ("fcldbid", fcldbid.to_string()),
            ],
        )
        .await?;
        Ok(())
    }

    async fn view_server_logs(&self, sid: u32, lines: u32) -> Result<Vec<TsLogEntry>> {
        let rows = self
            .call(
                Some(sid),
                "logview",
                vec![("lines", lines.to_string()), ("reverse", "1".to_string())],
            )
            .await?;
        Ok(rows.iter().map(map_log_entry).collect())
    }

    async fn create_snapshot(&self, sid: u32) -> Result<TsSnapshotData> {
        let rows = self
            .call(Some(sid), "serversnapshotcreate", vec![])
            .await?;
        let rec = rows.first().ok_or_else(|| {
            TsProtoError::Parse("serversnapshotcreate returned empty response".to_string())
        })?;
        Ok(TsSnapshotData {
            version: rec.get("version").cloned().unwrap_or_else(|| "1".to_string()),
            data: rec.get("data").cloned().unwrap_or_default(),
        })
    }

    async fn deploy_snapshot(&self, sid: u32, snapshot: &TsSnapshotData) -> Result<()> {
        self.call(
            Some(sid),
            "serversnapshotdeploy",
            vec![
                ("version", snapshot.version.clone()),
                ("data", snapshot.data.clone()),
            ],
        )
        .await?;
        Ok(())
    }
}
