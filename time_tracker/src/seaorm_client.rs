use async_trait::async_trait;
use std::{error::Error, sync::Arc, time::Duration};

use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectOptions, Database, DatabaseConnection,
    DbErr, EntityTrait, QueryFilter, QueryOrder, QuerySelect, RelationTrait,
};
use serde_json::{Value, json};

use crate::receive_types::ReceiveTypes;
use crate::restable::Restable;
use crate::websocket::{broadcast_apps_update, broadcast_timeline_update, has_active_broadcaster};
use crossbeam_channel::Receiver;

// Import our entities
use crate::entities::{apps, checkpoints, timeline};

// Import our SeaORM query functions
use crate::seaorm_queries::{
    add_process_alias, create_checkpoint, delete_checkpoint, get_active_checkpoints,
    get_active_checkpoints_for_app, get_all_app_ids, get_all_checkpoints, get_all_process_aliases,
    get_checkpoint_durations_by_ids, get_checkpoints_for_app, get_process_aliases_by_app_names,
    get_process_aliases_for_app, get_session_count_for_app, get_timeline_with_checkpoints,
    remove_process_alias, set_checkpoint_active,
};

#[derive(Clone)]
pub struct SeaORMClient {
    pub connection: Arc<DatabaseConnection>,
}

impl SeaORMClient {
    /// Create a new SeaORMClient with a provided database connection.
    /// This constructor is useful for testing with in-memory databases.
    ///
    /// # Arguments
    ///
    /// * `connection` - An Arc-wrapped DatabaseConnection to use
    pub fn new_with_connection(connection: Arc<DatabaseConnection>) -> Self {
        Self { connection }
    }
}

impl SeaORMClient {
    pub async fn new(url: &str) -> Result<Self, DbErr> {
        let mut opt = ConnectOptions::new(url.to_owned());
        opt.max_connections(100)
            .min_connections(5)
            .connect_timeout(Duration::from_secs(8))
            .idle_timeout(Duration::from_secs(8));
        let db = Database::connect(opt).await?;
        Ok(Self {
            connection: Arc::new(db),
        })
    }
}

#[async_trait]
impl Restable for SeaORMClient {
    async fn setup(&self) -> Result<(), Box<dyn Error>> {
        // Run database migrations
        use sea_orm_migration::MigratorTrait;
        crate::migration::Migrator::up(&*self.connection, None).await?;
        Ok(())
    }

    async fn get_processes(&self) -> Result<Vec<String>, Box<dyn Error>> {
        let apps: Vec<apps::Model> = apps::Entity::find().all(&*self.connection).await?;

        let process_names: Vec<String> = apps.into_iter().filter_map(|app| app.name).collect();

        Ok(process_names)
    }

    async fn put_data(&self, item: &str, product_name: &str) -> Result<Value, Box<dyn Error>> {
        // Get the next ID
        let max_id_result = apps::Entity::find()
            .select_only()
            .column_as(apps::Column::Id.max(), "max_id")
            .into_tuple::<Option<i32>>()
            .one(&*self.connection)
            .await?;

        let next_id = match max_id_result {
            Some(Some(id)) => id + 1,
            _ => 1, // Either no rows or max_id is NULL
        };

        // Create the active model
        let app = apps::ActiveModel {
            id: ActiveValue::Set(next_id),
            duration: ActiveValue::Set(Some(0)),
            launches: ActiveValue::Set(Some(0)),
            longest_session: ActiveValue::Set(Some(0)),
            name: ActiveValue::Set(Some(item.to_owned())),
            product_name: ActiveValue::Set(Some(product_name.to_owned())),
            longest_session_on: ActiveValue::Set(None),
        };

        // Insert into database
        apps::Entity::insert(app).exec(&*self.connection).await?;

        Ok(json!({"insert": item}))
    }

    async fn delete_data(&self, item: &str) -> Result<Value, Box<dyn Error>> {
        apps::Entity::delete_many()
            .filter(apps::Column::Name.eq(item))
            .exec(&*self.connection)
            .await?;

        Ok(json!({"delete": item}))
    }

