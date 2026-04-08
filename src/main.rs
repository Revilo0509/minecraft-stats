use std::{env, path::PathBuf};

use dotenvy::dotenv;

mod database;
mod models;
mod mojang_utils;
mod schema;

use crate::database::{establish_connection, populate_database};
use crate::mojang_utils::MojangCache;

#[tokio::main]
async fn main() {
    env_logger::init();

    dotenv().expect("No .env file configured");
    let stats_env = env::var("WORLD_PATH").expect("WORLD_PATH must be set");
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let world_folder = PathBuf::from(&stats_env);
    let usercache_path = world_folder.join("usercache.json");
    let stats_folder = world_folder.join("stats");

    let mojang_cache =
        MojangCache::from_usercache(&usercache_path).expect("Failed to load usercache.json");

    let mut database_connection = establish_connection(&database_url).await;

    let _ = populate_database(&mut database_connection, &stats_folder, &mojang_cache).await;
}
