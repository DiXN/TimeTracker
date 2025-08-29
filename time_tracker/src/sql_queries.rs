use std::error::Error;

use crate::restable::Restable;
use crate::sql::PgClient;

pub fn update_timeline(
    client: &PgClient,
    inc: i32,
    item: &str,
    date_str: &str,
) -> Result<u64, Box<dyn Error>> {
    let connection = client
        .connection
        .lock()
        .map_err(|e| format!("Failed to acquire connection lock: {}", e))?;

    Ok(connection.execute(
        &format!(
            "UPDATE timeline
      SET duration = {}
      WHERE app_id = (
        SELECT a.id FROM apps a
        WHERE a.name = '{}' AND date = '{}'
      ) ",
            inc, item, date_str
        ),
        &[],
    )?)
}

pub fn insert_timeline(
    client: &PgClient,
    date_str: &str,
    item: &str,
) -> Result<u64, Box<dyn Error>> {
    let id = match client.get_single_value::<i32>(
        "(SELECT id + 1 as id
    FROM timeline t
    ORDER BY id DESC
    LIMIT 1
  )",
    ) {
        Some(id) => id,
        None => 0,
    };

    let connection = client
        .connection
        .lock()
        .map_err(|e| format!("Failed to acquire connection lock: {}", e))?;

    Ok(connection.execute(
        &format!(
            "INSERT INTO timeline VALUES ({}, '{}', 1, (
        SELECT a.id FROM apps a
        WHERE a.name = '{}'
      ))",
            id, date_str, item
        ),
        &[],
    )?)
}

pub fn update_longest_session(
    client: &PgClient,
    current_session: i32,
    item: &str,
) -> Result<u64, Box<dyn Error>> {
    let connection = client
        .connection
        .lock()
        .map_err(|e| format!("Failed to acquire connection lock: {}", e))?;

    Ok(connection.execute(
        &format!(
            "UPDATE apps
    SET longest_session = {}
    WHERE name = '{}'",
            current_session, item
        ),
        &[],
    )?)
}

pub fn update_longest_session_on(
    client: &PgClient,
    today: &str,
    item: &str,
) -> Result<u64, Box<dyn Error>> {
    let connection = client
        .connection
        .lock()
        .map_err(|e| format!("Failed to acquire connection lock: {}", e))?;

    Ok(connection.execute(
        &format!(
            "UPDATE apps
    SET longest_session_on = '{}'
    WHERE name = '{}'",
            today, item
        ),
        &[],
    )?)
}

pub fn update_apps_generic(
    client: &PgClient,
    inc_type: &str,
    inc: i32,
    item: &str,
) -> Result<u64, Box<dyn Error>> {
    let connection = client
        .connection
        .lock()
        .map_err(|e| format!("Failed to acquire connection lock: {}", e))?;

    Ok(connection.execute(
        &format!(
            "UPDATE apps
      SET {} = {}
      WHERE name = '{}'",
            inc_type, inc, item
        ),
        &[],
    )?)
}

pub fn get_timeline_duration(client: &PgClient, item: &str, date_str: &str) -> Option<i32> {
    client.get_single_value::<i32>(&format!(
        "SELECT t.duration FROM timeline t
      JOIN apps a on t.app_id = a.id
      WHERE a.name = '{}' AND date = '{}'",
        item, date_str
    ))
}

pub fn get_longest_session(client: &PgClient, item: &str) -> Option<i32> {
    client.get_single_value::<i32>(&format!(
        "SELECT longest_session FROM apps
      WHERE name = '{}'",
        item
    ))
}

pub fn get_number_from_apps(client: &PgClient, inc_type: &str, item: &str) -> Option<i32> {
    client.get_single_value::<i32>(&format!(
        "SELECT {} FROM apps
      WHERE name = '{}'",
        inc_type, item
    ))
}

// Checkpoint SQL queries
pub fn create_checkpoint(
    client: &PgClient,
    name: &str,
    description: Option<&str>,
    app_id: i32,
) -> Result<u64, Box<dyn Error>> {
    let id = match client
        .get_single_value::<i32>("(SELECT COALESCE(MAX(id), 0) + 1 FROM checkpoints)")
    {
        Some(id) => id,
        None => 1,
    };

    let description_value = description.unwrap_or("");
    let connection = client
        .connection
        .lock()
        .map_err(|e| format!("Failed to acquire connection lock: {}", e))?;

    Ok(connection.execute(
        &format!(
            "INSERT INTO checkpoints (id, name, description, created_at, valid_from, color, app_id, is_active)
             VALUES ({}, '{}', '{}', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, '#2196F3', {}, false)",
            id, name, description_value, app_id
        ),
        &[],
    )?)
}

pub fn set_checkpoint_active(
    client: &PgClient,
    checkpoint_id: i32,
    is_active: bool,
) -> Result<u64, Box<dyn Error>> {
    let connection = client
        .connection
        .lock()
        .map_err(|e| format!("Failed to acquire connection lock: {}", e))?;

    Ok(connection.execute(
        &format!(
            "UPDATE checkpoints
             SET is_active = {}
             WHERE id = {}",
            is_active, checkpoint_id
        ),
        &[],
    )?)
}

