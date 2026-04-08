use std::{env, path::PathBuf, sync::Arc};

use dotenvy::dotenv;
use minecraft_stats::{
    database::DatabaseConnection,
    models::{Player, StatsFile},
    mojang_utils::UsernameCache,
};
use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use tokio::sync::mpsc;
use uuid::Uuid;

async fn handle_stats_file_change(
    db: &DatabaseConnection,
    path: &PathBuf,
    username_cache: &UsernameCache,
) {
    let file_stem = match path.file_stem().and_then(|s| s.to_str()) {
        Some(s) => s,
        None => {
            log::error!("Failed to get file stem for {:?}", path);
            return;
        }
    };

    let player_uuid = match Uuid::parse_str(file_stem) {
        Ok(uuid) => uuid,
        Err(e) => {
            log::error!("Invalid UUID in filename {}: {:?}", file_stem, e);
            return;
        }
    };

    log::info!("Processing stats file for player: {}", player_uuid);

    let stats_content = match tokio::fs::read_to_string(path).await {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to read stats file: {:?}", e);
            return;
        }
    };

    let player_stats: StatsFile = match serde_json::from_str(&stats_content) {
        Ok(s) => s,
        Err(e) => {
            log::error!("Failed to parse stats file: {:?}", e);
            return;
        }
    };

    let player_name = username_cache
        .uuid_to_username(&player_uuid)
        .unwrap_or_else(|| "Unknown".to_string());

    log::info!("Updating player: {} ({})", player_name, player_uuid);

    if let Err(e) = db
        .insert_player(Player {
            player_uuid,
            name: player_name.clone(),
        })
        .await
    {
        log::error!("Error inserting player {}: {:?}", player_name, e);
    }

    match db.insert_stats(player_uuid, player_stats).await {
        Ok(_) => log::info!(
            "Successfully synced stats for player: {} ({})",
            player_name,
            player_uuid
        ),
        Err(e) => log::error!("Error inserting stats for player {}: {:?}", player_name, e),
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
