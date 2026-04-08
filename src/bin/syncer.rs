use std::{env, path::PathBuf, sync::Arc};

use dotenvy::dotenv;
use minecraft_stats::{database::DatabaseConnection, mojang_utils::UsernameCache};
use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use tokio::sync::mpsc;

async fn handle_stats_file_change(
    db: &DatabaseConnection,
    path: &PathBuf,
    username_cache: &UsernameCache,
) {
    if let Err(e) = db.process_stats_file(path, username_cache).await {
        log::error!("Error processing stats file {:?}: {:?}", path, e);
    } else {
        log::info!("Successfully synced stats for file: {:?}", path);
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    log::info!("Starting Minecraft Stats Sync");

    let _ = dotenv();
    let stats_env = env::var("WORLD_PATH").expect("WORLD_PATH must be set");
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    log::info!("World path: {}", stats_env);
    log::info!("Database URL: {}", database_url);

    let world_folder = PathBuf::from(&stats_env);
    let usercache_path = world_folder.join("usercache.json");
    let stats_folder = world_folder.join("stats");

    log::info!("Loading usercache from: {:?}", usercache_path);
    let username_cache = UsernameCache::from_usercache(&usercache_path)?;
    log::info!("Loaded {} players from usercache", username_cache.len());

    let database = Arc::new(DatabaseConnection::new(&database_url).await?);

    log::info!("Starting initial population of database from stats folder...");
    database.populate(&stats_folder, &username_cache).await?;
    log::info!("Initial database population complete");

    let db = database.clone();
    let stats_path = stats_folder.clone();
    let cache = username_cache.clone();

    let (tx, mut rx) = mpsc::channel(100);

    let mut debouncer = new_debouncer(
        std::time::Duration::from_millis(200),
        move |res: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
            if let Ok(events) = res {
                for event in events {
                    if event.kind == DebouncedEventKind::Any {
                        let _ = tx.blocking_send(event);
                    }
                }
            }
        },
    )?;

    debouncer
        .watcher()
        .watch(&stats_path, RecursiveMode::Recursive)?;

    log::info!("Watching for changes in {:?}", stats_path);

    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            let path = event.path;
            if path.extension().is_some_and(|ext| ext == "json") {
                log::info!("Detected change in: {:?}", path);
                handle_stats_file_change(&db, &path, &cache).await;
            }
        }
    });

    log::info!("Application ready and running");
    tokio::signal::ctrl_c().await?;
    log::info!("Shutting down...");

    Ok(())
}
