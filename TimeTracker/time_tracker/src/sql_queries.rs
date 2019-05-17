use std::error::Error;

use crate::sql::PgClient;

pub fn update_timeline(client: &PgClient, inc: i32, item: &str, date_str: &str) -> Result<u64, Box<dyn Error>> {
  let connection = client.connection.lock().unwrap();

  Ok(connection.execute(
    &format!(
      "UPDATE timeline
      SET duration = {}
      WHERE app_id = (
        SELECT a.id FROM apps a
        WHERE a.name = '{}' AND date = '{}'
      ) "
  , inc, item, date_str), &[])?)
}

pub fn insert_timeline(client: &PgClient, date_str: &str, item: &str) -> Result<u64, Box<dyn Error>> {
  let connection = client.connection.lock().unwrap();

  Ok(connection.execute(
    &format!(
      "INSERT INTO timeline VALUES ((SELECT id + 1 as id
        FROM timeline t
        ORDER BY id DESC
        LIMIT 1
      ), '{}', 1, (
        SELECT a.id FROM apps a
        WHERE a.name = '{}'
      ))"
  , date_str, item), &[])?)
}

pub fn update_longest_session(client: &PgClient, current_session: i32, item: &str) -> Result<u64, Box<dyn Error>> {
  let connection = client.connection.lock().unwrap();

  Ok(connection.execute(
    &format!(
    "UPDATE apps
    SET longest_session = {}
    WHERE name = '{}'"
  , current_session, item), &[])?)
}

pub fn update_apps_generic(client: &PgClient, inc_type: &str,  inc: i32, item: &str) -> Result<u64, Box<dyn Error>> {
  let connection = client.connection.lock().unwrap();

  Ok(connection.execute(
    &format!(
      "UPDATE apps
      SET {} = {}
      WHERE name = '{}'"
  , inc_type, inc, item), &[])?)
}

pub fn get_timeline_duration(client: &PgClient, item: &str, date_str: &str) -> Option<i32> {
  client.get_single_value::<i32>(
    &format!(
      "SELECT t.duration FROM timeline t
      JOIN apps a on t.app_id = a.id
      WHERE a.name = '{}' AND date = '{}'"
    , item, date_str)
  )
}

pub fn get_longest_session(client: &PgClient, item: &str) -> Option<i32> {
  client.get_single_value::<i32>(
    &format!(
      "SELECT longest_session FROM apps
      WHERE name = '{}'"
    , item)
  )
}

pub fn get_number_from_apps(client: &PgClient, inc_type: &str, item: &str) -> Option<i32> {
  client.get_single_value::<i32>(
    &format!(
      "SELECT {} FROM apps
      WHERE name = '{}'"
    , inc_type, item)
  )
}
