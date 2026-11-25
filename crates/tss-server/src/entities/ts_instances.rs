use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "ts_instances")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub host: String,
    pub port: i32,
    pub username: String,
    pub encrypted_secret: String,
    pub default_virtual_server_id: i32,
    pub tls_verify: bool,
    pub enabled: bool,
    pub status: String,
    pub last_error: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::user_instance_roles::Entity")]
    UserInstanceRoles,
}

impl Related<super::user_instance_roles::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::UserInstanceRoles.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
