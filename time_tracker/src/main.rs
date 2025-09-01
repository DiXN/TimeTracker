#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate rust_embed;

use std::error::Error;
use std::fs;
use std::env;
use std::process;

use env_logger::{Builder, Env};

use serde_derive::Deserialize;

mod error;
mod receive_types;
mod structs;
mod time_tracking;

#[cfg(feature = "firebase")]
mod firebase;
#[cfg(feature = "firebase")]
use crate::firebase::FirebaseClient;

mod restable;

mod hook;
mod rpc;

#[cfg(any(feature = "psql", feature = "sqlite"))]
mod seaorm_client;
#[cfg(any(feature = "psql", feature = "sqlite"))]
use crate::seaorm_client::SeaORMClient;

#[cfg(any(feature = "psql", feature = "sqlite"))]
mod entities;

#[cfg(any(feature = "psql", feature = "sqlite"))]
mod seaorm_queries;

#[cfg(any(feature = "psql", feature = "sqlite"))]
mod migration;

// Test modules - always available for testing
mod test_db;
mod test_client;

mod native;
use crate::native::{autostart, init_tray};

#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
mod linux;
mod web_socket;

#[derive(RustEmbed)]
#[folder = "resource/"]
pub struct Asset;

#[derive(Deserialize)]
struct Config {
    #[cfg(feature = "firebase")]
    firebase: Option<Firebase>,
    #[cfg(feature = "psql")]
    postgres: Option<Postgres>,
    // Time tracking delay settings
    #[serde(default)]
    time_tracking: time_tracking::TimeTrackingConfig,
    // When using SQLite feature without psql or firebase, we still need a valid struct
    #[cfg(all(feature = "sqlite", not(any(feature = "firebase", feature = "psql"))))]
    _dummy: Option<String>, // Placeholder to make the struct valid
}


#[cfg(feature = "firebase")]
#[derive(Deserialize)]
struct Firebase {
    url: String,
    key: String,
}

#[cfg(feature = "firebase")]
impl AsRef<Firebase> for Firebase {
    fn as_ref(&self) -> &Firebase {
        self
    }
}

#[cfg(feature = "psql")]
#[derive(Deserialize)]
struct Postgres {
    user: String,
    url: String,
    password: Option<String>,
    database: Option<String>,
}

#[cfg(feature = "psql")]
impl AsRef<Postgres> for Postgres {
    fn as_ref(&self) -> &Postgres {
        self
    }
}

#[cfg(feature = "firebase")]
fn init_client(config: Config) -> Result<(), Box<dyn Error>> {
    let url = config.firebase.as_ref().map(|f| &f.url);
    let key = config.firebase.as_ref().map(|f| &f.key);

    let firebase_client = match (url, key) {
        (Some(url), Some(key)) => FirebaseClient::new(url, key),
        _ => panic!("Missing credentials."),
    };

    Ok(time_tracking::init(firebase_client, config.time_tracking)?)
}

#[cfg(feature = "psql")]
fn init_client(config: Config) -> Result<(), Box<dyn Error>> {
    let url = config.postgres.as_ref().map(|p| &p.url);
    let user = config.postgres.as_ref().map(|p| &p.user);
    let password = config.postgres.as_ref().and_then(|p| p.password.as_ref());
    let database = config.postgres.as_ref().and_then(|p| p.database.as_ref());

    let credentials = match (user, url, password, database) {
        (Some(user), Some(url), None, None) => (format!("postgres://{}@{}:5432", user, url), None),
        (Some(user), Some(url), Some(password), None) => (
            format!("postgres://{}:{}@{}:5432", user, password, url),
            None,
        ),
        (Some(user), Some(url), None, Some(database)) => {
            (format!("postgres://{}@{}:5432", user, url), Some(database))
        }
        (Some(user), Some(url), Some(password), Some(database)) => (
            format!("postgres://{}:{}@{}:5432", user, password, url),
            Some(database),
        ),
        _ => panic!("Missing credentials."),
    };

    let db_url = if let Some(db) = credentials.1 {
        format!("{}/{}", credentials.0, db)
    } else {
        format!("{}/{}", credentials.0, "time_tracker")
    };

    // Create a runtime for async operations
    let rt = tokio::runtime::Runtime::new()?;

    let seaorm_client = rt.block_on(async {
        // Connect to the database
        let client = SeaORMClient::new(&db_url).await?;

        // Run migrations
        use sea_orm_migration::MigratorTrait;
        crate::migration::Migrator::up(&*client.connection, None).await?;

        Ok::<SeaORMClient, Box<dyn Error>>(client)
    })?;

    // Handle the async function in a blocking manner
    rt.block_on(async {
        time_tracking::init(seaorm_client, config.time_tracking).await
    })
}

#[cfg(feature = "sqlite")]
fn init_client(config: Config) -> Result<(), Box<dyn Error>> {
    // Create a runtime for async operations
    let rt = tokio::runtime::Runtime::new()?;

    let seaorm_client = rt.block_on(async {
        // Connect to an in-memory SQLite database
        let db = sea_orm::Database::connect("sqlite::memory:").await?;
        let db_connection = std::sync::Arc::new(db);

        // Create a SeaORMClient with the in-memory database connection
        let client = SeaORMClient::new_with_connection(db_connection.clone());

        // Run migrations
        use sea_orm_migration::MigratorTrait;
        crate::migration::Migrator::up(&*client.connection, None).await?;

        Ok::<SeaORMClient, Box<dyn Error>>(client)
    })?;

    // Handle the async function in a blocking manner
    rt.block_on(async {
        time_tracking::init(seaorm_client, time_tracking::TimeTrackingConfig::default()).await
    })
}

#[cfg(not(any(feature = "firebase", feature = "psql", feature = "sqlite")))]
fn init_client(config: Config) -> Result<(), Box<dyn Error>> {
    // Default implementation when no features are enabled
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let env = Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info");

    Builder::from_env(env).init();

    autostart()?;
    init_tray();

    // Check for --memory flag
    let args: Vec<String> = env::args().collect();
    let use_memory_db = args.contains(&"--memory".to_string());

    if use_memory_db {
        // For in-memory database, we don't need to read config file
        #[cfg(feature = "sqlite")]
        {
            let config = Config {
                #[cfg(feature = "firebase")]
                firebase: None,
                #[cfg(feature = "psql")]
                postgres: None,
                time_tracking: time_tracking::TimeTrackingConfig::default(),
                #[cfg(all(feature = "sqlite", not(any(feature = "firebase", feature = "psql"))))]
                _dummy: None,
            };
            init_client(config)?;
        }
        #[cfg(not(feature = "sqlite"))]
        {
            eprintln!("Error: SQLite feature not enabled. Please compile with --features sqlite");
            process::exit(1);
        }
    } else {
        // Read config from file as usual
        let config = fs::read_to_string("env.toml")?;
        let config: Config = toml::from_str(&config)?;
        init_client(config)?;
    }

    Ok(())
}
