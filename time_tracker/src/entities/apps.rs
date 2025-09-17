use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Deserialize, Serialize)]
#[sea_orm(table_name = "apps")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub duration: Option<i32>,
    pub launches: Option<i32>,
    pub longest_session: Option<i32>,
    pub name: Option<String>,
    pub product_name: Option<String>,
    pub longest_session_on: Option<chrono::NaiveDate>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::timeline::Entity")]
    Timeline,
    #[sea_orm(has_many = "super::checkpoints::Entity")]
    Checkpoints,
    #[sea_orm(has_many = "super::process_aliases::Entity")]
    ProcessAliases,
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

impl Related<super::process_aliases::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ProcessAliases.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
