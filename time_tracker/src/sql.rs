use std::{
    error::Error,
    sync::{Arc, Mutex},
    thread,
};

use postgres::{
    Connection, TlsMode,
    types::{BOOL, DATE, FromSql, INT4, INT8, TIMESTAMP, VARCHAR},
};

use chrono::prelude::*;
use chrono::{NaiveDate, NaiveDateTime};
use crossbeam_channel::Receiver;
use serde_json::{Value, json};

use barrel::{Migration, backend::Pg, types};

use crate::receive_types::ReceiveTypes;
use crate::restable::Restable;

use crate::sql_queries::{
    activate_checkpoint, create_checkpoint, create_timeline_checkpoint, deactivate_checkpoint,
    delete_checkpoint, delete_timeline_checkpoint, get_active_checkpoints,
    get_active_checkpoints_for_app, get_active_checkpoints_for_app_table,
    get_all_active_checkpoints_table, get_all_checkpoints, get_all_timeline,
    get_checkpoint_durations, get_checkpoint_durations_for_app, get_checkpoints_for_app,
    get_longest_session, get_number_from_apps, get_session_count_for_app, get_all_app_ids,
    get_timeline_checkpoint_associations, get_timeline_checkpoints_for_timeline,
    get_timeline_duration, insert_timeline, is_checkpoint_active, set_checkpoint_active,
    update_apps_generic, update_checkpoint_duration, update_longest_session,
    update_longest_session_on, update_timeline,
};

#[derive(Clone)]
pub struct PgClient {
    pub connection: Arc<Mutex<Connection>>,
}

impl PgClient {
    pub fn new(url: &str, database: &str) -> PgClient {
        if let Ok(basic) = Connection::connect(url, TlsMode::None) {
            if let Ok(with_db) = Connection::connect(format!("{}/{}", url, database), TlsMode::None)
            {
                PgClient {
                    connection: Arc::new(Mutex::new(with_db)),
                }
            } else {
                basic
                    .batch_execute("CREATE DATABASE time_tracker")
                    .expect("Cannot create Database for \"time_tracker\".");

                let connection =
                    Connection::connect(format!("{}/{}", url, database), TlsMode::None).unwrap();

                PgClient {
                    connection: Arc::new(Mutex::new(connection)),
                }
            }
        } else {
            panic!(
                "Could not connect to Postgres server. Check connection string and if Postgres is running."
            )
        }
    }

    pub fn get_single_value<T: FromSql>(&self, query: &str) -> Option<T> {
        let connection = self.connection.lock().ok()?;

        if let Ok(row) = connection.query(query, &[]) {
            if let Some(col) = row.iter().next() {
                Some(col.get::<_, T>(0))
            } else {
                None
            }
        } else {
            None
        }
    }
}

