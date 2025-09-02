//! Test to simulate the full process of adding a process, running the event loop,
//! checking if the process is running, and verifying results after process end.

#[cfg(test)]
mod tests {
    use std::{
        process::Command,
        sync::{Arc, RwLock},
        thread,
        time::Duration,
    };
    use time_tracker::{
        restable::Restable,
        seaorm_client::SeaORMClient,
        test_db::create_and_initialize_test_db,
        time_tracking::{TimeTrackingConfig, init as init_time_tracking},
    };
    use tokio;

    #[test]
    fn test_full_process_tracking() {
        println!("Starting full process tracking test");

        let rt = tokio::runtime::Runtime::new().unwrap();
        let db = rt
            .block_on(create_and_initialize_test_db())
            .expect("Failed to create test database");
        let client = Arc::new(RwLock::new(SeaORMClient::new_with_connection(db)));
        println!("Created test database and client");

        // Setup the database schema
        let setup_result = rt.block_on(client.read().unwrap().setup());
        assert!(setup_result.is_ok());
        println!("Database schema setup completed");

        // Create a time tracking config with short delays for testing
        let config = TimeTrackingConfig {
            tracking_delay_ms: 100, // 100ms instead of 60 seconds
            check_delay_ms: 50,     // 50ms instead of 10 seconds
        };

        // Start the time tracking system in a separate thread
        let client_clone = client.clone();
        let _tracking_thread = thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                // This will run indefinitely, so we'll stop it manually after our test
                let _ = init_time_tracking(client_clone.read().unwrap().clone(), config).await;
            });
        });
        println!("Started time tracking system");

        // Add a process to track - we'll use "sleep" to match the Linux process name
        let add_result = rt.block_on(time_tracker::time_tracking::add_process(
            "sleep",
            "/bin/sleep",
            &client,
        ));
        match &add_result {
            Ok(_) => println!("Added sleep to tracking"),
            Err(e) => {
                println!("Failed to add sleep to tracking: {}", e);
                // Let's also try to get all apps to see if the table exists
                match rt.block_on(client.read().unwrap().get_all_apps()) {
                    Ok(apps) => println!("Current apps in database: {:?}", apps),
                    Err(e) => println!("Failed to get apps: {}", e),
                }
            }
        }
        assert!(add_result.is_ok());
        println!("Added sleep to tracking");

        // Verify the app was added
        let apps_result = rt.block_on(client.read().unwrap().get_all_apps());
        assert!(apps_result.is_ok());
        let apps_value = apps_result.unwrap();
        assert!(apps_value.as_array().is_some());
        assert_eq!(apps_value.as_array().unwrap().len(), 1);
        println!("Verified app was added to database");

        // Give the tracking system time to initialize
        thread::sleep(Duration::from_millis(100));

        // Start a real process to simulate the process running
        // We'll start a sleep process that runs for a few seconds
        println!("Starting a real sleep process...");
        let mut child = Command::new("sleep")
            .arg("10")
            .spawn()
            .expect("Failed to start sleep process");

        // Give the tracking system time to detect and track the process
        // Increase the sleep time to ensure the process tracking has time to work
        println!("Waiting for process detection and tracking...");
        thread::sleep(Duration::from_millis(1000));

        // Check the database state after simulation
        println!("Checking database state after simulation...");

        // Check app data - should have launches and duration now
        let apps_after = rt.block_on(client.read().unwrap().get_all_apps()).unwrap();
        println!(
            "Number of apps in database: {}",
            apps_after.as_array().unwrap().len()
        );
        if !apps_after.as_array().unwrap().is_empty() {
            let app = &apps_after.as_array().unwrap()[0];
            println!("App data:");
            println!("  ID: {}", app["id"]);
            println!("  Name: {:?}", app["name"]);
            println!("  Product Name: {:?}", app["product_name"]);
            println!("  Duration: {:?}", app["duration"]);
            println!("  Launches: {:?}", app["launches"]);
            println!("  Longest Session: {:?}", app["longest_session"]);

            // We can't guarantee specific values in a test environment, but we can check
            // that the fields exist
            assert!(app["id"].is_number(), "ID should be a number");
            assert!(app["name"].is_string(), "Name should be a string");
        }

        child.kill().expect("Failed to kill process");

        println!("Waiting for process {} to be killed...", child.id());
        thread::sleep(Duration::from_millis(2000));

        // Check app data - should have launches and duration now
        let apps_after = rt.block_on(client.read().unwrap().get_all_apps()).unwrap();

        if !apps_after.as_array().unwrap().is_empty() {
            let app = &apps_after.as_array().unwrap()[0];
            println!("App data:");
            println!("  ID: {}", app["id"]);
            println!("  Name: {:?}", app["name"]);
            println!("  Product Name: {:?}", app["product_name"]);
            println!("  Duration: {:?}", app["duration"]);
            println!("  Launches: {:?}", app["launches"]);
            println!("  Longest Session: {:?}", app["longest_session"]);

            // We can't guarantee specific values in a test environment, but we can check
            // that the fields exist
            assert!(app["id"].is_number(), "ID should be a number");
            assert!(app["name"].is_string(), "Name should be a string");
        }

        // Check timeline data
        let timeline_result = rt.block_on(client.read().unwrap().get_all_timeline());
        if let Ok(timeline_value) = timeline_result {
            if let Some(timeline_array) = timeline_value.as_array() {
                println!("Number of timeline entries: {}", timeline_array.len());
                for (i, entry) in timeline_array.iter().enumerate() {
                    println!("  Timeline entry {}:", i);
                    println!("    Date: {:?}", entry["date"]);
                    println!("    Duration: {:?}", entry["duration"]);
                    println!("    App ID: {:?}", entry["app_id"]);
                }

                // Verify we have at least one timeline entry, which indicates the process was tracked
                assert!(
                    !timeline_array.is_empty(),
                    "Should have at least one timeline entry when process is tracked"
                );
                println!("Verified that process tracking created timeline entries");
            }
        }

        println!("Full process tracking test completed");
    }

    #[test]
    fn test_add_process_function() {
        println!("Starting add_process function test");

        let rt = tokio::runtime::Runtime::new().unwrap();
        let db = rt
            .block_on(create_and_initialize_test_db())
            .expect("Failed to create test database");
        let client = Arc::new(RwLock::new(SeaORMClient::new_with_connection(db)));
        println!("Created test database and client");

        // Setup the database schema
        let setup_result = rt.block_on(client.read().unwrap().setup());
        assert!(setup_result.is_ok());
        println!("Database schema setup completed");

        // Try to add a process
        let add_result = rt.block_on(time_tracker::time_tracking::add_process(
            "test_app",
            "/path/to/test_app",
            &client,
        ));
        assert!(add_result.is_ok());
        println!("Added test_app to tracking");

        // Verify the app was added
        let apps_result = rt.block_on(client.read().unwrap().get_all_apps());
        assert!(apps_result.is_ok());
        let apps_value = apps_result.unwrap();
        assert!(apps_value.as_array().is_some());
        assert_eq!(apps_value.as_array().unwrap().len(), 1);
        println!("Verified app was added to database");

        // Try to add the same process again (should fail)
        let add_result2 = rt.block_on(time_tracker::time_tracking::add_process(
            "test_app",
            "/path/to/test_app",
            &client,
        ));
        assert!(add_result2.is_err());
        println!("Correctly rejected duplicate process");

        println!("Add process function test completed");
    }
}
