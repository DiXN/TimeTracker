use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Deserialize, Serialize)]
#[sea_orm(table_name = "checkpoints")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub name: String,
    pub description: Option<String>,
    pub created_at: Option<chrono::NaiveDateTime>,
    pub valid_from: Option<chrono::NaiveDateTime>,
    pub color: Option<String>,
    pub app_id: i32,
    pub is_active: Option<bool>,
    pub duration: Option<i32>,
    pub sessions_count: Option<i32>,
    pub last_updated: Option<chrono::NaiveDateTime>,
    pub activated_at: Option<chrono::NaiveDateTime>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::apps::Entity",
        from = "Column::AppId",
        to = "super::apps::Column::Id"
    )]
    Apps,
    #[sea_orm(
        has_many = "super::timeline::Entity"
    )]
    Timeline,
}

impl Related<super::apps::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Apps.def()
    }
}

impl Related<super::timeline::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Timeline.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
