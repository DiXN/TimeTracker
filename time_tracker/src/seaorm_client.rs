use std::{error::Error, sync::Arc, time::Duration};

use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectOptions, Database, DatabaseConnection, DbErr, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect, RelationTrait,
};
use serde_json::{Value, json};

use crate::receive_types::ReceiveTypes;
use crate::restable::Restable;
use crossbeam_channel::Receiver;

// Import our entities
use crate::entities::{apps, timeline};

// Import our SeaORM query functions
use crate::seaorm_queries::*;

#[derive(Clone)]
pub struct SeaORMClient {
    pub connection: Arc<DatabaseConnection>,
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

impl Restable for SeaORMClient {
    async fn setup(&self) -> Result<(), Box<dyn Error>> {
        // Run database migrations
        use sea_orm_migration::MigratorTrait;
        crate::migration::Migrator::up(&*self.connection, None).await?;
        Ok(())
    }

    async fn get_data(&self, item: &str) -> Result<Value, Box<dyn Error>> {
        // Parse the item path to determine what data to fetch
        let parts: Vec<&str> = item.split('/').filter(|s| !s.is_empty()).collect();

        match parts.as_slice() {
            ["apps"] => {
                // Get all apps
                let apps: Vec<apps::Model> =
                    apps::Entity::find().all(&*self.connection).await?;
                let json_values: Vec<Value> = apps
                    .into_iter()
                    .map(|app| crate::seaorm_queries::model_to_json_with_context(app, "app"))
                    .collect();
                Ok(Value::Object(serde_json::Map::from_iter([(
                    "apps".to_string(),
                    Value::Array(json_values),
                )])))
            }
            ["apps", app_name] => {
                // Get specific app
                let app = apps::Entity::find()
                    .filter(apps::Column::Name.eq(*app_name))
                    .one(&*self.connection)
                    .await?;

                match app {
                    Some(app_model) => Ok(serde_json::to_value(app_model)?),
                    None => Ok(Value::Null),
                }
            }
            ["apps", app_name, field] => {
                // Get specific field of an app
                let app = apps::Entity::find()
                    .filter(apps::Column::Name.eq(*app_name))
                    .one(&*self.connection)
                    .await?;

                match app {
                    Some(app_model) => {
                        let value = match *field {
                            "duration" => serde_json::to_value(app_model.duration)?,
                            "launches" => serde_json::to_value(app_model.launches)?,
                            "longestSession" => {
                                serde_json::to_value(app_model.longest_session)?
                            }
                            "name" => serde_json::to_value(app_model.name)?,
                            "productName" => serde_json::to_value(app_model.product_name)?,
                            _ => Value::Null,
                        };
                        Ok(value)
                    }
                    None => Ok(Value::Null),
                }
            }
            _ => {
                // For unsupported paths, return empty object
                Ok(json!({}))
            }
        }
    }

    async fn get_processes(&self) -> Result<Vec<String>, Box<dyn Error>> {
        let apps: Vec<apps::Model> = apps::Entity::find().all(&*self.connection).await?;

        let process_names: Vec<String> = apps.into_iter().filter_map(|app| app.name).collect();

        Ok(process_names)
    }

