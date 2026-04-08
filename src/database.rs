use anyhow::{anyhow, Result};
use diesel::{ExpressionMethods, QueryDsl, SelectableHelper};
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
use log::debug;
use std::path::Path;
use tokio::fs;
use uuid::Uuid;

use crate::models::{Player, PlayerStats, StatsFile};
use crate::mojang_utils::MojangCache;

pub async fn establish_connection(url: &str) -> AsyncPgConnection {
    AsyncPgConnection::establish(url)
        .await
        .unwrap_or_else(|_| panic!("Error connecting to {}", url))
}

async fn insert_player(database: &mut AsyncPgConnection, player: Player) -> Result<Player> {
    use crate::schema::players;
    debug!("Inserted player: {:?}", player);

    diesel::insert_into(players::table)
        .values(&player)
        .returning(Player::as_returning())
        .get_result(database)
        .await
        .map_err(|e| anyhow!(e))
}

async fn get_or_insert_category(
    database: &mut AsyncPgConnection,
    category_name: &str,
) -> Result<i32> {
    use crate::schema::stat_categories::dsl::*;
    use crate::schema::stat_categories::columns;

    let result: Result<i32, _> = stat_categories
        .filter(columns::name.eq(category_name))
        .select(columns::id)
        .get_result(database)
        .await;

    if let Ok(existing_id) = result {
        return Ok(existing_id);
    }

    let new_id: i32 = diesel::insert_into(stat_categories)
        .values(columns::name.eq(category_name))
        .returning(columns::id)
        .get_result(database)
        .await
        .map_err(|e| anyhow!(e))?;

    debug!("Inserted stat category: {} with id {}", category_name, new_id);
    Ok(new_id)
}

async fn insert_stats(
    database: &mut AsyncPgConnection,
    player_uuid: Uuid,
    stats: StatsFile,
) -> Result<()> {
    use crate::schema::player_stats;

    for (category_name, stat_map) in stats.stats {
        let stat_categories_id = get_or_insert_category(database, &category_name).await?;

        for (stat_name, value) in stat_map {
            let player_stat = PlayerStats {
                player_uuid,
                stat_categories_id,
                stat_name,
                value,
            };

            diesel::insert_into(player_stats::table)
                .values(&player_stat)
                .on_conflict_do_nothing()
                .execute(database)
                .await
                .map_err(|e| anyhow!(e))?;
        }
    }

    Ok(())
}

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

            insert_player(
                database,
                Player {
                    player_uuid,
                    name: player_name,
                },
            )
            .await?;

            insert_stats(database, player_uuid, player_stats).await?;
        }
    }

    Ok(())
}
