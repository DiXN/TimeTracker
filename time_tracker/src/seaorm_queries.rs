use std::error::Error;

use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect,
};
use serde_json::Value;

use crate::seaorm_client::SeaORMClient;

// Import our entities
use crate::entities::{
    apps, checkpoints, timeline,
};



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
pub async fn create_checkpoint(
    client: &SeaORMClient,
    name: &str,
    description: Option<&str>,
    app_id: i32,
) -> Result<Value, Box<dyn Error>> {
    // Create the active model
    let now = chrono::Utc::now().naive_utc();
    let checkpoint = checkpoints::ActiveModel {
        id: ActiveValue::NotSet, // Let the database auto-generate the ID
        name: ActiveValue::Set(name.to_owned()),
        description: ActiveValue::Set(description.map(|s| s.to_owned())),
        created_at: ActiveValue::Set(Some(now)),
        valid_from: ActiveValue::Set(Some(now)),
        color: ActiveValue::Set(Some("#2196F3".to_owned())),
        app_id: ActiveValue::Set(app_id),
        is_active: ActiveValue::Set(Some(false)),
        duration: ActiveValue::Set(None),
        sessions_count: ActiveValue::Set(None),
        last_updated: ActiveValue::Set(Some(now)), // Set last_updated when creating
        activated_at: ActiveValue::Set(None),
    };

    // Insert into database
    checkpoints::Entity::insert(checkpoint)
        .exec(&*client.connection)
        .await?;

    Ok(serde_json::json!({"success": "Checkpoint created successfully"}))
}

pub async fn set_checkpoint_active(
    client: &SeaORMClient,
    checkpoint_id: i32,
    is_active: bool,
) -> Result<Value, Box<dyn Error>> {
    let mut checkpoint: checkpoints::ActiveModel =
        checkpoints::Entity::find_by_id(checkpoint_id)
            .one(&*client.connection)
            .await?
            .ok_or("Checkpoint not found")?
            .into();

    checkpoint.is_active = ActiveValue::Set(Some(is_active));

    // Update activated_at timestamp when activating/deactivating
    let now = chrono::Utc::now().naive_utc();
    let activated_at = if is_active {
        Some(now)
    } else {
        None
    };
    checkpoint.activated_at = ActiveValue::Set(activated_at);

    // Also update last_updated timestamp
    checkpoint.last_updated = ActiveValue::Set(Some(now));

    checkpoint.update(&*client.connection).await?;

    Ok(serde_json::json!({"success": "Checkpoint status updated successfully"}))
}

pub async fn delete_checkpoint(
    client: &SeaORMClient,
    checkpoint_id: i32,
) -> Result<Value, Box<dyn Error>> {
    checkpoints::Entity::delete_by_id(checkpoint_id)
        .exec(&*client.connection)
        .await?;

    Ok(serde_json::json!({"success": "Checkpoint deleted successfully"}))
}

pub async fn get_all_checkpoints(client: &SeaORMClient) -> Result<Value, Box<dyn Error>> {
    let checkpoints: Vec<checkpoints::Model> = checkpoints::Entity::find()
        .order_by_desc(checkpoints::Column::Id)
        .all(&*client.connection)
        .await?;

    let json_values = checkpoints
        .into_iter()
        .map(|checkpoint| model_to_json_with_context(checkpoint, "checkpoint"))
        .collect::<Vec<Value>>();

    Ok(Value::Array(json_values))
}

pub async fn get_checkpoints_for_app(
    client: &SeaORMClient,
    app_id: i32,
) -> Result<Value, Box<dyn Error>> {
    let checkpoints: Vec<checkpoints::Model> = checkpoints::Entity::find()
        .filter(checkpoints::Column::AppId.eq(app_id))
        .order_by_desc(checkpoints::Column::Id)
        .all(&*client.connection)
        .await?;

    let json_values = checkpoints
        .into_iter()
        .map(|checkpoint| model_to_json_with_context(checkpoint, "checkpoint"))
        .collect::<Vec<Value>>();

    Ok(Value::Array(json_values))
}

