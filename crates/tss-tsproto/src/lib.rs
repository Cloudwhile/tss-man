pub mod codec;
pub mod driver;
#[cfg(any(test, feature = "test-mock"))]
pub mod mock_server;
pub mod server_query;
pub mod types;
pub mod web_query;

pub use driver::TsProtocolDriver;
#[cfg(any(test, feature = "test-mock"))]
pub use mock_server::MockTsServer;
pub use server_query::{RawTcpConnector, ServerQueryDriver, SshConnector};
pub use types::*;
pub use web_query::WebQueryDriver;

use std::sync::Arc;

pub fn create_driver(config: &TsConnectionConfig) -> Result<Arc<dyn TsProtocolDriver>> {
    create_driver_with_options(config, true)
}

pub fn create_ephemeral_driver(config: &TsConnectionConfig) -> Result<Arc<dyn TsProtocolDriver>> {
    create_driver_with_options(config, false)
}

fn create_driver_with_options(
    config: &TsConnectionConfig,
    enable_notify: bool,
) -> Result<Arc<dyn TsProtocolDriver>> {
    match config.protocol {
        ProtocolKind::RawTcp => {
            let conn = Arc::new(RawTcpConnector::new(config));
            Ok(Arc::new(ServerQueryDriver::new(
                conn,
                config.virtual_server_id,
                enable_notify,
            )))
        }
        ProtocolKind::Ssh => {
            let conn = Arc::new(SshConnector::new(config));
            Ok(Arc::new(ServerQueryDriver::new(
                conn,
                config.virtual_server_id,
                enable_notify,
            )))
        }
        ProtocolKind::WebQuery => Ok(Arc::new(WebQueryDriver::new(config)?)),
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn full_raw_tcp_roundtrip_against_mock_server() {
        let mock = MockTsServer::start().await.expect("start mock server");
        let cfg = TsConnectionConfig {
            protocol: ProtocolKind::RawTcp,
            host: mock.host().to_string(),
            port: mock.port(),
            username: mock.username().to_string(),
            secret: mock.password().to_string(),
            virtual_server_id: 1,
            tls_verify: false,
        };
        let drv = create_driver(&cfg).expect("create driver");
        let mut events = drv.subscribe_events();

        drv.test_connection().await.expect("test connection");
        let servers = drv.list_virtual_servers().await.expect("list servers");
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].id, 1);

        let info = drv.get_server_info(1).await.expect("serverinfo");
        assert_eq!(info.max_clients, 64);

        let channels = drv.list_channels(1).await.expect("channellist");
        assert_eq!(channels.len(), 6);

        let clients = drv.list_clients(1).await.expect("clientlist");
        assert_eq!(clients.len(), 8);

        // Snapshot before mutation
        let snap = drv.create_snapshot(1).await.expect("snapshot create");
        assert!(!snap.data.is_empty());

        // Create channel and verify live notify event
        tokio::time::sleep(Duration::from_millis(120)).await;
        let new_cid = drv
            .create_channel(
                1,
                &CreateChannelParams {
                    name: "Ops Temp".into(),
                    parent_id: Some(2),
                    topic: Some("Integration test".into()),
                    description: None,
                    password: None,
                    max_clients: Some(8),
                    is_permanent: Some(true),
                    is_semi_permanent: None,
                    is_default: None,
                    codec: None,
                    codec_quality: None,
                    needed_talk_power: None,
                    order: None,
                },
            )
            .await
            .expect("create channel");
        assert!(new_cid >= 7);

        let evt = timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("event timeout")
            .expect("event recv");
        assert!(matches!(evt, TsNotifyEvent::ChannelCreated { cid, .. } if cid == new_cid));

        // Deploy snapshot and verify created channel rolled back
        drv.deploy_snapshot(1, &snap).await.expect("deploy snapshot");
        let restored_channels = drv.list_channels(1).await.expect("channellist after rollback");
        assert_eq!(restored_channels.len(), 6);

        // Permissions & Privilege Keys
        let perms = drv.list_server_group_permissions(1, 6).await.expect("perms");
        assert!(!perms.is_empty());
        let token = drv
            .create_privilege_key(
                1,
                &CreatePrivilegeKeyParams {
                    token_type: 0,
                    group_id: 7,
                    channel_id: None,
                    description: Some("test token".into()),
                },
            )
            .await
            .expect("create token");
        assert!(token.starts_with("TSS-"));
        drv.delete_privilege_key(1, &token).await.expect("delete token");

        drv.shutdown().await.expect("shutdown driver");
        mock.shutdown();
    }

    #[tokio::test]
    async fn crlf_and_command_echo_roundtrip_without_blocking() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            let (rd, mut wr) = sock.into_split();
            let mut reader = BufReader::new(rd);
            // Send CRLF (\r\n) greeting with no trailing \r after \n
            wr.write_all(b"TS3\r\nWelcome to the TeamSpeak 3 ServerQuery interface.\r\n")
                .await
                .unwrap();
            wr.flush().await.unwrap();

            let mut line = String::new();
            while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
                let cmd = line.trim().to_string();
                // Simulate SSH shell echoing the command back first
                wr.write_all(format!("{cmd}\r\n").as_bytes()).await.unwrap();
                if cmd.starts_with("version") {
                    wr.write_all(b"version=6.0.0-beta6 platform=Linux\r\nerror id=0 msg=ok\r\n")
                        .await
                        .unwrap();
                } else if cmd.starts_with("serverinfo") {
                    wr.write_all(b"virtualserver_id=1 virtualserver_name=ICETSS virtualserver_maxclients=32\r\nerror id=0 msg=ok\r\n")
                        .await
                        .unwrap();
                } else if cmd == "quit" {
                    let _ = wr.write_all(b"error id=0 msg=ok\r\n").await;
                    break;
                } else {
                    wr.write_all(b"error id=0 msg=ok\r\n").await.unwrap();
                }
                wr.flush().await.unwrap();
                line.clear();
            }
        });

        let cfg = TsConnectionConfig {
            protocol: ProtocolKind::RawTcp,
            host: "127.0.0.1".into(),
            port,
            username: "serveradmin".into(),
            secret: "secret".into(),
            virtual_server_id: 1,
            tls_verify: false,
        };
        let drv = create_ephemeral_driver(&cfg).unwrap();

        timeout(Duration::from_secs(3), drv.test_connection())
            .await
            .expect("must not block on CRLF")
            .expect("version ok");
        let info = timeout(Duration::from_secs(3), drv.get_server_info(1))
            .await
            .expect("must not block on serverinfo")
            .expect("serverinfo ok");
        assert_eq!(info.name, "ICETSS");
        timeout(
            Duration::from_secs(3),
            drv.edit_server_info(
                1,
                &EditServerInfoParams {
                    name: Some("ICETSS Updated".into()),
                    ..Default::default()
                },
            ),
        )
        .await
        .expect("must not block on serveredit")
        .expect("serveredit ok");

        drv.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn surfaces_flood_ban_3329_and_reconnects_after_idle_drop() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        use tokio::net::TcpListener;

        // 1. Server immediately returning error id=3329 + FIN
        let ban_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ban_port = ban_listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            if let Ok((mut sock, _)) = ban_listener.accept().await {
                let _ = sock
                    .write_all(
                        b"error id=3329 msg=connection\\sfailed,\\syou\\sare\\sbanned extra_msg=you\\smay\\sretry\\sin\\s120\\sseconds\n\r",
                    )
                    .await;
                let _ = sock.shutdown().await;
            }
        });

        let ban_drv = create_ephemeral_driver(&TsConnectionConfig {
            protocol: ProtocolKind::RawTcp,
            host: "127.0.0.1".into(),
            port: ban_port,
            username: "serveradmin".into(),
            secret: "secret".into(),
            virtual_server_id: 1,
            tls_verify: false,
        })
        .unwrap();

        let err = ban_drv.test_connection().await.unwrap_err();
        assert!(
            matches!(err, TsProtoError::QueryError { id: 3329, .. }),
            "expected QueryError 3329, got {err:?}"
        );

        // 2. Server closes idle connection #1; subsequent serveredit transparently opens connection #2
        let drop_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let drop_port = drop_listener.local_addr().unwrap().port();
        let conn_count = Arc::new(AtomicUsize::new(0));
        let conn_count_bg = conn_count.clone();
        tokio::spawn(async move {
            loop {
                let Ok((sock, _)) = drop_listener.accept().await else {
                    break;
                };
                let idx = conn_count_bg.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    let (rd, mut wr) = sock.into_split();
                    let mut reader = BufReader::new(rd);
                    let _ = wr.write_all(b"TS3\n\rWelcome to TeamSpeak.\n\r").await;
                    let mut line = String::new();
                    while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
                        let cmd = line.trim().to_string();
                        let _ = wr.write_all(b"error id=0 msg=ok\n\r").await;
                        let _ = wr.flush().await;
                        if idx == 0 && cmd.starts_with("version") {
                            // Close connection #1 right after answering version
                            break;
                        }
                        if cmd == "quit" {
                            break;
                        }
                        line.clear();
                    }
                });
            }
        });

        let drop_drv = create_ephemeral_driver(&TsConnectionConfig {
            protocol: ProtocolKind::RawTcp,
            host: "127.0.0.1".into(),
            port: drop_port,
            username: "serveradmin".into(),
            secret: "secret".into(),
            virtual_server_id: 1,
            tls_verify: false,
        })
        .unwrap();

        drop_drv.test_connection().await.expect("conn #1 version ok");
        tokio::time::sleep(Duration::from_millis(80)).await;
        drop_drv
            .edit_server_info(
                1,
                &EditServerInfoParams {
                    name: Some("ICETSS".into()),
                    ..Default::default()
                },
            )
            .await
            .expect("serveredit must transparently reconnect after idle disconnect");
        assert_eq!(conn_count.load(Ordering::SeqCst), 2);
        drop_drv.shutdown().await.unwrap();
    }
}