    async fn put_data(&self, item: &str, product_name: &str) -> Result<Value, Box<dyn Error>> {
        // Get the next ID
        let max_id: Option<i32> = apps::Entity::find()
            .select_only()
            .column_as(apps::Column::Id.max(), "max_id")
            .into_tuple()
            .one(&*self.connection)
            .await?;

        let next_id = max_id.map(|id| id + 1).unwrap_or(1);

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
                        }
                    }
                    ReceiveTypes::Duration => {
                        if let Err(e) = rt.block_on(async {
                            // Find the app by name and increment duration
                            let app = apps::Entity::find()
                                .filter(apps::Column::Name.eq(&item))
                                .one(&*self.connection)
                                .await?;

                            if let Some(app_model) = app {
                                let current_duration = app_model.duration.unwrap_or(0);
                                let new_duration = current_duration + 1;

                                let mut app_active: apps::ActiveModel = app_model.into();
                                app_active.duration = ActiveValue::Set(Some(new_duration));
                                app_active.update(&*self.connection).await?;
                            }

                            Ok::<(), Box<dyn std::error::Error>>(())
                        }) {
                            eprintln!("Error updating duration for {}: {}", item, e);
                        }
                    }
                    ReceiveTypes::Launches => {
                        if let Err(e) = rt.block_on(async {
                            // Find the app by name and increment launches
                            let app = apps::Entity::find()
                                .filter(apps::Column::Name.eq(&item))
                                .one(&*self.connection)
                                .await?;

                            if let Some(app_model) = app {
                                let current_launches = app_model.launches.unwrap_or(0);
                                let new_launches = current_launches + 1;

                                let mut app_active: apps::ActiveModel = app_model.into();
                                app_active.launches = ActiveValue::Set(Some(new_launches));
                                app_active.update(&*self.connection).await?;
                            }

                            Ok::<(), Box<dyn std::error::Error>>(())
                        }) {
                            eprintln!("Error updating launches for {}: {}", item, e);
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
                                    let max_id: Option<i32> = timeline::Entity::find()
                                        .select_only()
                                        .column_as(timeline::Column::Id.max(), "max_id")
                                        .into_tuple()
                                        .one(&*self.connection)
                                        .await?;

                                    let next_id = max_id.map(|id| id + 1).unwrap_or(1);

                                    let new_timeline = timeline::ActiveModel {
                                        id: ActiveValue::Set(next_id),
                                        date: ActiveValue::Set(today),
                                        duration: ActiveValue::Set(Some(1)),
                                        app_id: ActiveValue::Set(app_id),
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
                    .filter(apps::Column::Name.eq(name).and(timeline::Column::Date.gte(
                        chrono::Utc::now().naive_utc().date() - chrono::Duration::days(days),
                    )))
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
            let timeline_entries: Vec<timeline::Model> =
                timeline::Entity::find()
                    .filter(timeline::Column::Date.gte(
                        chrono::Utc::now().naive_utc().date() - chrono::Duration::days(days),
                    ))
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

    async fn get_all_timeline(&self) -> Result<Value, Box<dyn Error>> {
        get_all_timeline(self).await
    }

    async fn get_timeline_checkpoint_associations(&self) -> Result<Value, Box<dyn Error>> {
        get_timeline_checkpoint_associations(self).await
    }

    async fn create_timeline_checkpoint(
        &self,
        timeline_id: i32,
        checkpoint_id: i32,
    ) -> Result<Value, Box<dyn Error>> {
        create_timeline_checkpoint(self, timeline_id, checkpoint_id).await
    }

    async fn delete_timeline_checkpoint(
        &self,
        timeline_id: i32,
        checkpoint_id: i32,
    ) -> Result<Value, Box<dyn Error>> {
        delete_timeline_checkpoint(self, timeline_id, checkpoint_id).await
    }

    async fn get_timeline_checkpoints_for_timeline(
        &self,
        timeline_id: i32,
    ) -> Result<Value, Box<dyn Error>> {
        get_timeline_checkpoints_for_timeline(self, timeline_id).await
    }

    async fn get_checkpoint_durations(&self) -> Result<Value, Box<dyn Error>> {
        get_checkpoint_durations(self).await
    }

    async fn get_checkpoint_durations_for_app(&self, app_id: i32) -> Result<Value, Box<dyn Error>> {
        get_checkpoint_durations_for_app(self, app_id).await
    }

    async fn update_checkpoint_duration(
        &self,
        checkpoint_id: i32,
        app_id: i32,
        duration: i32,
        sessions_count: i32,
    ) -> Result<Value, Box<dyn Error>> {
        update_checkpoint_duration(self, checkpoint_id, app_id, duration, sessions_count).await
    }

    async fn get_all_active_checkpoints_table(&self) -> Result<Value, Box<dyn Error>> {
        get_all_active_checkpoints_table(self).await
    }

    async fn get_active_checkpoints_for_app_table(&self, app_id: i32) -> Result<Value, Box<dyn Error>> {
        get_active_checkpoints_for_app_table(self, app_id).await
    }

    async fn activate_checkpoint(
        &self,
        checkpoint_id: i32,
        app_id: i32,
    ) -> Result<Value, Box<dyn Error>> {
        activate_checkpoint(self, checkpoint_id, app_id).await
    }

    async fn deactivate_checkpoint(
        &self,
        checkpoint_id: i32,
        app_id: i32,
    ) -> Result<Value, Box<dyn Error>> {
        deactivate_checkpoint(self, checkpoint_id, app_id).await
    }

    async fn is_checkpoint_active(
        &self,
        checkpoint_id: i32,
        app_id: i32,
    ) -> Result<bool, Box<dyn Error>> {
        is_checkpoint_active(self, checkpoint_id, app_id).await
    }
}