impl Restable for PgClient {
    fn setup(&self) -> Result<(), Box<dyn Error>> {
        let connection = self
            .connection
            .lock()
            .map_err(|e| format!("Failed to acquire connection lock: {}", e))?;

        let mut m = Migration::new();

        m.create_table_if_not_exists("apps", |t| {
            t.add_column("id", types::integer().primary(true));
            t.add_column("duration", types::integer());
            t.add_column("launches", types::integer());
            t.add_column("longest_session", types::integer());
            t.add_column("name", types::varchar(255).nullable(true));
            t.add_column("product_name", types::varchar(255).nullable(true));
            t.add_column("longest_session_on", types::date().nullable(true));
        });

        m.create_table_if_not_exists("timeline", |t| {
            t.add_column("id", types::integer().primary(true));
            t.add_column("date", types::date());
            t.add_column("duration", types::integer());
            t.inject_custom("app_id INTEGER NOT NULL REFERENCES APPS(id) ON DELETE CASCADE");
        });

        m.create_table_if_not_exists("checkpoints", |t| {
            t.add_column("id", types::integer().primary(true));
            t.add_column("name", types::varchar(255).nullable(false));
            t.add_column("description", types::text().nullable(true));
            t.add_column(
                "created_at",
                types::custom("timestamp")
                    .nullable(true)
                    .default("CURRENT_TIMESTAMP"),
            );
            t.add_column(
                "valid_from",
                types::custom("timestamp")
                    .nullable(false)
                    .default("CURRENT_TIMESTAMP"),
            );
            t.add_column("color", types::varchar(7).nullable(true));
            t.add_column("app_id", types::integer().nullable(false));
            t.add_column(
                "is_active",
                types::boolean().nullable(true).default("false"),
            );
        });

        m.create_table_if_not_exists("timeline_checkpoints", |t| {
            t.add_column("id", types::integer().primary(true));
            t.add_column("timeline_id", types::integer().nullable(false));
            t.add_column("checkpoint_id", types::integer().nullable(false));
            t.add_column(
                "created_at",
                types::custom("timestamp")
                    .nullable(true)
                    .default("CURRENT_TIMESTAMP"),
            );
            t.inject_custom("FOREIGN KEY (timeline_id) REFERENCES timeline(id) ON DELETE CASCADE");
            t.inject_custom(
                "FOREIGN KEY (checkpoint_id) REFERENCES checkpoints(id) ON DELETE CASCADE",
            );
        });

        m.create_table_if_not_exists("checkpoint_durations", |t| {
            t.add_column("id", types::integer().primary(true));
            t.add_column("checkpoint_id", types::integer().nullable(false));
            t.add_column("app_id", types::integer().nullable(false));
            t.add_column("duration", types::integer().nullable(true).default("0"));
            t.add_column(
                "sessions_count",
                types::integer().nullable(true).default("0"),
            );
            t.add_column(
                "last_updated",
                types::custom("timestamp")
                    .nullable(true)
                    .default("CURRENT_TIMESTAMP"),
            );
            t.inject_custom(
                "FOREIGN KEY (checkpoint_id) REFERENCES checkpoints(id) ON DELETE CASCADE",
            );
            t.inject_custom("FOREIGN KEY (app_id) REFERENCES apps(id) ON DELETE CASCADE");
        });

        m.create_table_if_not_exists("active_checkpoints", |t| {
            t.add_column("id", types::integer().primary(true));
            t.add_column("checkpoint_id", types::integer().nullable(false));
            t.add_column(
                "activated_at",
                types::custom("timestamp")
                    .nullable(true)
                    .default("CURRENT_TIMESTAMP"),
            );
            t.add_column("app_id", types::integer().nullable(false));
            t.inject_custom(
                "FOREIGN KEY (checkpoint_id) REFERENCES checkpoints(id) ON DELETE CASCADE",
            );
            t.inject_custom("FOREIGN KEY (app_id) REFERENCES apps(id) ON DELETE CASCADE");
        });

        connection.batch_execute(&m.make::<Pg>())?;

        Ok(())
    }

    fn get_data(&self, item: &str) -> Result<Value, Box<dyn Error>> {
        let connection = self
            .connection
            .lock()
            .map_err(|e| format!("Failed to acquire connection lock: {}", e))?;

        let mut col = Vec::new();

        for row in &connection.query(item, &[])? {
            let mut map = std::collections::HashMap::new();
            for (idx, column) in row.columns().iter().enumerate() {
                let value = match column.type_() {
                    &DATE => match row.get::<_, Option<NaiveDate>>(idx) {
                        Some(date) => date.to_string(),
                        None => String::new(),
                    },
                    &TIMESTAMP => match row.get::<_, Option<NaiveDateTime>>(idx) {
                        Some(timestamp) => timestamp.to_string(),
                        None => String::new(),
                    },
                    &BOOL => match row.get::<_, Option<bool>>(idx) {
                        Some(b) => b.to_string(),
                        None => String::new(),
                    },
                    &VARCHAR => match row.get::<_, Option<String>>(idx) {
                        Some(s) => s,
                        None => String::new(),
                    },
                    &INT4 => match row.get::<_, Option<i32>>(idx) {
                        Some(i) => i.to_string(),
                        None => String::new(),
                    },
                    &INT8 => match row.get::<_, Option<i64>>(idx) {
                        Some(i) => i.to_string(),
                        None => String::new(),
                    },
                    _ => match row.get::<_, Option<String>>(idx) {
                        Some(s) => s,
                        None => String::new(),
                    },
                };

                map.insert(column.name().to_string(), value);
            }

            col.push(json!(map));
        }

        Ok(Value::Array(col))
    }

    fn get_processes(&self) -> Result<Vec<String>, Box<dyn Error>> {
        let data = self.get_data("SELECT name from apps")?;
        let array = data.as_array().ok_or("Expected array from query")?;

        let processes = array
            .iter()
            .filter_map(|p| {
                p.as_object()
                    .and_then(|obj| obj.get("name"))
                    .and_then(|name| name.as_str())
                    .map(|s| s.to_owned())
            })
            .collect();

        Ok(processes)
    }

    fn put_data(&self, item: &str, product_name: &str) -> Result<Value, Box<dyn Error>> {
        let id = match self.get_single_value::<i32>(
            "(SELECT id + 1 as id
      FROM apps a
      ORDER BY id DESC
      LIMIT 1
    )",
        ) {
            Some(id) => id,
            None => 0,
        };

