use anyhow::{anyhow, Result};
use diesel::SelectableHelper;
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
use log::debug;
use std::path::Path;
use tokio::fs;
use uuid::Uuid;

use crate::models::{NewPlayer, Player};
use crate::mojang_utils::MojangCache;
use crate::stat_file::StatsFile;

pub async fn establish_connection(url: &str) -> AsyncPgConnection {
    AsyncPgConnection::establish(url)
        .await
        .unwrap_or_else(|_| panic!("Error connecting to {}", url))
}

async fn insert_player(database: &mut AsyncPgConnection, player: NewPlayer) -> Result<Player> {
    use crate::schema::players;
    debug!("Inserted player: {:?}", player);

    diesel::insert_into(players::table)
        .values(&player)
        .returning(Player::as_returning())
        .get_result(database)
        .await
        .map_err(|e| anyhow!(e))
}

async fn insert_stats(database: &mut AsyncPgConnection, )

// TODO: Implement a connection pool and make this parrarel
pub async fn populate_database(
    database: &mut AsyncPgConnection,
    stats_folder: &Path,
    mojang_cache: &MojangCache,
) -> Result<()> {
    debug!("Using stats folder: {:?}", stats_folder);

    let mut dir_entries = fs::read_dir(stats_folder).await?;

    while let Some(entry) = dir_entries.next_entry().await? {
        let path = entry.path();

        if path.extension().map_or(false, |ext| ext == "json") {
            let file_stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| anyhow!("Failed to get file stem for {:?}", path))?;

            let player_uuid = Uuid::parse_str(file_stem)
                .map_err(|e| anyhow!("Invalid UUID in filename {}: {:?}", file_stem, e))?;

            let stats_content = fs::read_to_string(&path).await?;
            let player_stats: StatsFile = serde_json::from_str(&stats_content)?;

            let player_name = mojang_cache
                .uuid_to_username(&player_uuid)
                .unwrap_or_else(|| "Unknown".to_string());

            if let Err(e) = insert_player(
                database,
                NewPlayer {
                    player_uuid,
                    name: player_name,
                },
            )
            .await
            {
                log::error!("Error inserting player: {:?}", e);
            }
        }
    }

    Ok(())
}
