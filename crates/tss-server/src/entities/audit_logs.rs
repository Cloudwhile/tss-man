use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "audit_logs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub actor_user_id: Option<String>,
    pub actor_username: String,
    pub instance_id: Option<String>,
    pub virtual_server_id: Option<i32>,
    pub action: String,
    pub target: String,
    pub detail_json: String,
    pub ip_addr: String,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
