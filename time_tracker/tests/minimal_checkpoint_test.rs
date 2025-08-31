//! Minimal test to isolate checkpoint creation issue

#[cfg(test)]
mod tests {
    use time_tracker::{
        test_db::create_and_initialize_test_db,
        seaorm_client::SeaORMClient,
        seaorm_queries::create_checkpoint,
        seaorm_queries::set_checkpoint_active,
        seaorm_queries::get_all_checkpoints,
        seaorm_queries::get_active_checkpoints,
        entities::apps,
        migration::Migrator,
    };
    use sea_orm::{ActiveModelTrait, ActiveValue, EntityTrait};
    use sea_orm_migration::MigratorTrait;

    #[test]
    fn test_minimal_checkpoint_creation() {
        println!("Starting minimal checkpoint creation test");

        let rt = tokio::runtime::Runtime::new().unwrap();
        let db = rt.block_on(create_and_initialize_test_db()).expect("Failed to create test database");
        let client = SeaORMClient::new_with_connection(db);
        println!("Created test database and client");

        // Setup the database schema
        let setup_result = rt.block_on(Migrator::up(&*client.connection, None));
        assert!(setup_result.is_ok());
        println!("Database schema setup completed");

        // First create an app
        let app = apps::ActiveModel {
            id: ActiveValue::Set(1),
            name: ActiveValue::Set(Some("test_app".to_string())),
            product_name: ActiveValue::Set(Some("Test Application".to_string())),
            duration: ActiveValue::Set(Some(0)),
            launches: ActiveValue::Set(Some(0)),
            longest_session: ActiveValue::Set(Some(0)),
            longest_session_on: ActiveValue::Set(None),
        };

        let app_insert_result = rt.block_on(apps::Entity::insert(app).exec(&*client.connection));
        assert!(app_insert_result.is_ok());
        println!("App created successfully");

        // Try to create a checkpoint directly using the create_checkpoint function
        println!("Creating checkpoint using create_checkpoint function...");
        let checkpoint_result = rt.block_on(create_checkpoint(&client, "Test Checkpoint", Some("Checkpoint for testing"), 1));

        match &checkpoint_result {
            Ok(value) => {
                println!("Checkpoint created successfully: {:?}", value);
            }
            Err(e) => {
                println!("Error creating checkpoint: {}", e);
                // Let's also try to get more information about the apps to see if the app_id is valid
                let apps_result = rt.block_on(apps::Entity::find().all(&*client.connection));
                if let Ok(apps) = apps_result {
                    println!("Current apps in database: {:?}", apps);
                }

                // Let's also try to get all checkpoints to see if any were created
                let all_checkpoints_result = rt.block_on(get_all_checkpoints(&client));
                if let Ok(checkpoints_value) = all_checkpoints_result {
                    println!("All checkpoints: {:?}", checkpoints_value);
                }
            }
        }

        assert!(checkpoint_result.is_ok());
        println!("Checkpoint created");

        // Get the checkpoint ID
        let checkpoints_result = rt.block_on(get_all_checkpoints(&client));
        assert!(checkpoints_result.is_ok());
        let checkpoints_value = checkpoints_result.unwrap();
        assert!(checkpoints_value.as_array().is_some());
        assert_eq!(checkpoints_value.as_array().unwrap().len(), 1);
        let checkpoint_id = checkpoints_value.as_array().unwrap()[0]["id"].as_i64().unwrap() as i32;
        println!("Checkpoint ID: {}", checkpoint_id);

        // Try to activate the checkpoint directly using the activate_checkpoint function
        println!("Activating checkpoint using activate_checkpoint function...");
        let activate_result = rt.block_on(set_checkpoint_active(&client, checkpoint_id, true));

        match &activate_result {
            Ok(value) => {
                println!("Checkpoint activated successfully: {:?}", value);
            }
            Err(e) => {
                println!("Error activating checkpoint: {}", e);
                // Let's also try to get more information about the checkpoints to see if the checkpoint_id is valid
                let checkpoints_result = rt.block_on(get_all_checkpoints(&client));
                if let Ok(checkpoints_value) = checkpoints_result {
                    println!("Current checkpoints in database: {:?}", checkpoints_value);
                }

                // Let's also try to get all active checkpoints to see if any were activated
                let all_active_checkpoints_result = rt.block_on(get_active_checkpoints(&client));
                if let Ok(active_checkpoints_value) = all_active_checkpoints_result {
                    println!("All active checkpoints: {:?}", active_checkpoints_value);
                }
            }
        }

        assert!(activate_result.is_ok());
        println!("Checkpoint activated");
        println!("Minimal checkpoint creation test completed");
    }
}