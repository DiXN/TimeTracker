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
    pub valid_from: chrono::NaiveDateTime,
    pub color: Option<String>,
    pub app_id: i32,
    pub is_active: Option<bool>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::apps::Entity",
        from = "Column::AppId",
        to = "super::apps::Column::Id"
    )]
    Apps,
    #[sea_orm(has_many = "super::timeline_checkpoints::Entity")]
    TimelineCheckpoints,
    #[sea_orm(has_many = "super::checkpoint_durations::Entity")]
    CheckpointDurations,
    #[sea_orm(has_many = "super::active_checkpoints::Entity")]
    ActiveCheckpoints,
}

impl Related<super::apps::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Apps.def()
    }
}

impl Related<super::timeline_checkpoints::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::TimelineCheckpoints.def()
    }
}

impl Related<super::checkpoint_durations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::CheckpointDurations.def()
    }
}

impl Related<super::active_checkpoints::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ActiveCheckpoints.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}