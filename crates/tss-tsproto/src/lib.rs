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
    match config.protocol {
        ProtocolKind::RawTcp => {
            let conn = Arc::new(RawTcpConnector::new(config));
            Ok(Arc::new(ServerQueryDriver::new(
                conn,
                config.virtual_server_id,
                true,
            )))
        }
        ProtocolKind::Ssh => {
            let conn = Arc::new(SshConnector::new(config));
            Ok(Arc::new(ServerQueryDriver::new(
                conn,
                config.virtual_server_id,
                true,
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
}
