use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Deserialize, Serialize)]
#[sea_orm(table_name = "timeline_checkpoints")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub timeline_id: i32,
    pub checkpoint_id: i32,
    pub created_at: Option<chrono::NaiveDateTime>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::timeline::Entity",
        from = "Column::TimelineId",
        to = "super::timeline::Column::Id"
    )]
    Timeline,
    #[sea_orm(
        belongs_to = "super::checkpoints::Entity",
        from = "Column::CheckpointId",
        to = "super::checkpoints::Column::Id"
    )]
    Checkpoints,
}

impl Related<super::timeline::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Timeline.def()
    }
}

impl Related<super::checkpoints::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Checkpoints.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}