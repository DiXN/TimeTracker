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
use std::error::Error;

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

    for row in &connection.query(item, &[]).unwrap() {
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
    &self.connection.lock().unwrap().execute(r#"
      INSERT INTO apps VALUES ((SELECT id + 1 as id
        FROM apps a
        ORDER BY id DESC
        LIMIT 1
      ), 0, 0, 0, $1, &2);"#
    , &[&item, &product_name])?;

    Ok(json!({"insert": item}))
  }

  fn delete_data(&self, item: &str) -> Result<Value, Box<dyn Error>> {
    Ok(json!(""))
  }

  fn patch_data(&self, item: &str, value: &Value) -> Result<Value, Box<dyn Error>> {
    Ok(json!(""))
  }

  fn init_event_loop(self, rx: Receiver<(String, ReceiveTypes)>) {

  }
}