pub async fn get_active_checkpoints(client: &SeaORMClient) -> Result<Value, Box<dyn Error>> {
    let checkpoints = checkpoints::Entity::find()
        .filter(checkpoints::Column::IsActive.eq(true))
        .order_by_desc(checkpoints::Column::Id)
        .all(&*client.connection)
        .await?;

    let json_values = checkpoints
        .into_iter()
        .map(|checkpoint| model_to_json_with_context(checkpoint, "checkpoint"))
        .collect::<Vec<Value>>();

    Ok(Value::Array(json_values))
}

pub async fn get_active_checkpoints_for_app(
    client: &SeaORMClient,
    app_id: i32,
) -> Result<Value, Box<dyn Error>> {
    let checkpoints = checkpoints::Entity::find()
        .filter(checkpoints::Column::AppId.eq(app_id))
        .filter(checkpoints::Column::IsActive.eq(true))
        .order_by_desc(checkpoints::Column::Id)
        .all(&*client.connection)
        .await?;

    let json_values = checkpoints
        .into_iter()
        .map(|checkpoint| model_to_json_with_context(checkpoint, "checkpoint"))
        .collect::<Vec<Value>>();

    Ok(Value::Array(json_values))
}


// Other utility functions
pub async fn get_session_count_for_app(
    client: &SeaORMClient,
    app_id: i32,
) -> Result<i32, Box<dyn Error>> {
    let count = timeline::Entity::find()
        .filter(timeline::Column::AppId.eq(app_id))
        .count(&*client.connection)
        .await?;

    Ok(count as i32)
}

pub async fn get_all_app_ids(client: &SeaORMClient) -> Result<Vec<i32>, Box<dyn Error>> {
    let app_ids: Vec<i32> = apps::Entity::find()
        .order_by(apps::Column::Id, sea_orm::Order::Asc)
        .select_only()
        .column(apps::Column::Id)
        .into_tuple()
        .all(&*client.connection)
        .await?;

    Ok(app_ids)
}

pub async fn get_checkpoint_durations_by_ids(
    client: &SeaORMClient,
    checkpoint_ids: &[i32],
) -> Result<Value, Box<dyn Error>> {
    if checkpoint_ids.is_empty() {
        return Ok(Value::Array(vec![]));
    }

    // Convert &[i32] to Vec<i32> for SeaORM
    let ids: Vec<i32> = checkpoint_ids.to_vec();

    let checkpoints: Vec<checkpoints::Model> = checkpoints::Entity::find()
        .filter(checkpoints::Column::Id.is_in(ids))
        .order_by_desc(checkpoints::Column::LastUpdated)
        .all(&*client.connection)
        .await?;

    let json_values = checkpoints
        .into_iter()
        .map(|checkpoint| model_to_json_with_context(checkpoint, "checkpoint"))
        .collect::<Vec<Value>>();

    Ok(Value::Array(json_values))
}



/// Get timeline entries with their associated checkpoints
pub async fn get_timeline_with_checkpoints(client: &SeaORMClient) -> Result<Value, Box<dyn Error>> {
    let timeline_entries: Vec<(timeline::Model, Option<checkpoints::Model>)> = timeline::Entity::find()
        .order_by_desc(timeline::Column::Date)
        .find_also_related(checkpoints::Entity)
        .all(&*client.connection)
        .await?;

    let json_values = timeline_entries
        .into_iter()
        .map(|(timeline, checkpoint)| {
            let mut timeline_json = serde_json::to_value(&timeline).unwrap_or(serde_json::Value::Null);
            if let Some(checkpoint_model) = checkpoint {
                if let Some(obj) = timeline_json.as_object_mut() {
                    obj.insert("checkpoint".to_string(), model_to_json_with_context(checkpoint_model, "checkpoint"));
                }
            }
            timeline_json
        })
        .collect::<Vec<Value>>();

    Ok(Value::Array(json_values))
}









