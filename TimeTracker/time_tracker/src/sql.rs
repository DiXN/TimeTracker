use postgres::{
  Connection,
  TlsMode,
  types::{
    INT4,
    INT8,
    VARCHAR,
    DATE
  }
};

use std::sync::{Arc, Mutex};
use std::thread;
use std::error::Error;

use chrono::prelude::*;
use chrono::NaiveDate;
use serde_json::{json, Value};
use crossbeam_channel::Receiver;

use crate::restable::Restable;
use crate::receive_types::ReceiveTypes;

#[derive(Clone)]
pub struct PgClient {
  pub connection: Arc<Mutex<Connection>>
}

impl PgClient {
  pub fn new(database_url: &str) -> PgClient {
    let sql_connection = Connection::connect(database_url, TlsMode::None).unwrap();

    PgClient {
      connection: Arc::new(Mutex::new(sql_connection))
    }
  }
}

impl Restable for PgClient {
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
    &self.connection.lock().unwrap().execute(&format!("
      INSERT INTO apps VALUES ((SELECT id + 1 as id
        FROM apps a
        ORDER BY id DESC
        LIMIT 1
      ), 0, 0, 0, '{}', '{}');", item, product_name)
    , &[])?;

    Ok(json!({"insert": item}))
  }

  fn delete_data(&self, item: &str) -> Result<Value, Box<dyn Error>> {
    let connection = &self.connection.lock().unwrap();

    connection.execute(&format!(
      "DELETE FROM apps
      WHERE name = '{}'", item), &[])?;

    Ok(json!({"delete": item}))
  }

  fn patch_data(&self, item: &str, value: &Value) -> Result<Value, Box<dyn Error>> {

    Ok(json!(""))
  }

  fn init_event_loop(self, rx: Receiver<(String, ReceiveTypes)>) {
    thread::spawn(move || {
      let patch_increment = |item: &str, inc_type: &str| {
        let connection = &self.connection.lock().unwrap();
        if let Ok(row) = connection.query(&format!(
          "SELECT {} FROM apps
          WHERE name = '{}'"
          , inc_type, item), &[]) {
          if let Some(col) = row.iter().next() {
            let mut inc = col.get::<_, i32>(0);
            inc += 1i32;

            if let Ok(_) = connection.execute(&format!(
              "UPDATE apps
              SET {} = {}
              WHERE name = '{}'"
              , inc_type, inc, item), &[]) {
              info!("{}: {} -> {}", item, inc_type, inc);
            } else {
              error!("could not update \"{}\" for {}", inc_type, item);
            }
          }
        }
      };

      while let Ok(rx) = rx.recv() {
        match rx {
          (item, ReceiveTypes::LONGEST_SESSION) => {
            let split = item.split(";").collect::<Vec<&str>>();
            let connection = &self.connection.lock().unwrap();
            let item = split[0];

            if let Ok(row) = connection.query(&format!(
              "SELECT longest_session FROM apps
              WHERE name = '{}'", item), &[]) {
              if let Some(col) = row.iter().next() {
                let longest_session = col.get::<_, i32>(0);
                let current_session = split[1].parse::<i32>().unwrap();

                if current_session > longest_session {
                  if let Ok(_) = connection.execute(&format!(
                    "UPDATE apps
                    SET longest_session = {}
                    WHERE name = '{}'"
                    , current_session, item), &[]) {
                    info!("{}: longest_session -> {}", item, current_session);
                  } else {
                    error!("could not update \"longest_session\" for {}", item);
                  }
                }
              }
            }
          },
          (item, ReceiveTypes::DURATION) => patch_increment(&item, "duration"),
          (item, ReceiveTypes::LAUNCHES) => patch_increment(&item, "launches"),
          (item, ReceiveTypes::TIMELINE) => {
            let dt = Local::now();
            let date_str = format!("{}-{}-{}", dt.year(), dt.month(), dt.day());

            let connection = &self.connection.lock().unwrap();

            if let Ok(row) = connection.query(&format!(
              "SELECT t.duration FROM timeline t
              JOIN apps a on t.app_id = a.id
              WHERE a.name = '{}' AND date = '{}'", item, date_str), &[]) {
              if let Some(col) = row.iter().next() {
                let mut inc = col.get::<_, i32>(0);
                inc += 1i32;

                if let Ok(_) = connection.execute(&format!(
                  "UPDATE timeline
                  SET duration = {}
                  WHERE app_id = (
                    SELECT a.id FROM apps a
                    WHERE a.name = '{}' AND date = '{}'
                  )"
                  , inc, item, date_str), &[]) {
                  info!("{}: timeline -> {}", item, inc);
                } else {
                  error!("could not update \"timeline\" for {}", item);
                }
              } else {
                if let Ok(_) = connection.execute(&format!(
                  "INSERT INTO timeline VALUES ((SELECT id + 1 as id
                    FROM timeline t
                    ORDER BY id DESC
                    LIMIT 1
                  ), '{}', 1, (
                    SELECT a.id FROM apps a
                    WHERE a.name = '{}'
                  ))", date_str, item), &[]) {
                  info!("{}: timeline -> {}", item, 1);
                } else {
                  error!("could not insert into \"timeline\" for {}", item);
                }
              }
            }
          }
        }
      }
    });
  }
}
