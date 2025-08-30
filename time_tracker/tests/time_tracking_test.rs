//! Test to simulate an actual app run for time tracking.

#[cfg(test)]
mod tests {
    use time_tracker::{
        test_db::create_and_initialize_test_db,
        seaorm_client::SeaORMClient,
        restable::Restable,
        receive_types::ReceiveTypes
    };
    use crossbeam_channel::unbounded;
    use std::{thread, time::Duration};

    #[test]
    fn test_time_tracking_simulation() {
        println!("Starting time tracking simulation test");

        let rt = tokio::runtime::Runtime::new().unwrap();
        let db = rt.block_on(create_and_initialize_test_db()).expect("Failed to create test database");
        let client = SeaORMClient::new_with_connection(db);
        println!("Created test database and client");

        // Setup the database schema
        let setup_result = rt.block_on(client.setup());
        assert!(setup_result.is_ok());
        println!("Database schema setup completed");

        // Add a process to track
        let add_result = rt.block_on(client.put_data("test_app", "Test Application"));
        match &add_result {
            Ok(_) => println!("Added test_app to tracking"),
            Err(e) => {
                println!("Failed to add test_app to tracking: {}", e);
                // Let's also try to get all apps to see if the table exists
                match rt.block_on(client.get_all_apps()) {
                    Ok(apps) => println!("Current apps in database: {:?}", apps),
                    Err(e) => println!("Failed to get apps: {}", e),
                }
            },
        }
        assert!(add_result.is_ok());
        println!("Added test_app to tracking");

        // Verify the app was added
        let apps_result = rt.block_on(client.get_all_apps());
        assert!(apps_result.is_ok());
        let apps_value = apps_result.unwrap();
        assert!(apps_value.as_array().is_some());
        assert_eq!(apps_value.as_array().unwrap().len(), 1);
        println!("Verified app was added to database");

        // Start the event loop in a separate thread
        let (tx, rx) = unbounded();
        let client_clone = client.clone();
        let _event_loop_thread = thread::spawn(move || {
            client_clone.init_event_loop(rx);
            // Keep the thread alive for a short time to process events
            thread::sleep(Duration::from_millis(100));
        });
        println!("Started event loop thread");

        // Simulate the process running for 3 minutes
        println!("Simulating 3 minutes of app usage...");

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

        // Send a LongestSession event
        tx.send(("test_app;3".to_string(), ReceiveTypes::LongestSession)).unwrap();
        println!("Sent LongestSession event");

        // Give the event loop time to process all events
        thread::sleep(Duration::from_millis(100));

        // Check the database state after simulation
        println!("Checking database state after simulation...");

        // Check app data
        let apps_after = rt.block_on(client.get_all_apps()).unwrap();
        println!("Number of apps in database: {}", apps_after.as_array().unwrap().len());
        if !apps_after.as_array().unwrap().is_empty() {
            let app = &apps_after.as_array().unwrap()[0];
            println!("App data:");
            println!("  ID: {}", app["id"]);
            println!("  Name: {:?}", app["name"]);
            println!("  Product Name: {:?}", app["product_name"]);
            println!("  Duration: {:?}", app["duration"]);
            println!("  Launches: {:?}", app["launches"]);
            println!(" Longest Session: {:?}", app["longest_session"]);
        }

        // Check timeline data
        let timeline_result = rt.block_on(client.get_all_timeline());
        if let Ok(timeline_value) = timeline_result {
            if let Some(timeline_array) = timeline_value.as_array() {
                println!("Number of timeline entries: {}", timeline_array.len());
                for (i, entry) in timeline_array.iter().enumerate() {
                    println!("  Timeline entry {}:", i);
                    println!("    Date: {:?}", entry["date"]);
                    println!("    Duration: {:?}", entry["duration"]);
                    println!("    App ID: {:?}", entry["app_id"]);
                }
            }
        }

        println!("Time tracking simulation test completed");
    }
}