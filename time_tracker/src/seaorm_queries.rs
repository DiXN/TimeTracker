use std::error::Error;

use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, RelationTrait,
};
use serde_json::Value;

use crate::seaorm_client::SeaORMClient;

// Import our entities
use crate::entities::{
    active_checkpoints, apps, checkpoint_durations, checkpoints, timeline, timeline_checkpoints,
};

// Helper function to convert SeaORM model to JSON
fn model_to_json<T: serde::Serialize>(model: T) -> serde_json::Value {
    match serde_json::to_value(model) {
        Ok(val) => val,
        Err(e) => {
            eprintln!("Failed to serialize model: {}", e);
            serde_json::Value::Null
        }
    }
}

// Helper function to convert SeaORM model to JSON with custom error context
pub fn model_to_json_with_context<T: serde::Serialize>(
    model: T,
    context: &str,
) -> serde_json::Value {
    match serde_json::to_value(model) {
        Ok(val) => val,
        Err(e) => {
            eprintln!("Failed to serialize {}: {}", context, e);
            serde_json::Value::Null
        }
    }
}

// Checkpoint queries
pub fn create_checkpoint(
    client: &SeaORMClient,
    name: &str,
    description: Option<&str>,
    app_id: i32,
) -> Result<Value, Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        // Get the next ID
        let max_id: Option<i32> = checkpoints::Entity::find()
            .select_only()
            .column_as(checkpoints::Column::Id.max(), "max_id")
            .into_tuple()
            .one(&*client.connection)
            .await?;

        let next_id = max_id.map(|id| id + 1).unwrap_or(1);

        // Create the active model
        let checkpoint = checkpoints::ActiveModel {
            id: ActiveValue::Set(next_id),
            name: ActiveValue::Set(name.to_owned()),
            description: ActiveValue::Set(description.map(|s| s.to_owned())),
            created_at: ActiveValue::Set(Some(chrono::Utc::now().naive_utc())),
            valid_from: ActiveValue::Set(chrono::Utc::now().naive_utc()),
            color: ActiveValue::Set(Some("#2196F3".to_owned())),
            app_id: ActiveValue::Set(app_id),
            is_active: ActiveValue::Set(Some(false)),
        };

        // Insert into database
        checkpoints::Entity::insert(checkpoint)
            .exec(&*client.connection)
            .await?;

        Ok(serde_json::json!({"success": "Checkpoint created successfully"}))
    })
}

pub fn set_checkpoint_active(
    client: &SeaORMClient,
    checkpoint_id: i32,
    is_active: bool,
) -> Result<Value, Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        let mut checkpoint: checkpoints::ActiveModel =
            checkpoints::Entity::find_by_id(checkpoint_id)
                .one(&*client.connection)
                .await?
                .ok_or("Checkpoint not found")?
                .into();

        checkpoint.is_active = ActiveValue::Set(Some(is_active));

        checkpoint.update(&*client.connection).await?;

        Ok(serde_json::json!({"success": "Checkpoint status updated successfully"}))
    })
}

pub fn delete_checkpoint(
    client: &SeaORMClient,
    checkpoint_id: i32,
) -> Result<Value, Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        checkpoints::Entity::delete_by_id(checkpoint_id)
            .exec(&*client.connection)
            .await?;

        Ok(serde_json::json!({"success": "Checkpoint deleted successfully"}))
    })
}

pub fn get_all_checkpoints(client: &SeaORMClient) -> Result<Value, Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        let checkpoints: Vec<checkpoints::Model> = checkpoints::Entity::find()
            .order_by_desc(checkpoints::Column::Id)
            .all(&*client.connection)
            .await?;

        let json_values: Vec<Value> = checkpoints
            .into_iter()
            .map(|checkpoint| model_to_json_with_context(checkpoint, "checkpoint"))
            .collect();

        Ok(Value::Array(json_values))
    })
}

