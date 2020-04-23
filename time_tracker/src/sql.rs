use std::{
  sync::{Arc, Mutex},
  thread,
  error::Error
};

use postgres::{
  Connection,
  TlsMode,
  types::{
    INT4,
    INT8,
    VARCHAR,
    DATE,
    FromSql
  }
};

use chrono::prelude::*;
use chrono::NaiveDate;
use serde_json::{json, Value};
use crossbeam_channel::Receiver;

use barrel::{
  types,
  backend::Pg,
  Migration
};

use crate::restable::Restable;
use crate::receive_types::ReceiveTypes;

use crate::sql_queries::{
  update_timeline,
  insert_timeline,
  update_longest_session,
  update_longest_session_on,
  update_apps_generic,
  get_timeline_duration,
  get_longest_session,
  get_number_from_apps
};

#[derive(Clone)]
pub struct PgClient {
  pub connection: Arc<Mutex<Connection>>
}

impl PgClient {
  pub fn new(url: &str, database: &str) -> PgClient {
    if let Ok(basic) = Connection::connect(url, TlsMode::None) {
      if let Ok(with_db) = Connection::connect(format!("{}/{}", url, database), TlsMode::None) {
        PgClient {
          connection: Arc::new(Mutex::new(with_db))
        }
      } else {
        basic.batch_execute("CREATE DATABASE time_tracker").expect("Cannot create Database for \"time_tracker\".");

        let connection = Connection::connect(format!("{}/{}", url, database), TlsMode::None).unwrap();

        PgClient {
          connection: Arc::new(Mutex::new(connection))
        }
      }
    } else {
      panic!("Could not connect to Postgres server. Check connection string and if Postgres is running.")
    }
  }

  pub fn get_single_value<T : FromSql>(&self, query: &str) -> Option<T> {
    let connection = &self.connection.lock().unwrap();

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
    let connection = &self.connection.lock().unwrap();

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

    connection.batch_execute(&m.make::<Pg>())?;

    Ok(())
  }

  fn get_data(&self, item: &str) -> Result<Value, Box<dyn Error>> {
    let connection = &self.connection.lock().unwrap();
    let mut col = Vec::new();

    for row in &connection.query(item, &[])? {
      let mut map = std::collections::HashMap::new();
      for (idx, column) in row.columns().iter().enumerate() {
        let value = match column.type_() {
          &DATE => row.get::<_, NaiveDate>(idx).to_string(),
          &VARCHAR => row.get::<_, String>(idx),
          &INT4 => row.get::<_, i32>(idx).to_string(),
          &INT8 => row.get::<_, i64>(idx).to_string(),
          _ => row.get::<_, i64>(idx).to_string()
        };

        map.insert(column.name().to_string(), value);
      }

      col.push(json!(map));
    }

    Ok(Value::Array(col))
  }

  fn get_processes(&self) -> Result<Vec<String>, Box<dyn Error>> {
    Ok(self.get_data("SELECT name from apps")?
      .as_array()
      .unwrap()
      .iter()
      .map(|p| {
        let obj = p.as_object().unwrap();
        obj["name"].as_str().unwrap().to_owned()
      }).collect::<Vec<String>>())
  }

  fn put_data(&self, item: &str, product_name: &str) -> Result<Value, Box<dyn Error>> {
    let id = match self.get_single_value::<i32>("(SELECT id + 1 as id
      FROM apps a
      ORDER BY id DESC
      LIMIT 1
    )") {
      Some(id) => id,
      None => 0
    };

    &self.connection.lock().unwrap().execute(&format!("INSERT INTO apps VALUES ({}, 0, 0, 0, '{}', '{}', NULL);", id, item, product_name) , &[])?;

    Ok(json!({"insert": item}))
  }

  fn delete_data(&self, item: &str) -> Result<Value, Box<dyn Error>> {
    let connection = &self.connection.lock().unwrap();

    connection.execute(&format!(
      "DELETE FROM apps
      WHERE name = '{}'", item), &[])?;

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
          },
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
}
