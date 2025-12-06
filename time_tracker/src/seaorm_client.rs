use async_trait::async_trait;
use std::{error::Error, sync::Arc, time::Duration};
use log::{error, info};

use crossbeam_channel::Receiver;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectOptions, Database, DatabaseConnection,
    DbErr, EntityTrait, QueryFilter, QueryOrder, QuerySelect, RelationTrait,
};
use serde_json::{json, Value};

use crate::entities::{apps, checkpoints, timeline};
use crate::receive_types::ReceiveTypes;
use crate::restable::Restable;
use crate::seaorm_queries::{
    add_process_alias, create_checkpoint, delete_checkpoint, get_active_checkpoints,
    get_active_checkpoints_for_app, get_all_app_ids, get_all_checkpoints, get_all_process_aliases,
    get_checkpoint_durations_by_ids, get_checkpoints_for_app, get_process_aliases_by_app_names,
    get_process_aliases_for_app, get_session_count_for_app, get_timeline_with_checkpoints,
    remove_process_alias, set_checkpoint_active,
};
use crate::websocket::{broadcast_apps_update, broadcast_timeline_update, has_active_broadcaster};

#[derive(Clone)]
pub struct SeaORMClient {
    pub connection: Arc<DatabaseConnection>,
}

impl SeaORMClient {
    pub fn new_with_connection(connection: Arc<DatabaseConnection>) -> Self {
        Self { connection }
    }

    pub async fn new(url: &str) -> Result<Self, DbErr> {
        let mut opt = ConnectOptions::new(url.to_owned());
        opt.max_connections(100)
            .min_connections(5)
            .sqlx_logging(false)
            .connect_timeout(Duration::from_secs(8))
            .idle_timeout(Duration::from_secs(8));
        let db = Database::connect(opt).await?;
        Ok(Self {
            connection: Arc::new(db),
        })
    }

    fn handle_longest_session(&self, rt: &tokio::runtime::Runtime, item: &str) {
        let split: Vec<&str> = item.split(';').collect();
        if split.len() < 2 {
            return;
        }

        let app_name = split[0];
        let current_session = split[1].parse::<i32>().unwrap_or_else(|_| {
            error!("Failed to parse session duration: {}", split[1]);
            0
        });

        if let Err(e) = rt.block_on(async {
            let Some(app_model) = apps::Entity::find()
                .filter(apps::Column::Name.eq(app_name))
                .one(&*self.connection)
                .await?
            else {
                return Ok::<(), Box<dyn std::error::Error + Send + Sync>>(());
            };

            if current_session > app_model.longest_session.unwrap_or(0) {
                let mut app_active: apps::ActiveModel = app_model.into();
                app_active.longest_session = ActiveValue::Set(Some(current_session));
                let now_date = chrono::Local::now().naive_local().date();
                app_active.longest_session_on = ActiveValue::Set(Some(now_date));
                app_active.update(&*self.connection).await?;
                let date_str = format!("{}", now_date);
                info!("{}: longest_session -> {}", app_name, current_session);
                info!("{}: longest_session_on -> {}", app_name, date_str);
            }

            Ok(())
        }) {
            error!("Error updating longest session for {}: {}", app_name, e);
        }

        self.broadcast_apps_if_active(rt);
    }

    fn handle_duration(&self, rt: &tokio::runtime::Runtime, item: &str) {
        if let Err(e) = rt.block_on(async {
            let Some(app_model) = apps::Entity::find()
                .filter(apps::Column::Name.eq(item))
                .one(&*self.connection)
                .await
                .map_err(|e| format!("Database error finding app: {}", e))?
            else {
                return Ok::<(), Box<dyn std::error::Error + Send + Sync>>(());
            };

            let app_id = app_model.id;
            let new_duration = app_model.duration.unwrap_or(0) + 1;

            let mut app_active: apps::ActiveModel = app_model.into();
            app_active.duration = ActiveValue::Set(Some(new_duration));
            app_active
                .update(&*self.connection)
                .await
                .map_err(|e| format!("Database error updating app duration: {}", e))?;

            self.update_checkpoint_durations(app_id).await?;

            info!("{}: duration -> {}", item, new_duration);

            Ok(())
        }) {
            error!("Error updating duration for {}: {}", item, e);
        }

        self.broadcast_apps_if_active(rt);
    }

