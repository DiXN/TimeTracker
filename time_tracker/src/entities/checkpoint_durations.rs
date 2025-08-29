use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Deserialize, Serialize)]
#[sea_orm(table_name = "checkpoint_durations")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub checkpoint_id: i32,
    pub app_id: i32,
    pub duration: Option<i32>,
    pub sessions_count: Option<i32>,
    pub last_updated: Option<chrono::NaiveDateTime>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::checkpoints::Entity",
        from = "Column::CheckpointId",
        to = "super::checkpoints::Column::Id"
    )]
    Checkpoints,
    #[sea_orm(
        belongs_to = "super::apps::Entity",
        from = "Column::AppId",
        to = "super::apps::Column::Id"
    )]
    Apps,
}

impl Related<super::checkpoints::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Checkpoints.def()
    }
}

impl Related<super::apps::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Apps.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