pub fn delete_checkpoint(client: &PgClient, checkpoint_id: i32) -> Result<u64, Box<dyn Error>> {
    let connection = client
        .connection
        .lock()
        .map_err(|e| format!("Failed to acquire connection lock: {}", e))?;

    Ok(connection.execute(
        &format!(
            "DELETE FROM checkpoints
             WHERE id = {}",
            checkpoint_id
        ),
        &[],
    )?)
}

pub fn get_all_checkpoints(client: &PgClient) -> Result<serde_json::Value, Box<dyn Error>> {
    client.get_data(
        "SELECT id, name, description, created_at, valid_from, color, app_id, is_active
         FROM checkpoints
         ORDER BY id DESC",
    )
}

pub fn get_checkpoints_for_app(
    client: &PgClient,
    app_id: i32,
) -> Result<serde_json::Value, Box<dyn Error>> {
    client.get_data(&format!(
        "SELECT id, name, description, created_at, valid_from, color, app_id, is_active
         FROM checkpoints
         WHERE app_id = {}
         ORDER BY id DESC",
        app_id
    ))
}

pub fn get_active_checkpoints(client: &PgClient) -> Result<serde_json::Value, Box<dyn Error>> {
    client.get_data(
        "SELECT c.id, c.name, c.description, c.created_at, c.valid_from, c.color, c.app_id, c.is_active
         FROM checkpoints c
         JOIN active_checkpoints ac ON c.id = ac.checkpoint_id
         ORDER BY c.id DESC",
    )
}

pub fn get_active_checkpoints_for_app(
    client: &PgClient,
    app_id: i32,
) -> Result<serde_json::Value, Box<dyn Error>> {
    client.get_data(&format!(
        "SELECT c.id, c.name, c.description, c.created_at, c.valid_from, c.color, c.app_id, c.is_active
         FROM checkpoints c
         JOIN active_checkpoints ac ON c.id = ac.checkpoint_id
         WHERE c.app_id = {}
         ORDER BY c.id DESC",
        app_id
    ))
}

pub fn get_timeline_checkpoint_associations(
    client: &PgClient,
) -> Result<serde_json::Value, Box<dyn Error>> {
    client.get_data(
        "SELECT timeline_id, checkpoint_id
         FROM timeline_checkpoints
         ORDER BY timeline_id, checkpoint_id",
    )
}

pub fn get_all_timeline(client: &PgClient) -> Result<serde_json::Value, Box<dyn Error>> {
    client.get_data("SELECT * FROM timeline ORDER BY date DESC")
}

pub fn get_session_count_for_app(
    client: &PgClient,
    app_id: i32,
) -> Result<i32, Box<dyn Error>> {
    Ok(client.get_single_value::<i64>(&format!(
        "SELECT count(*) FROM timeline WHERE app_id = {}",
        app_id
    )).unwrap_or(0) as i32)
}

pub fn get_all_app_ids(client: &PgClient) -> Result<Vec<i32>, Box<dyn Error>> {
    client.get_data("SELECT id FROM apps ORDER BY id").map(|data| {
        data.as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|obj| {
                        obj.as_object()
                            .and_then(|map| map.get("id"))
                            .and_then(|id_val| id_val.as_str())
                            .and_then(|id_str| id_str.parse::<i32>().ok())
                    })
                    .collect()
            })
            .unwrap_or_else(|| vec![])
    })
}

// Timeline Checkpoint queries
pub fn create_timeline_checkpoint(
    client: &PgClient,
    timeline_id: i32,
    checkpoint_id: i32,
) -> Result<u64, Box<dyn Error>> {
    let id = match client
        .get_single_value::<i32>("(SELECT COALESCE(MAX(id), 0) + 1 FROM timeline_checkpoints)")
    {
        Some(id) => id,
        None => 1,
    };

    let connection = client
        .connection
        .lock()
        .map_err(|e| format!("Failed to acquire connection lock: {}", e))?;
    Ok(connection.execute(
        &format!(
            "INSERT INTO timeline_checkpoints (id, timeline_id, checkpoint_id, created_at)
             VALUES ({}, {}, {}, CURRENT_TIMESTAMP)",
            id, timeline_id, checkpoint_id
        ),
        &[],
    )?)
}

pub fn delete_timeline_checkpoint(
    client: &PgClient,
    timeline_id: i32,
    checkpoint_id: i32,
) -> Result<u64, Box<dyn Error>> {
    let connection = client
        .connection
        .lock()
        .map_err(|e| format!("Failed to acquire connection lock: {}", e))?;
    Ok(connection.execute(
        &format!(
            "DELETE FROM timeline_checkpoints
             WHERE timeline_id = {} AND checkpoint_id = {}",
            timeline_id, checkpoint_id
        ),
        &[],
    )?)
}