    fn init_event_loop(self, rx: Receiver<(String, ReceiveTypes)>) {
        // Initialize the event loop for processing data
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(runtime) => runtime,
                Err(e) => {
                    eprintln!("Failed to create async runtime: {}", e);
                    return;
                }
            };

            while let Ok((item, receive_type)) = rx.recv() {
                match receive_type {
                    ReceiveTypes::LongestSession => {
                        let split: Vec<&str> = item.split(';').collect();
                        if split.len() >= 2 {
                            let app_name = split[0];
                            let current_session = match split[1].parse::<i32>() {
                                Ok(val) => val,
                                Err(_) => {
                                    eprintln!("Failed to parse session duration: {}", split[1]);
                                    0
                                }
                            };

                            // Update longest session for the app
                            if let Err(e) = rt.block_on(async {
                                // Find the app by name
                                let app = apps::Entity::find()
                                    .filter(apps::Column::Name.eq(app_name))
                                    .one(&*self.connection)
                                    .await?;

                                if let Some(app_model) = app {
                                    let longest_session = app_model.longest_session.unwrap_or(0);
                                    if current_session > longest_session {
                                        let mut app_active: apps::ActiveModel = app_model.into();
                                        app_active.longest_session =
                                            ActiveValue::Set(Some(current_session));
                                        app_active.longest_session_on = ActiveValue::Set(Some(
                                            chrono::Utc::now().naive_utc().date(),
                                        ));
                                        app_active.update(&*self.connection).await?;
                                    }
                                }

                                Ok::<(), Box<dyn std::error::Error>>(())
                            }) {
                                eprintln!("Error updating longest session for {}: {}", app_name, e);
                            }

                            // Broadcast apps update after longest session change
                            if has_active_broadcaster() {
                                if let Ok(apps_result) = rt.block_on(self.get_all_apps()) {
                                    if let Ok(apps_vec) =
                                        serde_json::from_value::<Vec<crate::structs::App>>(
                                            apps_result,
                                        )
                                    {
                                        broadcast_apps_update(apps_vec);
                                    }
                                }
                            }
                        }
                    }
                    ReceiveTypes::Duration => {
                        if let Err(e) = rt.block_on(async {
                            // Find the app by name and increment duration
                            let app = apps::Entity::find()
                                .filter(apps::Column::Name.eq(&item))
                                .one(&*self.connection)
                                .await
                                .map_err(|e| format!("Database error finding app: {}", e))?;

                            if let Some(app_model) = app {
                                let app_id = app_model.id;
                                let current_duration = app_model.duration.unwrap_or(0);
                                let new_duration = current_duration + 1;

                                let mut app_active: apps::ActiveModel = app_model.into();
                                app_active.duration = ActiveValue::Set(Some(new_duration));
                                app_active.update(&*self.connection)
                                    .await
                                    .map_err(|e| format!("Database error updating app duration: {}", e))?;

                                // Check for active checkpoints for this app and update their duration
                                let active_checkpoints_value = get_active_checkpoints_for_app(&self, app_id)
                                    .await
                                    .map_err(|e| format!("Database error finding active checkpoints: {}", e))?;

                                // Parse the JSON response to get checkpoint models and update their duration
                                if let Some(checkpoints) = active_checkpoints_value.as_array() {
                                    for checkpoint_value in checkpoints {
                                        if let Ok(checkpoint) = serde_json::from_value::<checkpoints::Model>(checkpoint_value.clone()) {
                                            let current_checkpoint_duration = checkpoint.duration.unwrap_or(0);
                                            let new_checkpoint_duration = current_checkpoint_duration + 1;

                                            // Update the checkpoint duration directly
                                            let checkpoint_model = checkpoints::Entity::find_by_id(checkpoint.id)
                                                .one(&*self.connection)
                                                .await
                                                .ok()
                                                .flatten();

                                            if let Some(checkpoint_model) = checkpoint_model {
                                                let mut checkpoint_active: checkpoints::ActiveModel = checkpoint_model.into();
                                                checkpoint_active.duration = ActiveValue::Set(Some(new_checkpoint_duration));
                                                checkpoint_active.last_updated = ActiveValue::Set(Some(chrono::Utc::now().naive_utc()));
                                                if let Err(e) = checkpoint_active.update(&*self.connection).await {
                                                    eprintln!("Database error updating checkpoint duration: {}", e);
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            Ok::<(), Box<dyn std::error::Error>>(())
                        }) {
                            eprintln!("Error updating duration for {}: {}", item, e);
                        }

                        // Broadcast apps update after duration change
                        if has_active_broadcaster() {
                            if let Ok(apps_result) = rt.block_on(self.get_all_apps()) {
                                if let Ok(apps_vec) =
                                    serde_json::from_value::<Vec<crate::structs::App>>(apps_result)
                                {
                                    broadcast_apps_update(apps_vec);
                                }
                            }
                        }
                    }
                    ReceiveTypes::Launches => {
                        if let Err(e) = rt.block_on(async {
                            // Find the app by name and increment launches
                            let app = apps::Entity::find()
                                .filter(apps::Column::Name.eq(&item))
                                .one(&*self.connection)
                                .await
                                .map_err(|e| format!("Database error finding app: {}", e))?;

                            if let Some(app_model) = app {
                                let app_id = app_model.id;
                                let current_launches = app_model.launches.unwrap_or(0);
                                let new_launches = current_launches + 1;

                                let mut app_active: apps::ActiveModel = app_model.into();
                                app_active.launches = ActiveValue::Set(Some(new_launches));
                                app_active.update(&*self.connection)
                                    .await
                                    .map_err(|e| format!("Database error updating app launches: {}", e))?;

                                // Check for active checkpoints for this app and increment their session count
                                let active_checkpoints_value = get_active_checkpoints_for_app(&self, app_id)
                                    .await
                                    .map_err(|e| format!("Database error finding active checkpoints: {}", e))?;

                                // Parse the JSON response to get checkpoint models and increment their session count
                                if let Some(checkpoints) = active_checkpoints_value.as_array() {
                                    for checkpoint_value in checkpoints {
                                        if let Ok(checkpoint) = serde_json::from_value::<checkpoints::Model>(checkpoint_value.clone()) {
                                            let current_sessions_count = checkpoint.sessions_count.unwrap_or(0);
                                            let new_sessions_count = current_sessions_count + 1;

                                            // Update the checkpoint sessions count directly
                                            let checkpoint_model = checkpoints::Entity::find_by_id(checkpoint.id)
                                                .one(&*self.connection)
                                                .await
                                                .ok()
                                                .flatten();

                                            if let Some(checkpoint_model) = checkpoint_model {
                                                let mut checkpoint_active: checkpoints::ActiveModel = checkpoint_model.into();
                                                checkpoint_active.sessions_count = ActiveValue::Set(Some(new_sessions_count));
                                                checkpoint_active.last_updated = ActiveValue::Set(Some(chrono::Utc::now().naive_utc()));
                                                if let Err(e) = checkpoint_active.update(&*self.connection).await {
                                                    eprintln!("Database error updating checkpoint sessions: {}", e);
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            Ok::<(), Box<dyn std::error::Error>>(())
                        }) {
                            eprintln!("Error updating launches for {}: {}", item, e);
                        }

                        // Broadcast apps update after launches change
                        if has_active_broadcaster() {
                            if let Ok(apps_result) = rt.block_on(self.get_all_apps()) {
                                if let Ok(apps_vec) =
                                    serde_json::from_value::<Vec<crate::structs::App>>(apps_result)
                                {
                                    broadcast_apps_update(apps_vec);
                                }
                            }
                        }
                    }
                    ReceiveTypes::Timeline => {
                        if let Err(e) = rt.block_on(async {
                            // Find the app by name
                            let app = apps::Entity::find()
                                .filter(apps::Column::Name.eq(&item))
                                .one(&*self.connection)
                                .await?;

                            if let Some(app_model) = app {
                                let app_id = app_model.id;
                                let today = chrono::Utc::now().naive_utc().date();

                                // Check if there's already a timeline entry for today
                                let timeline_entry = timeline::Entity::find()
                                    .filter(timeline::Column::AppId.eq(app_id))
                                    .filter(timeline::Column::Date.eq(today))
                                    .one(&*self.connection)
                                    .await?;

                                if let Some(timeline_model) = timeline_entry {
                                    // Update existing entry
                                    let current_duration = timeline_model.duration.unwrap_or(0);
                                    let new_duration = current_duration + 1;

                                    let mut timeline_active: timeline::ActiveModel =
                                        timeline_model.into();
                                    timeline_active.duration = ActiveValue::Set(Some(new_duration));
                                    timeline_active.update(&*self.connection).await?;
                                } else {
                                    // Create new entry
                                    // Get the next ID
                                    let max_id_result = timeline::Entity::find()
                                        .select_only()
                                        .column_as(timeline::Column::Id.max(), "max_id")
                                        .into_tuple::<Option<i32>>()
                                        .one(&*self.connection)
                                        .await?;

                                    let next_id = match max_id_result {
                                        Some(Some(id)) => id + 1,
                                        _ => 1, // Either no rows or max_id is NULL
                                    };

                                    let new_timeline = timeline::ActiveModel {
                                        id: ActiveValue::Set(next_id),
                                        date: ActiveValue::Set(today),
                                        duration: ActiveValue::Set(Some(1)),
                                        app_id: ActiveValue::Set(app_id),
                                        checkpoint_id: ActiveValue::Set({
                                            // Check if there are any active checkpoints for this app
                                            // and associate the timeline entry with the first active checkpoint
                                            get_active_checkpoints_for_app(&self, app_id)
                                                .await
                                                .ok()
                                                .and_then(|active_checkpoints_value| {
                                                    // Parse the JSON response to get the first active checkpoint
                                                    active_checkpoints_value
                                                        .as_array()
                                                        .and_then(|arr| arr.first())
                                                        .and_then(|first_checkpoint| {
                                                            serde_json::from_value::<
                                                                checkpoints::Model,
                                                            >(
                                                                first_checkpoint.clone()
                                                            )
                                                            .ok()
                                                        })
                                                        .map(|checkpoint| checkpoint.id)
                                                })
                                        }),
                                    };

                                    timeline::Entity::insert(new_timeline)
                                        .exec(&*self.connection)
                                        .await?;
                                }
                            }

                            Ok::<(), Box<dyn std::error::Error>>(())
                        }) {
                            eprintln!("Error updating timeline for {}: {}", item, e);
                        }

                        // Broadcast timeline update after timeline change
                        if has_active_broadcaster() {
                            if let Ok(timeline_result) =
                                rt.block_on(self.get_timeline_data(None, 30))
                            {
                                if let Ok(timeline_vec) =
                                    serde_json::from_value::<Vec<crate::structs::Timeline>>(
                                        timeline_result,
                                    )
                                {
                                    broadcast_timeline_update(timeline_vec);
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    async fn get_all_apps(&self) -> Result<Value, Box<dyn Error>> {
        let apps: Vec<apps::Model> = apps::Entity::find()
            .order_by(apps::Column::Id, sea_orm::Order::Asc)
            .all(&*self.connection)
            .await?;

        let json_values: Vec<Value> = apps
            .into_iter()
            .map(|app| crate::seaorm_queries::model_to_json_with_context(app, "app"))
            .collect();

        Ok(Value::Array(json_values))
    }

    async fn get_timeline_data(
        &self,
        app_name: Option<&str>,
        days: i64,
    ) -> Result<Value, Box<dyn Error>> {
        if let Some(name) = app_name {
            // Get timeline data for a specific app
            let timeline_entries: Vec<timeline::Model> =
                timeline::Entity::find()
                    .join(sea_orm::JoinType::InnerJoin, timeline::Relation::Apps.def())
                    .filter(apps::Column::Name.eq(name).and(
                        timeline::Column::Date.gte(
                            chrono::Utc::now().naive_utc().date() - chrono::Duration::days(days),
                        ),
                    ))
                    .order_by_desc(timeline::Column::Date)
                    .all(&*self.connection)
                    .await?;

            let json_values: Vec<Value> = timeline_entries
                .into_iter()
                .map(|entry| match serde_json::to_value(entry) {
                    Ok(val) => val,
                    Err(e) => {
                        eprintln!("Failed to serialize timeline entry: {}", e);
                        Value::Null
                    }
                })
                .collect();

            Ok(Value::Array(json_values))
        } else {
            // Get timeline data for all apps
            let timeline_entries: Vec<timeline::Model> = timeline::Entity::find()
                .filter(
                    timeline::Column::Date
                        .gte(chrono::Utc::now().naive_utc().date() - chrono::Duration::days(days)),
                )
                .order_by_desc(timeline::Column::Date)
                .all(&*self.connection)
                .await?;

            let json_values: Vec<Value> = timeline_entries
                .into_iter()
                .map(|entry| {
                    crate::seaorm_queries::model_to_json_with_context(entry, "timeline entry")
                })
                .collect();

            Ok(Value::Array(json_values))
        }
    }

    async fn get_session_count_for_app(&self, app_id: i32) -> Result<i32, Box<dyn Error>> {
        get_session_count_for_app(self, app_id).await
    }

    async fn get_all_app_ids(&self) -> Result<Vec<i32>, Box<dyn Error>> {
        get_all_app_ids(self).await
    }

    async fn get_all_checkpoints(&self) -> Result<Value, Box<dyn Error>> {
        get_all_checkpoints(self).await
    }

    async fn get_checkpoints_for_app(&self, app_id: i32) -> Result<Value, Box<dyn Error>> {
        get_checkpoints_for_app(self, app_id).await
    }

    async fn create_checkpoint(
        &self,
        name: &str,
        description: Option<&str>,
        app_id: i32,
    ) -> Result<Value, Box<dyn Error>> {
        create_checkpoint(self, name, description, app_id).await
    }

    async fn set_checkpoint_active(
        &self,
        checkpoint_id: i32,
        is_active: bool,
    ) -> Result<Value, Box<dyn Error>> {
        set_checkpoint_active(self, checkpoint_id, is_active).await
    }

    async fn delete_checkpoint(&self, checkpoint_id: i32) -> Result<Value, Box<dyn Error>> {
        delete_checkpoint(self, checkpoint_id).await
    }

    async fn get_active_checkpoints(&self) -> Result<Value, Box<dyn Error>> {
        get_active_checkpoints(self).await
    }

    async fn get_active_checkpoints_for_app(&self, app_id: i32) -> Result<Value, Box<dyn Error>> {
        get_active_checkpoints_for_app(self, app_id).await
    }

    async fn get_timeline_with_checkpoints(&self) -> Result<Value, Box<dyn Error>> {
        get_timeline_with_checkpoints(self).await
    }

    async fn get_checkpoint_durations_by_ids(
        &self,
        checkpoint_ids: &[i32],
    ) -> Result<Value, Box<dyn Error>> {
        get_checkpoint_durations_by_ids(self, checkpoint_ids).await
    }

    async fn get_process_aliases_for_app(
        &self,
        app_id: i32,
    ) -> Result<Vec<String>, Box<dyn Error>> {
        get_process_aliases_for_app(self, app_id).await
    }

    async fn get_all_process_aliases(
        &self,
    ) -> Result<std::collections::HashMap<String, i32>, Box<dyn Error>> {
        get_all_process_aliases(self).await
    }

    async fn add_process_alias(
        &self,
        process_name: &str,
        app_id: i32,
    ) -> Result<Value, Box<dyn Error>> {
        add_process_alias(self, process_name, app_id).await
    }

    async fn remove_process_alias(
        &self,
        process_name: &str,
        app_id: i32,
    ) -> Result<Value, Box<dyn Error>> {
        remove_process_alias(self, process_name, app_id).await
    }

    async fn get_process_aliases_by_app_names(
        &self,
        app_names: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<String>>, Box<dyn Error>> {
        get_process_aliases_by_app_names(self, app_names).await
    }
}