pub fn get_checkpoints_for_app(
    client: &SeaORMClient,
    app_id: i32,
) -> Result<Value, Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        let checkpoints: Vec<checkpoints::Model> = checkpoints::Entity::find()
            .filter(checkpoints::Column::AppId.eq(app_id))
            .order_by_desc(checkpoints::Column::Id)
            .all(&*client.connection)
            .await?;

        let json_values: Vec<Value> = checkpoints
            .into_iter()
            .map(|checkpoint| model_to_json_with_context(checkpoint, "checkpoint"))
            .collect();

        Ok(Value::Array(json_values))
    })
}

pub fn get_active_checkpoints(client: &SeaORMClient) -> Result<Value, Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        let checkpoints = checkpoints::Entity::find()
            .join(
                sea_orm::JoinType::InnerJoin,
                checkpoints::Relation::ActiveCheckpoints.def(),
            )
            .order_by_desc(checkpoints::Column::Id)
            .all(&*client.connection)
            .await?;

        let json_values: Vec<Value> = checkpoints
            .into_iter()
            .map(|checkpoint| model_to_json_with_context(checkpoint, "checkpoint"))
            .collect();

        Ok(Value::Array(json_values))
    })
}

pub fn get_active_checkpoints_for_app(
    client: &SeaORMClient,
    app_id: i32,
) -> Result<Value, Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        let checkpoints = checkpoints::Entity::find()
            .filter(checkpoints::Column::AppId.eq(app_id))
            .join(
                sea_orm::JoinType::InnerJoin,
                checkpoints::Relation::ActiveCheckpoints.def(),
            )
            .order_by_desc(checkpoints::Column::Id)
            .all(&*client.connection)
            .await?;

        let json_values: Vec<Value> = checkpoints
            .into_iter()
            .map(|checkpoint| model_to_json_with_context(checkpoint, "checkpoint"))
            .collect();

        Ok(Value::Array(json_values))
    })
}

// Timeline queries
pub fn get_all_timeline(client: &SeaORMClient) -> Result<Value, Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        let timeline_entries: Vec<timeline::Model> = timeline::Entity::find()
            .order_by_desc(timeline::Column::Date)
            .all(&*client.connection)
            .await?;

        let json_values: Vec<Value> = timeline_entries
            .into_iter()
            .map(|entry| model_to_json_with_context(entry, "timeline entry"))
            .collect();

        Ok(Value::Array(json_values))
    })
}

pub fn get_timeline_checkpoint_associations(
    client: &SeaORMClient,
) -> Result<Value, Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        let associations: Vec<timeline_checkpoints::Model> = timeline_checkpoints::Entity::find()
            .order_by(
                timeline_checkpoints::Column::TimelineId,
                sea_orm::Order::Asc,
            )
            .order_by(
                timeline_checkpoints::Column::CheckpointId,
                sea_orm::Order::Asc,
            )
            .all(&*client.connection)
            .await?;

        let json_values: Vec<Value> = associations
            .into_iter()
            .map(|association| model_to_json_with_context(association, "association"))
            .collect();

        Ok(Value::Array(json_values))
    })
}

// Timeline checkpoint queries
pub fn create_timeline_checkpoint(
    client: &SeaORMClient,
    timeline_id: i32,
    checkpoint_id: i32,
) -> Result<Value, Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        // Get the next ID
        let max_id: Option<i32> = timeline_checkpoints::Entity::find()
            .select_only()
            .column_as(timeline_checkpoints::Column::Id.max(), "max_id")
            .into_tuple()
            .one(&*client.connection)
            .await?;

        let next_id = max_id.map(|id| id + 1).unwrap_or(1);

        // Create the active model
        let timeline_checkpoint = timeline_checkpoints::ActiveModel {
            id: ActiveValue::Set(next_id),
            timeline_id: ActiveValue::Set(timeline_id),
            checkpoint_id: ActiveValue::Set(checkpoint_id),
            created_at: ActiveValue::Set(Some(chrono::Utc::now().naive_utc())),
        };

        // Insert into database
        timeline_checkpoints::Entity::insert(timeline_checkpoint)
            .exec(&*client.connection)
            .await?;

        Ok(serde_json::json!({"success": "Timeline checkpoint association created"}))
    })
}