pub fn get_timeline_checkpoints_for_timeline(
    client: &PgClient,
    timeline_id: i32,
) -> Result<serde_json::Value, Box<dyn Error>> {
    client.get_data(&format!(
        "SELECT id, timeline_id, checkpoint_id, created_at
         FROM timeline_checkpoints
         WHERE timeline_id = {}
         ORDER BY created_at DESC",
        timeline_id
    ))
}

// Checkpoint Duration queries
pub fn get_checkpoint_durations(client: &PgClient) -> Result<serde_json::Value, Box<dyn Error>> {
    client.get_data(
        "SELECT id, checkpoint_id, app_id, duration, sessions_count, last_updated
         FROM checkpoint_durations
         ORDER BY last_updated DESC",
    )
}

pub fn get_checkpoint_durations_for_app(
    client: &PgClient,
    app_id: i32,
) -> Result<serde_json::Value, Box<dyn Error>> {
    client.get_data(&format!(
        "SELECT id, checkpoint_id, app_id, duration, sessions_count, last_updated
         FROM checkpoint_durations
         WHERE app_id = {}
         ORDER BY last_updated DESC",
        app_id
    ))
}

pub fn update_checkpoint_duration(
    client: &PgClient,
    checkpoint_id: i32,
    app_id: i32,
    duration: i32,
    sessions_count: i32,
) -> Result<u64, Box<dyn Error>> {
    let connection = client
        .connection
        .lock()
        .map_err(|e| format!("Failed to acquire connection lock: {}", e))?;

    // First try to update existing record
    let updated_rows = connection.execute(
        &format!(
            "UPDATE checkpoint_durations
             SET duration = {}, sessions_count = {}, last_updated = CURRENT_TIMESTAMP
             WHERE checkpoint_id = {} AND app_id = {}",
            duration, sessions_count, checkpoint_id, app_id
        ),
        &[],
    )?;

    // If no rows were updated, insert a new record
    if updated_rows == 0 {
        let id = match client
            .get_single_value::<i32>("(SELECT COALESCE(MAX(id), 0) + 1 FROM checkpoint_durations)")
        {
            Some(id) => id,
            None => 1,
        };

        connection.execute(
            &format!(
                "INSERT INTO checkpoint_durations (id, checkpoint_id, app_id, duration, sessions_count, last_updated)
                 VALUES ({}, {}, {}, {}, {}, CURRENT_TIMESTAMP)",
                id, checkpoint_id, app_id, duration, sessions_count
            ),
            &[],
        )?;
    }

    Ok(updated_rows)
}

// Active Checkpoint queries
pub fn get_all_active_checkpoints_table(
    client: &PgClient,
) -> Result<serde_json::Value, Box<dyn Error>> {
    client.get_data(
        "SELECT id, checkpoint_id, activated_at, app_id
         FROM active_checkpoints
         ORDER BY activated_at DESC",
    )
}

pub fn get_active_checkpoints_for_app_table(
    client: &PgClient,
    app_id: i32,
) -> Result<serde_json::Value, Box<dyn Error>> {
    client.get_data(&format!(
        "SELECT id, checkpoint_id, activated_at, app_id
         FROM active_checkpoints
         WHERE app_id = {}
         ORDER BY activated_at DESC",
        app_id
    ))
}

pub fn activate_checkpoint(
    client: &PgClient,
    checkpoint_id: i32,
    app_id: i32,
) -> Result<u64, Box<dyn Error>> {
    let id = match client
        .get_single_value::<i32>("(SELECT COALESCE(MAX(id), 0) + 1 FROM active_checkpoints)")
    {
        Some(id) => id,
        None => 1,
    };

    let connection = client
        .connection
        .lock()
        .map_err(|e| format!("Failed to acquire connection lock: {}", e))?;
    Ok(connection.execute(
        &format!(
            "INSERT INTO active_checkpoints (id, checkpoint_id, activated_at, app_id)
             VALUES ({}, {}, CURRENT_TIMESTAMP, {})",
            id, checkpoint_id, app_id
        ),
        &[],
    )?)
}

pub fn deactivate_checkpoint(
    client: &PgClient,
    checkpoint_id: i32,
    app_id: i32,
) -> Result<u64, Box<dyn Error>> {
    let connection = client
        .connection
        .lock()
        .map_err(|e| format!("Failed to acquire connection lock: {}", e))?;
    Ok(connection.execute(
        &format!(
            "DELETE FROM active_checkpoints
             WHERE checkpoint_id = {} AND app_id = {}",
            checkpoint_id, app_id
        ),
        &[],
    )?)
}

pub fn is_checkpoint_active(
    client: &PgClient,
    checkpoint_id: i32,
    app_id: i32,
) -> Result<bool, Box<dyn Error>> {
    let count = client
        .get_single_value::<i64>(&format!(
            "SELECT COUNT(*) FROM active_checkpoints
             WHERE checkpoint_id = {} AND app_id = {}",
            checkpoint_id, app_id
        ))
        .unwrap_or(0);
    Ok(count > 0)
}
