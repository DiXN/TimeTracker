//! Simple test to debug checkpoint creation issues

#[cfg(test)]
mod tests {
    use time_tracker::{
        test_db::create_and_initialize_test_db,
        test_client::TestClient,
        restable::Restable,
    };

    #[test]
    fn test_checkpoint_creation_debug() {
        println!("Starting checkpoint creation debug test");

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

        // Verify the app was added and get its ID
        let apps_result = rt.block_on(client.get_all_apps());
        assert!(apps_result.is_ok());
        let apps_value = apps_result.unwrap();
        assert!(apps_value.as_array().is_some());
        assert_eq!(apps_value.as_array().unwrap().len(), 1);
        let app_id = apps_value.as_array().unwrap()[0]["id"].as_i64().unwrap() as i32;
        println!("Verified app was added to database with ID: {}", app_id);

        // Try to create a checkpoint
        println!("Attempting to create checkpoint...");
        let checkpoint_result = rt.block_on(client.create_checkpoint("Test Checkpoint", Some("Checkpoint for testing"), app_id));

        if let Err(ref e) = checkpoint_result {
            println!("Error creating checkpoint: {}", e);

            // Let's try to get more information about the database state
            println!("Current apps in database:");
            let apps_result = rt.block_on(client.get_all_apps());
            if let Ok(apps_value) = apps_result {
                println!("{:?}", apps_value);
            }

            // Let's also check if the checkpoints table exists
            println!("Trying to get all checkpoints...");
            let all_checkpoints_result = rt.block_on(client.get_all_checkpoints());
            if let Ok(checkpoints_value) = all_checkpoints_result {
                println!("All checkpoints: {:?}", checkpoints_value);
            } else {
                println!("Error getting all checkpoints: {:?}", all_checkpoints_result.err());
            }
        } else {
            println!("Checkpoint created successfully");
        }

        assert!(checkpoint_result.is_ok());
        println!("Checkpoint creation debug test completed");
    }
}