    fn handle_launches(&self, rt: &tokio::runtime::Runtime, item: &str) {
        if let Err(e) = rt.block_on(async {
            let Some(app_model) = apps::Entity::find()
                .filter(apps::Column::Name.eq(item))
                .one(&*self.connection)
                .await
                .map_err(|e| format!("Database error finding app: {}", e))?
            else {
                return Ok::<(), Box<dyn std::error::Error + Send + Sync>>(());
            };

            let app_id = app_model.id;
            let new_launches = app_model.launches.unwrap_or(0) + 1;

            let mut app_active: apps::ActiveModel = app_model.into();
            app_active.launches = ActiveValue::Set(Some(new_launches));
            app_active
                .update(&*self.connection)
                .await
                .map_err(|e| format!("Database error updating app launches: {}", e))?;

            self.update_checkpoint_sessions(app_id).await?;

            info!("{}: launches -> {}", item, new_launches);

            Ok(())
        }) {
            error!("Error updating launches for {}: {}", item, e);
        }

        self.broadcast_apps_if_active(rt);
    }

    fn handle_timeline(&self, rt: &tokio::runtime::Runtime, item: &str) {
        if let Err(e) = rt.block_on(async {
            let Some(app_model) = apps::Entity::find()
                .filter(apps::Column::Name.eq(item))
                .one(&*self.connection)
                .await?
            else {
                return Ok::<(), Box<dyn std::error::Error + Send + Sync>>(());
            };

            let app_id = app_model.id;
            let today = chrono::Local::now().naive_local().date();

            let timeline_entry = timeline::Entity::find()
                .filter(timeline::Column::AppId.eq(app_id))
                .filter(timeline::Column::Date.eq(today))
                .one(&*self.connection)
                .await?;

            if let Some(timeline_model) = timeline_entry {
                let new_duration = timeline_model.duration.unwrap_or(0) + 1;
                let mut timeline_active: timeline::ActiveModel = timeline_model.into();
                timeline_active.duration = ActiveValue::Set(Some(new_duration));
                timeline_active.update(&*self.connection).await?;
                info!("{}: timeline -> {}", item, new_duration);
            } else {
                let next_id = timeline::Entity::find()
                    .select_only()
                    .column_as(timeline::Column::Id.max(), "max_id")
                    .into_tuple::<Option<i32>>()
                    .one(&*self.connection)
                    .await?
                    .flatten()
                    .map_or(1, |id| id + 1);

                let checkpoint_id = self.get_first_active_checkpoint_id(app_id).await;

                let new_timeline = timeline::ActiveModel {
                    id: ActiveValue::Set(next_id),
                    date: ActiveValue::Set(today),
                    duration: ActiveValue::Set(Some(1)),
                    app_id: ActiveValue::Set(app_id),
                    checkpoint_id: ActiveValue::Set(checkpoint_id),
                };

                timeline::Entity::insert(new_timeline)
                    .exec(&*self.connection)
                    .await?;
                info!("{}: timeline -> {}", item, 1);
            }

            Ok(())
        }) {
            error!("Error updating timeline for {}: {}", item, e);
        }

        if has_active_broadcaster()
            && let Ok(timeline_result) = rt.block_on(self.get_timeline_data(None, 30))
            && let Ok(timeline_vec) =
                serde_json::from_value::<Vec<crate::structs::Timeline>>(timeline_result)
        {
            broadcast_timeline_update(timeline_vec);
        }
    }

    async fn update_checkpoint_durations(
        &self,
        app_id: i32,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let active_checkpoints_value = get_active_checkpoints_for_app(self, app_id)
            .await
            .map_err(|e| format!("Database error finding active checkpoints: {}", e))?;

        let Some(checkpoints_arr) = active_checkpoints_value.as_array() else {
            return Ok(());
        };

        let now = chrono::Local::now().naive_local();
        for checkpoint_value in checkpoints_arr {
            let Ok(checkpoint) =
                serde_json::from_value::<checkpoints::Model>(checkpoint_value.clone())
            else {
                continue;
            };

            let Some(checkpoint_model) = checkpoints::Entity::find_by_id(checkpoint.id)
                .one(&*self.connection)
                .await
                .ok()
                .flatten()
            else {
                continue;
            };

            let new_duration = checkpoint.duration.unwrap_or(0) + 1;
            let mut checkpoint_active: checkpoints::ActiveModel = checkpoint_model.into();
            checkpoint_active.duration = ActiveValue::Set(Some(new_duration));
            checkpoint_active.last_updated = ActiveValue::Set(Some(now));

            if let Err(e) = checkpoint_active.update(&*self.connection).await {
                error!("Database error updating checkpoint duration: {}", e);
            }
        }

        Ok(())
    }