pub fn delete_timeline_checkpoint(
    client: &SeaORMClient,
    timeline_id: i32,
    checkpoint_id: i32,
) -> Result<Value, Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        timeline_checkpoints::Entity::delete_many()
            .filter(
                timeline_checkpoints::Column::TimelineId
                    .eq(timeline_id)
                    .and(timeline_checkpoints::Column::CheckpointId.eq(checkpoint_id)),
            )
            .exec(&*client.connection)
            .await?;

        Ok(serde_json::json!({"success": "Timeline checkpoint association deleted"}))
    })
}

pub fn get_timeline_checkpoints_for_timeline(
    client: &SeaORMClient,
    timeline_id: i32,
) -> Result<Value, Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        let timeline_checkpoints: Vec<timeline_checkpoints::Model> =
            timeline_checkpoints::Entity::find()
                .filter(timeline_checkpoints::Column::TimelineId.eq(timeline_id))
                .order_by_desc(timeline_checkpoints::Column::CreatedAt)
                .all(&*client.connection)
                .await?;

        let json_values: Vec<Value> = timeline_checkpoints
            .into_iter()
            .map(|tc| model_to_json_with_context(tc, "timeline checkpoint"))
            .collect();

        Ok(Value::Array(json_values))
    })
}

// Checkpoint duration queries
pub fn get_checkpoint_durations(client: &SeaORMClient) -> Result<Value, Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        let durations: Vec<checkpoint_durations::Model> = checkpoint_durations::Entity::find()
            .order_by_desc(checkpoint_durations::Column::LastUpdated)
            .all(&*client.connection)
            .await?;

        let json_values: Vec<Value> = durations
            .into_iter()
            .map(|duration| model_to_json_with_context(duration, "duration"))
            .collect();

        Ok(Value::Array(json_values))
    })
}

pub fn get_checkpoint_durations_for_app(
    client: &SeaORMClient,
    app_id: i32,
) -> Result<Value, Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        let durations: Vec<checkpoint_durations::Model> = checkpoint_durations::Entity::find()
            .filter(checkpoint_durations::Column::AppId.eq(app_id))
            .order_by_desc(checkpoint_durations::Column::LastUpdated)
            .all(&*client.connection)
            .await?;

        let json_values: Vec<Value> = durations
            .into_iter()
            .map(|duration| model_to_json_with_context(duration, "duration"))
            .collect();

        Ok(Value::Array(json_values))
    })
}

pub fn update_checkpoint_duration(
    client: &SeaORMClient,
    checkpoint_id: i32,
    app_id: i32,
    duration: i32,
    sessions_count: i32,
) -> Result<Value, Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        // Try to find existing record
        let existing = checkpoint_durations::Entity::find()
            .filter(
                checkpoint_durations::Column::CheckpointId
                    .eq(checkpoint_id)
                    .and(checkpoint_durations::Column::AppId.eq(app_id)),
            )
            .one(&*client.connection)
            .await?;

        if let Some(model) = existing {
            // Update existing record
            let mut active_model: checkpoint_durations::ActiveModel = model.into();
            active_model.duration = ActiveValue::Set(Some(duration));
            active_model.sessions_count = ActiveValue::Set(Some(sessions_count));
            active_model.last_updated = ActiveValue::Set(Some(chrono::Utc::now().naive_utc()));

            active_model.update(&*client.connection).await?;
        } else {
            // Insert new record
            // Get the next ID
            let max_id: Option<i32> = checkpoint_durations::Entity::find()
                .select_only()
                .column_as(checkpoint_durations::Column::Id.max(), "max_id")
                .into_tuple()
                .one(&*client.connection)
                .await?;

            let next_id = max_id.map(|id| id + 1).unwrap_or(1);

            let new_duration = checkpoint_durations::ActiveModel {
                id: ActiveValue::Set(next_id),
                checkpoint_id: ActiveValue::Set(checkpoint_id),
                app_id: ActiveValue::Set(app_id),
                duration: ActiveValue::Set(Some(duration)),
                sessions_count: ActiveValue::Set(Some(sessions_count)),
                last_updated: ActiveValue::Set(Some(chrono::Utc::now().naive_utc())),
            };

            checkpoint_durations::Entity::insert(new_duration)
                .exec(&*client.connection)
                .await?;
        }

        Ok(serde_json::json!({"success": "Checkpoint duration updated"}))
    })
}

