use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 1. users
        manager
            .create_table(
                Table::create()
                    .table(Users::Table)
                    .if_not_exists()
                    .col(string_len(Users::Id, 36).primary_key())
                    .col(string_len(Users::Username, 64).unique_key())
                    .col(string_len(Users::DisplayName, 128))
                    .col(text(Users::PasswordHash))
                    .col(text_null(Users::TotpSecretEnc))
                    .col(boolean(Users::TotpEnabled).default(false))
                    .col(string_len(Users::GlobalRole, 32))
                    .col(boolean(Users::IsActive).default(true))
                    .col(timestamp_with_time_zone_null(Users::LastLoginAt))
                    .col(timestamp_with_time_zone(Users::CreatedAt))
                    .col(timestamp_with_time_zone(Users::UpdatedAt))
                    .to_owned(),
            )
            .await?;

        // 2. refresh_tokens
        manager
            .create_table(
                Table::create()
                    .table(RefreshTokens::Table)
                    .if_not_exists()
                    .col(string_len(RefreshTokens::Id, 36).primary_key())
                    .col(string_len(RefreshTokens::UserId, 36))
                    .col(string_len(RefreshTokens::TokenHash, 128).unique_key())
                    .col(string_len(RefreshTokens::UserAgent, 255))
                    .col(string_len(RefreshTokens::IpAddr, 64))
                    .col(timestamp_with_time_zone(RefreshTokens::ExpiresAt))
                    .col(timestamp_with_time_zone_null(RefreshTokens::RevokedAt))
                    .col(timestamp_with_time_zone(RefreshTokens::CreatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_refresh_tokens_user")
                            .from(RefreshTokens::Table, RefreshTokens::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // 3. ts_instances
        manager
            .create_table(
                Table::create()
                    .table(TsInstances::Table)
                    .if_not_exists()
                    .col(string_len(TsInstances::Id, 36).primary_key())
                    .col(string_len(TsInstances::Name, 128))
                    .col(string_len(TsInstances::Protocol, 24))
                    .col(string_len(TsInstances::Host, 255))
                    .col(integer(TsInstances::Port))
                    .col(string_len(TsInstances::Username, 128))
                    .col(text(TsInstances::EncryptedSecret))
                    .col(integer(TsInstances::DefaultVirtualServerId).default(1))
                    .col(boolean(TsInstances::TlsVerify).default(true))
                    .col(boolean(TsInstances::Enabled).default(true))
                    .col(string_len(TsInstances::Status, 24))
                    .col(text_null(TsInstances::LastError))
                    .col(timestamp_with_time_zone(TsInstances::CreatedAt))
                    .col(timestamp_with_time_zone(TsInstances::UpdatedAt))
                    .to_owned(),
            )
            .await?;

        // 4. user_instance_roles
        manager
            .create_table(
                Table::create()
                    .table(UserInstanceRoles::Table)
                    .if_not_exists()
                    .col(string_len(UserInstanceRoles::Id, 36).primary_key())
                    .col(string_len(UserInstanceRoles::UserId, 36))
                    .col(string_len(UserInstanceRoles::InstanceId, 36))
                    .col(string_len(UserInstanceRoles::Role, 32))
                    .col(timestamp_with_time_zone(UserInstanceRoles::CreatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_instance_roles_user")
                            .from(UserInstanceRoles::Table, UserInstanceRoles::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_instance_roles_instance")
                            .from(UserInstanceRoles::Table, UserInstanceRoles::InstanceId)
                            .to(TsInstances::Table, TsInstances::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_user_instance_roles_unique")
                    .table(UserInstanceRoles::Table)
                    .col(UserInstanceRoles::UserId)
                    .col(UserInstanceRoles::InstanceId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // 5. metric_points
        manager
            .create_table(
                Table::create()
                    .table(MetricPoints::Table)
                    .if_not_exists()
                    .col(string_len(MetricPoints::Id, 36).primary_key())
                    .col(string_len(MetricPoints::InstanceId, 36))
                    .col(integer(MetricPoints::VirtualServerId))
                    .col(integer(MetricPoints::ClientsOnline))
                    .col(integer(MetricPoints::MaxClients))
                    .col(integer(MetricPoints::ChannelsOnline))
                    .col(double(MetricPoints::AvgPingMs))
                    .col(double(MetricPoints::PacketLossPct))
                    .col(big_integer(MetricPoints::BwInBps))
                    .col(big_integer(MetricPoints::BwOutBps))
                    .col(timestamp_with_time_zone(MetricPoints::RecordedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_metric_points_instance")
                            .from(MetricPoints::Table, MetricPoints::InstanceId)
                            .to(TsInstances::Table, TsInstances::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_metric_points_lookup")
                    .table(MetricPoints::Table)
                    .col(MetricPoints::InstanceId)
                    .col(MetricPoints::VirtualServerId)
                    .col(MetricPoints::RecordedAt)
                    .to_owned(),
            )
            .await?;

        // 6. snapshots
        manager
            .create_table(
                Table::create()
                    .table(Snapshots::Table)
                    .if_not_exists()
                    .col(string_len(Snapshots::Id, 36).primary_key())
                    .col(string_len(Snapshots::InstanceId, 36))
                    .col(integer(Snapshots::VirtualServerId))
                    .col(string_len(Snapshots::Name, 128))
                    .col(string_len(Snapshots::TriggerKind, 32))
                    .col(text(Snapshots::EncryptedPayload))
                    .col(text(Snapshots::SummaryJson))
                    .col(big_integer(Snapshots::SizeBytes))
                    .col(string_len_null(Snapshots::CreatedBy, 36))
                    .col(timestamp_with_time_zone(Snapshots::CreatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_snapshots_instance")
                            .from(Snapshots::Table, Snapshots::InstanceId)
                            .to(TsInstances::Table, TsInstances::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_snapshots_user")
                            .from(Snapshots::Table, Snapshots::CreatedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        // 7. backup_policies
        manager
            .create_table(
                Table::create()
                    .table(BackupPolicies::Table)
                    .if_not_exists()
                    .col(string_len(BackupPolicies::Id, 36).primary_key())
                    .col(string_len(BackupPolicies::InstanceId, 36))
                    .col(integer(BackupPolicies::VirtualServerId))
                    .col(string_len(BackupPolicies::CronExpr, 64))
                    .col(integer(BackupPolicies::RetentionCount))
                    .col(boolean(BackupPolicies::Enabled).default(true))
                    .col(timestamp_with_time_zone_null(BackupPolicies::LastRunAt))
                    .col(string_len_null(BackupPolicies::LastStatus, 128))
                    .col(timestamp_with_time_zone(BackupPolicies::CreatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_backup_policies_instance")
                            .from(BackupPolicies::Table, BackupPolicies::InstanceId)
                            .to(TsInstances::Table, TsInstances::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // 8. automation_rules
        manager
            .create_table(
                Table::create()
                    .table(AutomationRules::Table)
                    .if_not_exists()
                    .col(string_len(AutomationRules::Id, 36).primary_key())
                    .col(string_len(AutomationRules::InstanceId, 36))
                    .col(integer(AutomationRules::VirtualServerId))
                    .col(string_len(AutomationRules::Name, 128))
                    .col(string_len(AutomationRules::RuleKind, 32))
                    .col(text(AutomationRules::ConfigJson))
                    .col(boolean(AutomationRules::Enabled).default(true))
                    .col(timestamp_with_time_zone_null(AutomationRules::LastTriggeredAt))
                    .col(timestamp_with_time_zone(AutomationRules::CreatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_automation_rules_instance")
                            .from(AutomationRules::Table, AutomationRules::InstanceId)
                            .to(TsInstances::Table, TsInstances::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // 9. webhook_endpoints
        manager
            .create_table(
                Table::create()
                    .table(WebhookEndpoints::Table)
                    .if_not_exists()
                    .col(string_len(WebhookEndpoints::Id, 36).primary_key())
                    .col(string_len(WebhookEndpoints::Name, 128))
                    .col(string_len(WebhookEndpoints::Url, 512))
                    .col(text_null(WebhookEndpoints::EncryptedSigningSecret))
                    .col(string_len(WebhookEndpoints::EventsCsv, 255))
                    .col(boolean(WebhookEndpoints::Enabled).default(true))
                    .col(string_len_null(WebhookEndpoints::LastDeliveryStatus, 128))
                    .col(timestamp_with_time_zone(WebhookEndpoints::CreatedAt))
                    .to_owned(),
            )
            .await?;

        // 10. audit_logs
        manager
            .create_table(
                Table::create()
                    .table(AuditLogs::Table)
                    .if_not_exists()
                    .col(string_len(AuditLogs::Id, 36).primary_key())
                    .col(string_len_null(AuditLogs::ActorUserId, 36))
                    .col(string_len(AuditLogs::ActorUsername, 64))
                    .col(string_len_null(AuditLogs::InstanceId, 36))
                    .col(integer_null(AuditLogs::VirtualServerId))
                    .col(string_len(AuditLogs::Action, 64))
                    .col(string_len(AuditLogs::Target, 128))
                    .col(text(AuditLogs::DetailJson))
                    .col(string_len(AuditLogs::IpAddr, 64))
                    .col(timestamp_with_time_zone(AuditLogs::CreatedAt))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_audit_logs_created_at")
                    .table(AuditLogs::Table)
                    .col(AuditLogs::CreatedAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for tbl in [
            AuditLogs::Table.into_iden(),
            WebhookEndpoints::Table.into_iden(),
            AutomationRules::Table.into_iden(),
            BackupPolicies::Table.into_iden(),
            Snapshots::Table.into_iden(),
            MetricPoints::Table.into_iden(),
            UserInstanceRoles::Table.into_iden(),
            TsInstances::Table.into_iden(),
            RefreshTokens::Table.into_iden(),
            Users::Table.into_iden(),
        ] {
            manager
                .drop_table(Table::drop().table(tbl).if_exists().to_owned())
                .await?;
        }
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
    Username,
    DisplayName,
    PasswordHash,
    TotpSecretEnc,
    TotpEnabled,
    GlobalRole,
    IsActive,
    LastLoginAt,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum RefreshTokens {
    Table,
    Id,
    UserId,
    TokenHash,
    UserAgent,
    IpAddr,
    ExpiresAt,
    RevokedAt,
    CreatedAt,
}

#[derive(DeriveIden)]
enum TsInstances {
    Table,
    Id,
    Name,
    Protocol,
    Host,
    Port,
    Username,
    EncryptedSecret,
    DefaultVirtualServerId,
    TlsVerify,
    Enabled,
    Status,
    LastError,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum UserInstanceRoles {
    Table,
    Id,
    UserId,
    InstanceId,
    Role,
    CreatedAt,
}

#[derive(DeriveIden)]
enum MetricPoints {
    Table,
    Id,
    InstanceId,
    VirtualServerId,
    ClientsOnline,
    MaxClients,
    ChannelsOnline,
    AvgPingMs,
    PacketLossPct,
    BwInBps,
    BwOutBps,
    RecordedAt,
}

#[derive(DeriveIden)]
enum Snapshots {
    Table,
    Id,
    InstanceId,
    VirtualServerId,
    Name,
    TriggerKind,
    EncryptedPayload,
    SummaryJson,
    SizeBytes,
    CreatedBy,
    CreatedAt,
}

#[derive(DeriveIden)]
enum BackupPolicies {
    Table,
    Id,
    InstanceId,
    VirtualServerId,
    CronExpr,
    RetentionCount,
    Enabled,
    LastRunAt,
    LastStatus,
    CreatedAt,
}

#[derive(DeriveIden)]
enum AutomationRules {
    Table,
    Id,
    InstanceId,
    VirtualServerId,
    Name,
    RuleKind,
    ConfigJson,
    Enabled,
    LastTriggeredAt,
    CreatedAt,
}

#[derive(DeriveIden)]
enum WebhookEndpoints {
    Table,
    Id,
    Name,
    Url,
    EncryptedSigningSecret,
    EventsCsv,
    Enabled,
    LastDeliveryStatus,
    CreatedAt,
}

#[derive(DeriveIden)]
enum AuditLogs {
    Table,
    Id,
    ActorUserId,
    ActorUsername,
    InstanceId,
    VirtualServerId,
    Action,
    Target,
    DetailJson,
    IpAddr,
    CreatedAt,
}
