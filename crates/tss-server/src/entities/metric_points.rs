use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "metric_points")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub instance_id: String,
    pub virtual_server_id: i32,
    pub clients_online: i32,
    pub max_clients: i32,
    pub channels_online: i32,
    pub avg_ping_ms: f64,
    pub packet_loss_pct: f64,
    pub bw_in_bps: i64,
    pub bw_out_bps: i64,
    pub recorded_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