// Active checkpoint queries
pub fn get_all_active_checkpoints_table(client: &SeaORMClient) -> Result<Value, Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        let active_checkpoints: Vec<active_checkpoints::Model> = active_checkpoints::Entity::find()
            .order_by_desc(active_checkpoints::Column::ActivatedAt)
            .all(&*client.connection)
            .await?;

        let json_values: Vec<Value> = active_checkpoints
            .into_iter()
            .map(|ac| model_to_json_with_context(ac, "active checkpoint"))
            .collect();

        Ok(Value::Array(json_values))
    })
}

pub fn get_active_checkpoints_for_app_table(
    client: &SeaORMClient,
    app_id: i32,
) -> Result<Value, Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        let active_checkpoints: Vec<active_checkpoints::Model> = active_checkpoints::Entity::find()
            .filter(active_checkpoints::Column::AppId.eq(app_id))
            .order_by_desc(active_checkpoints::Column::ActivatedAt)
            .all(&*client.connection)
            .await?;

        let json_values: Vec<Value> = active_checkpoints
            .into_iter()
            .map(|ac| model_to_json_with_context(ac, "active checkpoint"))
            .collect();

        Ok(Value::Array(json_values))
    })
}

pub fn activate_checkpoint(
    client: &SeaORMClient,
    checkpoint_id: i32,
    app_id: i32,
) -> Result<Value, Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        // Get the next ID
        let max_id: Option<i32> = active_checkpoints::Entity::find()
            .select_only()
            .column_as(active_checkpoints::Column::Id.max(), "max_id")
            .into_tuple()
            .one(&*client.connection)
            .await?;

        let next_id = max_id.map(|id| id + 1).unwrap_or(1);

        // Create the active model
        let active_checkpoint = active_checkpoints::ActiveModel {
            id: ActiveValue::Set(next_id),
            checkpoint_id: ActiveValue::Set(checkpoint_id),
            activated_at: ActiveValue::Set(Some(chrono::Utc::now().naive_utc())),
            app_id: ActiveValue::Set(app_id),
        };

        // Insert into database
        active_checkpoints::Entity::insert(active_checkpoint)
            .exec(&*client.connection)
            .await?;

        Ok(serde_json::json!({"success": "Checkpoint activated"}))
    })
}

pub fn deactivate_checkpoint(
    client: &SeaORMClient,
    checkpoint_id: i32,
    app_id: i32,
) -> Result<Value, Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        active_checkpoints::Entity::delete_many()
            .filter(
                active_checkpoints::Column::CheckpointId
                    .eq(checkpoint_id)
                    .and(active_checkpoints::Column::AppId.eq(app_id)),
            )
            .exec(&*client.connection)
            .await?;

        Ok(serde_json::json!({"success": "Checkpoint deactivated"}))
    })
}

pub fn is_checkpoint_active(
    client: &SeaORMClient,
    checkpoint_id: i32,
    app_id: i32,
) -> Result<bool, Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        let count = active_checkpoints::Entity::find()
            .filter(
                active_checkpoints::Column::CheckpointId
                    .eq(checkpoint_id)
                    .and(active_checkpoints::Column::AppId.eq(app_id)),
            )
            .count(&*client.connection)
            .await?;

        Ok(count > 0)
    })
}

// Other utility functions
pub fn get_session_count_for_app(
    client: &SeaORMClient,
    app_id: i32,
) -> Result<i32, Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        let count = timeline::Entity::find()
            .filter(timeline::Column::AppId.eq(app_id))
            .count(&*client.connection)
            .await?;

        Ok(count as i32)
    })
}

pub fn get_all_app_ids(client: &SeaORMClient) -> Result<Vec<i32>, Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        let app_ids: Vec<i32> = apps::Entity::find()
            .order_by(apps::Column::Id, sea_orm::Order::Asc)
            .select_only()
            .column(apps::Column::Id)
            .into_tuple()
            .all(&*client.connection)
            .await?;

        Ok(app_ids)
    })
}
