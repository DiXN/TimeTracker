//! Test to directly create a checkpoint using SeaORM

#[cfg(test)]
mod tests {
    use time_tracker::{
        test_db::create_and_initialize_test_db,
        entities::checkpoints,
        migration::Migrator,
    };
    use sea_orm::{ActiveModelTrait, ActiveValue, EntityTrait};
    use chrono::Utc;
    use sea_orm_migration::MigratorTrait;

    #[test]
    fn test_direct_checkpoint_creation() {
        println!("Starting direct checkpoint creation test");

        let rt = tokio::runtime::Runtime::new().unwrap();
        let db = rt.block_on(create_and_initialize_test_db()).expect("Failed to create test database");
        println!("Created test database");

        // Setup the database schema
        let setup_result = rt.block_on(Migrator::up(&*db, None));
        assert!(setup_result.is_ok());
        println!("Database schema setup completed");

        // First create an app
        use time_tracker::entities::apps;
        let app = apps::ActiveModel {
            id: ActiveValue::Set(1),
            name: ActiveValue::Set(Some("test_app".to_string())),
            product_name: ActiveValue::Set(Some("Test Application".to_string())),
            duration: ActiveValue::Set(Some(0)),
            launches: ActiveValue::Set(Some(0)),
            longest_session: ActiveValue::Set(Some(0)),
            longest_session_on: ActiveValue::Set(None),
        };

        let app_insert_result = rt.block_on(apps::Entity::insert(app).exec(&*db));
        assert!(app_insert_result.is_ok());
        println!("App created successfully");

        // Create a checkpoint directly
        println!("Creating checkpoint directly...");
        let now = Utc::now().naive_utc();
        let checkpoint = checkpoints::ActiveModel {
            id: ActiveValue::Set(1),
            name: ActiveValue::Set("Test Checkpoint".to_string()),
            description: ActiveValue::Set(Some("Checkpoint for testing".to_string())),
            created_at: ActiveValue::Set(Some(now)),
            valid_from: ActiveValue::Set(Some(now)),
            color: ActiveValue::Set(Some("#2196F3".to_string())),
            app_id: ActiveValue::Set(1), // Now this app exists
            is_active: ActiveValue::Set(Some(false)),
            duration: ActiveValue::Set(None),
            sessions_count: ActiveValue::Set(None),
            last_updated: ActiveValue::Set(Some(now)),
            activated_at: ActiveValue::Set(None),
        };

        let insert_result = rt.block_on(checkpoints::Entity::insert(checkpoint).exec(&*db));
        if let Err(ref e) = insert_result {
            println!("Error creating checkpoint directly: {}", e);
        } else {
            println!("Checkpoint created directly successfully");
        }

        assert!(insert_result.is_ok());
        println!("Direct checkpoint creation test completed");
    }
}