        let connection = self
            .connection
            .lock()
            .map_err(|e| format!("Failed to acquire connection lock: {}", e))?;
        connection.execute(
            &format!(
                "INSERT INTO apps VALUES ({}, 0, 0, 0, '{}', '{}', NULL);",
                id, item, product_name
            ),
            &[],
        )?;

        Ok(json!({"insert": item}))
    }

    fn delete_data(&self, item: &str) -> Result<Value, Box<dyn Error>> {
        let connection = self
            .connection
            .lock()
            .map_err(|e| format!("Failed to acquire connection lock: {}", e))?;

        connection.execute(
            &format!(
                "DELETE FROM apps
      WHERE name = '{}'",
                item
            ),
            &[],
        )?;

        Ok(json!({"delete": item}))
    }

    fn init_event_loop(self, rx: Receiver<(String, ReceiveTypes)>) {
        thread::spawn(move || {
            let patch_increment = |item: &str, inc_type: &str| {
                if let Some(mut inc) = get_number_from_apps(&self, &inc_type, &item) {
                    inc += 1i32;

                    if update_apps_generic(&self, inc_type, inc, item).is_ok() {
                        info!("{}: {} -> {}", item, inc_type, inc);
                    } else {
                        error!("could not update \"{}\" for {}", inc_type, item);
                    }
                }
            };

            while let Ok(rx) = rx.recv() {
                match rx {
                    (item, ReceiveTypes::LONGEST_SESSION) => {
                        let split = item.split(';').collect::<Vec<&str>>();
                        let item = split[0];

                        if let Some(longest_session) = get_longest_session(&self, &item) {
                            let current_session = split[1].parse::<i32>().unwrap();

                            if current_session > longest_session {
                                let dt = Local::now();
                                let date_str = format!("{}-{}-{}", dt.year(), dt.month(), dt.day());

                                if update_longest_session(&self, current_session, &item).is_ok() {
                                    info!("{}: longest_session -> {}", item, current_session);
                                } else {
                                    error!("could not update \"longest_session\" for {}", item);
                                }

                                if update_longest_session_on(&self, &date_str, &item).is_ok() {
                                    info!("{}: longest_session_on -> {}", item, date_str);
                                } else {
                                    error!("could not update \"longest_session_on\" for {}", item);
                                }
                            }
                        }
                    }
                    (item, ReceiveTypes::DURATION) => patch_increment(&item, "duration"),
                    (item, ReceiveTypes::LAUNCHES) => patch_increment(&item, "launches"),
                    (item, ReceiveTypes::TIMELINE) => {
                        let dt = Local::now();
                        let date_str = format!("{}-{}-{}", dt.year(), dt.month(), dt.day());

                        if let Some(mut inc) = get_timeline_duration(&self, &item, &date_str) {
                            inc += 1i32;

                            if update_timeline(&self, inc, &item, &date_str).is_ok() {
                                info!("{}: timeline -> {}", item, inc);
                            } else {
                                error!("could not update \"timeline\" for {}", item);
                            }
                        } else {
                            if insert_timeline(&self, &date_str, &item).is_ok() {
                                info!("{}: timeline -> {}", item, 1);
                            } else {
                                error!("could not insert into \"timeline\" for {}", item);
                            }
                        }
                    }
                }
            }
        });
    }

    fn get_all_apps(&self) -> Result<Value, Box<dyn Error>> {
        self.get_data("SELECT id, duration, launches, longest_session, name, product_name, longest_session_on FROM apps ORDER BY id")
    }

    fn get_timeline_data(
        &self,
        app_name: Option<&str>,
        days: i64,
    ) -> Result<Value, Box<dyn Error>> {
        let query = if let Some(name) = app_name {
            format!(
                "SELECT t.id, t.date, t.duration, t.app_id
                 FROM timeline t
                 JOIN apps a ON t.app_id = a.id
                 WHERE a.name = '{}'
                 AND t.date >= CURRENT_DATE - INTERVAL '{} days'
                 ORDER BY t.date DESC",
                name, days
            )
        } else {
            format!(
                "SELECT t.id, t.date, t.duration, t.app_id
                 FROM timeline t
                 WHERE t.date >= CURRENT_DATE - INTERVAL '{} days'
                 ORDER BY t.date DESC",
                days
            )
        };

        self.get_data(&query)
    }

    fn get_session_count_for_app(&self, app_id: i32) -> Result<i32, Box<dyn Error>> {
        get_session_count_for_app(self, app_id)
    }

    fn get_all_app_ids(&self) -> Result<Vec<i32>, Box<dyn Error>> {
        get_all_app_ids(self)
    }

    fn get_all_checkpoints(&self) -> Result<Value, Box<dyn Error>> {
        get_all_checkpoints(self)
    }

    fn get_checkpoints_for_app(&self, app_id: i32) -> Result<Value, Box<dyn Error>> {
        get_checkpoints_for_app(self, app_id)
    }

    fn create_checkpoint(
        &self,
        name: &str,
        description: Option<&str>,
        app_id: i32,
    ) -> Result<Value, Box<dyn Error>> {
        create_checkpoint(self, name, description, app_id)?;
        Ok(json!({"success": "Checkpoint created successfully"}))
    }

    fn set_checkpoint_active(
        &self,
        checkpoint_id: i32,
        is_active: bool,
    ) -> Result<Value, Box<dyn Error>> {
        set_checkpoint_active(self, checkpoint_id, is_active)?;
        Ok(json!({"success": "Checkpoint status updated successfully"}))
    }

    fn delete_checkpoint(&self, checkpoint_id: i32) -> Result<Value, Box<dyn Error>> {
        delete_checkpoint(self, checkpoint_id)?;
        Ok(json!({"success": "Checkpoint deleted successfully"}))
    }

    fn get_active_checkpoints(&self) -> Result<Value, Box<dyn Error>> {
        get_active_checkpoints(self)
    }

    fn get_active_checkpoints_for_app(&self, app_id: i32) -> Result<Value, Box<dyn Error>> {
        get_active_checkpoints_for_app(self, app_id)
    }

    fn get_all_timeline(&self) -> Result<Value, Box<dyn Error>> {
        get_all_timeline(self)
    }

    fn get_timeline_checkpoint_associations(&self) -> Result<Value, Box<dyn Error>> {
        get_timeline_checkpoint_associations(self)
    }

    // Timeline checkpoint methods
    fn create_timeline_checkpoint(
        &self,
        timeline_id: i32,
        checkpoint_id: i32,
    ) -> Result<Value, Box<dyn Error>> {
        create_timeline_checkpoint(self, timeline_id, checkpoint_id)?;
        Ok(json!({"success": "Timeline checkpoint association created"}))
    }

    fn delete_timeline_checkpoint(
        &self,
        timeline_id: i32,
        checkpoint_id: i32,
    ) -> Result<Value, Box<dyn Error>> {
        delete_timeline_checkpoint(self, timeline_id, checkpoint_id)?;
        Ok(json!({"success": "Timeline checkpoint association deleted"}))
    }

    fn get_timeline_checkpoints_for_timeline(
        &self,
        timeline_id: i32,
    ) -> Result<Value, Box<dyn Error>> {
        get_timeline_checkpoints_for_timeline(self, timeline_id)
    }

    // Checkpoint duration methods
    fn get_checkpoint_durations(&self) -> Result<Value, Box<dyn Error>> {
        get_checkpoint_durations(self)
    }

    fn get_checkpoint_durations_for_app(&self, app_id: i32) -> Result<Value, Box<dyn Error>> {
        get_checkpoint_durations_for_app(self, app_id)
    }

    fn update_checkpoint_duration(
        &self,
        checkpoint_id: i32,
        app_id: i32,
        duration: i32,
        sessions_count: i32,
    ) -> Result<Value, Box<dyn Error>> {
        update_checkpoint_duration(self, checkpoint_id, app_id, duration, sessions_count)?;
        Ok(json!({"success": "Checkpoint duration updated"}))
    }

    // Active checkpoint table methods
    fn get_all_active_checkpoints_table(&self) -> Result<Value, Box<dyn Error>> {
        get_all_active_checkpoints_table(self)
    }

    fn get_active_checkpoints_for_app_table(&self, app_id: i32) -> Result<Value, Box<dyn Error>> {
        get_active_checkpoints_for_app_table(self, app_id)
    }

    fn activate_checkpoint(
        &self,
        checkpoint_id: i32,
        app_id: i32,
    ) -> Result<Value, Box<dyn Error>> {
        activate_checkpoint(self, checkpoint_id, app_id)?;
        Ok(json!({"success": "Checkpoint activated"}))
    }

    fn deactivate_checkpoint(
        &self,
        checkpoint_id: i32,
        app_id: i32,
    ) -> Result<Value, Box<dyn Error>> {
        deactivate_checkpoint(self, checkpoint_id, app_id)?;
        Ok(json!({"success": "Checkpoint deactivated"}))
    }

    fn is_checkpoint_active(
        &self,
        checkpoint_id: i32,
        app_id: i32,
    ) -> Result<bool, Box<dyn Error>> {
        is_checkpoint_active(self, checkpoint_id, app_id)
    }
}
