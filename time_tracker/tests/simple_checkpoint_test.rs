//! Simple test to verify checkpoint duration and session count updates.
//! This test verifies that when checkpoints are active, their duration and session counts
//! are automatically updated when ReceiveTypes::Duration and ReceiveTypes::Launches events occur.

#[cfg(test)]
mod tests {
    use time_tracker::{
        test_db::create_and_initialize_test_db,
        test_client::TestClient,
        restable::Restable,
        receive_types::ReceiveTypes
    };
    use crossbeam_channel::unbounded;
    use std::{thread, time::Duration};

    #[test]
    fn test_checkpoint_auto_updates() {
        println!("Starting checkpoint auto-update test");

        let rt = tokio::runtime::Runtime::new().unwrap();
        let db = rt.block_on(create_and_initialize_test_db()).expect("Failed to create test database");
        let client = TestClient::new_with_connection(db);
        println!("Created test database and client");

        // Setup the database schema
        let setup_result = rt.block_on(client.setup());
        assert!(setup_result.is_ok());
        println!("Database schema setup completed");

        // Add a process to track
        let add_result = rt.block_on(client.put_data("test_app", "Test Application"));
        assert!(add_result.is_ok());
        println!("Added test_app to tracking");

        // Verify the app was added
        let apps_result = rt.block_on(client.get_all_apps());
        assert!(apps_result.is_ok());
        let apps_value = apps_result.unwrap();
        assert!(apps_value.as_array().is_some());
        assert_eq!(apps_value.as_array().unwrap().len(), 1);
        let app_id = apps_value.as_array().unwrap()[0]["id"].as_i64().unwrap() as i32;
        println!("Verified app was added to database with ID: {}", app_id);

        // Create a checkpoint
        println!("Creating checkpoint...");
        let checkpoint_result = rt.block_on(client.create_checkpoint("Test Checkpoint", Some("Checkpoint for testing"), app_id));
        assert!(checkpoint_result.is_ok());
        println!("Checkpoint created");

        // Get the checkpoint ID
        let checkpoints_result = rt.block_on(client.get_checkpoints_for_app(app_id));
        assert!(checkpoints_result.is_ok());
        let checkpoints_value = checkpoints_result.unwrap();
        assert!(checkpoints_value.as_array().is_some());
        assert_eq!(checkpoints_value.as_array().unwrap().len(), 1);
        let checkpoint_id = checkpoints_value.as_array().unwrap()[0]["id"].as_i64().unwrap() as i32;
        println!("Checkpoint ID: {}", checkpoint_id);

        // Activate the checkpoint
        let activate_result = rt.block_on(client.set_checkpoint_active(checkpoint_id, true));
        assert!(activate_result.is_ok());
        println!("Checkpoint activated");

        // Start the event loop in a separate thread
        let (tx, rx) = unbounded();
        let client_clone = client.clone();
        let _event_loop_thread = thread::spawn(move || {
            client_clone.client.init_event_loop(rx);
            // Keep the thread alive for a short time to process events
            thread::sleep(Duration::from_millis(100));
        });
        println!("Started event loop thread");

        // Send a Launches event
        tx.send(("test_app".to_string(), ReceiveTypes::Launches)).unwrap();
        println!("Sent Launches event");

        // Send Duration and Timeline events for 3 minutes
        for i in 1..=3 {
            tx.send(("test_app".to_string(), ReceiveTypes::Duration)).unwrap();
            tx.send(("test_app".to_string(), ReceiveTypes::Timeline)).unwrap();
            println!("Sent Duration and Timeline events for minute {}", i);
            thread::sleep(Duration::from_millis(10)); // Small delay between events
        }

        // Give the event loop time to process all events
        thread::sleep(Duration::from_millis(100));

        // Check that the checkpoint has been updated
        let checkpoint_durations_result = rt.block_on(client.get_checkpoint_durations_for_app(app_id));
        assert!(checkpoint_durations_result.is_ok());
        let checkpoint_durations_value = checkpoint_durations_result.unwrap();
        assert!(checkpoint_durations_value.as_array().is_some());
        let checkpoint_durations_array = checkpoint_durations_value.as_array().unwrap();
        println!("Number of checkpoint durations: {}", checkpoint_durations_array.len());

        // There should be one checkpoint duration entry for our checkpoint
        assert_eq!(checkpoint_durations_array.len(), 1);

        // Get the checkpoint duration
        let checkpoint_duration = &checkpoint_durations_array[0];
        let duration_value = checkpoint_duration["duration"].as_i64().unwrap() as i32;
        let sessions_count_value = checkpoint_duration["sessions_count"].as_i64().unwrap() as i32;
        println!("Checkpoint duration:");
        println!("  Duration: {}", duration_value);
        println!("  Sessions count: {}", sessions_count_value);

        // Verify checkpoint values make sense
        // The duration should be 3 minutes (the time we sent Duration events)
        assert_eq!(duration_value, 3);
        // The sessions count should be 1 (one session from the Launches event)
        assert_eq!(sessions_count_value, 1);

        println!("Checkpoint auto-update test completed successfully");
        println!("Verified that checkpoint has correct duration of {} minutes and {} sessions", duration_value, sessions_count_value);
    }
}