    async fn update_checkpoint_sessions(
        &self,
        app_id: i32,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let active_checkpoints_value = get_active_checkpoints_for_app(self, app_id)
            .await
            .map_err(|e| format!("Database error finding active checkpoints: {}", e))?;

        let Some(checkpoints_arr) = active_checkpoints_value.as_array() else {
            return Ok(());
        };

        let now = chrono::Local::now().naive_local();
        for checkpoint_value in checkpoints_arr {
            let Ok(checkpoint) =
                serde_json::from_value::<checkpoints::Model>(checkpoint_value.clone())
            else {
                continue;
            };

            let Some(checkpoint_model) = checkpoints::Entity::find_by_id(checkpoint.id)
                .one(&*self.connection)
                .await
                .ok()
                .flatten()
            else {
                continue;
            };

            let new_sessions = checkpoint.sessions_count.unwrap_or(0) + 1;
            let mut checkpoint_active: checkpoints::ActiveModel = checkpoint_model.into();
            checkpoint_active.sessions_count = ActiveValue::Set(Some(new_sessions));
            checkpoint_active.last_updated = ActiveValue::Set(Some(now));

            if let Err(e) = checkpoint_active.update(&*self.connection).await {
                error!("Database error updating checkpoint sessions: {}", e);
            }
        }

        Ok(())
    }

    async fn get_first_active_checkpoint_id(&self, app_id: i32) -> Option<i32> {
        get_active_checkpoints_for_app(self, app_id)
            .await
            .ok()
            .and_then(|v| v.as_array().and_then(|arr| arr.first().cloned()))
            .and_then(|v| serde_json::from_value::<checkpoints::Model>(v).ok())
            .map(|c| c.id)
    }

    fn broadcast_apps_if_active(&self, rt: &tokio::runtime::Runtime) {
        if !has_active_broadcaster() {
            return;
        }

        if let Ok(apps_result) = rt.block_on(self.get_all_apps())
            && let Ok(apps_vec) = serde_json::from_value::<Vec<crate::structs::App>>(apps_result)
        {
            broadcast_apps_update(apps_vec);
        }
    }
}

#[async_trait]
impl Restable for SeaORMClient {
    async fn setup(&self) -> Result<(), Box<dyn Error>> {
        use sea_orm_migration::MigratorTrait;
        crate::migration::Migrator::up(&*self.connection, None).await?;
        Ok(())
    }

    async fn get_processes(&self) -> Result<Vec<String>, Box<dyn Error>> {
        let apps: Vec<apps::Model> = apps::Entity::find().all(&*self.connection).await?;
        Ok(apps.into_iter().filter_map(|app| app.name).collect())
    }

    async fn put_data(&self, item: &str, product_name: &str) -> Result<Value, Box<dyn Error>> {
        let max_id_result = apps::Entity::find()
            .select_only()
            .column_as(apps::Column::Id.max(), "max_id")
            .into_tuple::<Option<i32>>()
            .one(&*self.connection)
            .await?;

        let next_id = max_id_result.flatten().map_or(1, |id| id + 1);

        let app = apps::ActiveModel {
            id: ActiveValue::Set(next_id),
            duration: ActiveValue::Set(Some(0)),
            launches: ActiveValue::Set(Some(0)),
            longest_session: ActiveValue::Set(Some(0)),
            name: ActiveValue::Set(Some(item.to_owned())),
            product_name: ActiveValue::Set(Some(product_name.to_owned())),
            longest_session_on: ActiveValue::Set(None),
        };

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
        std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Runtime::new() else {
                eprintln!("Failed to create async runtime");
                return;
            };

            while let Ok((item, receive_type)) = rx.recv() {
                match receive_type {
                    ReceiveTypes::LongestSession => {
                        self.handle_longest_session(&rt, &item);
                    }
                    ReceiveTypes::Duration => {
                        self.handle_duration(&rt, &item);
                    }
                    ReceiveTypes::Launches => {
                        self.handle_launches(&rt, &item);
                    }
                    ReceiveTypes::Timeline => {
                        self.handle_timeline(&rt, &item);
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
        let cutoff_date =
            chrono::Local::now().naive_local().date() - chrono::Duration::days(days);

        let timeline_entries: Vec<timeline::Model> = match app_name {
            Some(name) => {
                timeline::Entity::find()
                    .join(sea_orm::JoinType::InnerJoin, timeline::Relation::Apps.def())
                    .filter(
                        apps::Column::Name
                            .eq(name)
                            .and(timeline::Column::Date.gte(cutoff_date)),
                    )
                    .order_by_desc(timeline::Column::Date)
                    .all(&*self.connection)
                    .await?
            }
            None => {
                timeline::Entity::find()
                    .filter(timeline::Column::Date.gte(cutoff_date))
                    .order_by_desc(timeline::Column::Date)
                    .all(&*self.connection)
                    .await?
            }
        };

        let json_values: Vec<Value> = timeline_entries
            .into_iter()
            .map(|entry| crate::seaorm_queries::model_to_json_with_context(entry, "timeline entry"))
            .collect();

        Ok(Value::Array(json_values))
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
