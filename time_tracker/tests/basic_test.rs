//! Basic test to demonstrate the test environment setup.
//! This test shows how to use the TestEnvironment to create a test database
//! and perform basic operations.

#[cfg(test)]
mod tests {
    use time_tracker::{test_db::create_and_initialize_test_db, entities::apps, seaorm_client::SeaORMClient, restable::Restable};
    use sea_orm::{EntityTrait, ActiveValue::Set};

    #[test]
    fn test_basic_database_operations() {
        println!("Starting basic database operations test");

        let rt = tokio::runtime::Runtime::new().unwrap();
        let db = rt.block_on(create_and_initialize_test_db()).expect("Failed to create test database");
        let client = SeaORMClient::new_with_connection(db);
        println!("Created test database and client");

        // Check initial state of database
        let initial_apps = rt.block_on(apps::Entity::find().all(&*client.connection)).unwrap();
        println!("Initial database state - Number of apps: {}", initial_apps.len());

        let app = apps::ActiveModel {
            id: Set(1),
            name: Set(Some("Test App".to_string())),
            product_name: Set(Some("Test Product".to_string())),
            duration: Set(Some(0)),
            launches: Set(Some(0)),
            longest_session: Set(Some(0)),
            longest_session_on: Set(None),
        };

        let result = rt.block_on(apps::Entity::insert(app).exec(&*client.connection));
        assert!(result.is_ok());
        println!("Inserted test app into database");

        // Check database state after insertion
        let apps_after_insert = rt.block_on(apps::Entity::find().all(&*client.connection)).unwrap();
        println!("Database state after insertion - Number of apps: {}", apps_after_insert.len());
        if !apps_after_insert.is_empty() {
            println!("App details: {:?}", apps_after_insert[0]);
        }

        let apps_result = rt.block_on(apps::Entity::find().all(&*client.connection));
        assert!(apps_result.is_ok());

        let apps_vec = apps_result.unwrap();
        assert_eq!(apps_vec.len(), 1);
        assert_eq!(apps_vec[0].name.as_ref().unwrap(), "Test App");
        println!("Retrieved and verified test app from database");
    }

    #[test]
    fn test_client_get_all_apps() {
        println!("Starting get_all_apps test");

        let rt = tokio::runtime::Runtime::new().unwrap();
        let db = rt.block_on(create_and_initialize_test_db()).expect("Failed to create test database");
        let client = SeaORMClient::new_with_connection(db);
        println!("Created test database and client");

        // Check initial state of database
        let initial_apps = rt.block_on(apps::Entity::find().all(&*client.connection)).unwrap();
        println!("Initial database state - Number of apps: {}", initial_apps.len());

        let apps_result = rt.block_on(client.get_all_apps());
        assert!(apps_result.is_ok());

        let apps_value = apps_result.unwrap();
        assert!(apps_value.as_array().is_some());
        assert_eq!(apps_value.as_array().unwrap().len(), 0);
        println!("Verified get_all_apps returns empty array for empty database");
    }
}