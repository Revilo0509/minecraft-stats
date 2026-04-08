use anyhow::{anyhow, Result};
use diesel::{ExpressionMethods, QueryDsl, SelectableHelper};
use diesel_async::pooled_connection::bb8::{Pool, PooledConnection};
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use log::{debug, info};
use std::path::Path;
use tokio::fs;
use uuid::Uuid;

use crate::models::{Player, PlayerStats, StatsFile};
use crate::mojang_utils::UsernameCache;

pub struct DatabaseConnection {
    pool: Pool<AsyncPgConnection>,
}

impl DatabaseConnection {
    pub async fn new(url: &str) -> Result<Self> {
        info!("Establishing database connection...");
        let config = AsyncDieselConnectionManager::<AsyncPgConnection>::new(url);
        let pool = Pool::builder().build(config).await?;
        Ok(Self { pool })
    }

    pub async fn get(&self) -> Result<PooledConnection<'_, AsyncPgConnection>> {
        self.pool.get().await.map_err(|e| anyhow!(e))
    }

    pub async fn insert_player(&self, player: Player) -> Result<Player> {
        use crate::schema::players::dsl::*;
        debug!("Inserting player: {:?}", player);

        let mut database_connection = self.get().await?;
        let result = diesel::insert_into(players)
            .values(&player)
            .on_conflict(player_uuid)
            .do_update()
            .set(name.eq(&player.name))
            .returning(Player::as_returning())
            .get_result(&mut database_connection)
            .await
            .map_err(|e| anyhow!(e))?;

        Ok(result)
    }

    pub async fn insert_stats(&self, uuid: Uuid, stats: StatsFile) -> Result<()> {
        use crate::schema::player_stats::columns;
        use crate::schema::player_stats::dsl::*;

        let mut database_connection = self.get().await?;

        let stat_count = stats.stats.values().map(|m| m.len()).sum::<usize>();
        info!("Inserting {} stats for player {}", stat_count, uuid);

        for (category_name, stat_map) in stats.stats {
            let category_id = self
                .insert_category(&mut database_connection, &category_name)
                .await?;

            for (stat_nm, val) in stat_map {
                let player_stat = PlayerStats {
                    player_uuid: uuid,
                    stat_categories_id: category_id,
                    stat_name: stat_nm,
                    value: val,
                };

                diesel::insert_into(player_stats)
                    .values(&player_stat)
                    .on_conflict((
                        columns::player_uuid,
                        columns::stat_categories_id,
                        columns::stat_name,
                    ))
                    .do_update()
                    .set(columns::value.eq(val))
                    .execute(&mut database_connection)
                    .await
                    .map_err(|e| anyhow!(e))?;
            }
        }

        info!(
            "Successfully inserted/updated {} stats for player {}",
            stat_count, uuid
        );
        Ok(())
    }

    pub async fn insert_category(
        &self,
        database: &mut AsyncPgConnection,
        category_name: &str,
    ) -> Result<i32> {
        use crate::schema::stat_categories::columns;
        use crate::schema::stat_categories::dsl::*;

        if let Ok(existing_id) = stat_categories
            .filter(columns::name.eq(category_name))
            .select(columns::id)
            .get_result::<i32>(database)
            .await
        {
            return Ok(existing_id);
        }

        let new_id: i32 = diesel::insert_into(stat_categories)
            .values(columns::name.eq(category_name))
            .on_conflict(columns::name)
            .do_update()
            .set(columns::name.eq(category_name))
            .returning(columns::id)
            .get_result(database)
            .await
            .map_err(|e| anyhow!(e))?;

        debug!(
            "Inserted stat category: {} with id {}",
            category_name, new_id
        );
        Ok(new_id)
    }

    pub async fn populate(
        &self,
        stats_folder: &Path,
        username_cache: &UsernameCache,
    ) -> Result<()> {
        let mut dir_entries = fs::read_dir(stats_folder).await?;
        let mut tasks = FuturesUnordered::new();

        while let Some(entry) = dir_entries.next_entry().await? {
            let path = entry.path();

            if path.extension().map_or(false, |ext| ext == "json") {
                tasks.push(async move {
                    let file_stem = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .ok_or_else(|| anyhow!("Failed to get file stem"))?;

                    let player_uuid = Uuid::parse_str(file_stem)?;

                    let stats_content = fs::read_to_string(&path).await?;
                    let player_stats: StatsFile = serde_json::from_str(&stats_content)?;

                    let player_name = username_cache
                        .uuid_to_username(&player_uuid)
                        .unwrap_or_else(|| "Unknown".to_string());

                    self.insert_player(Player {
                        player_uuid,
                        name: player_name,
                    })
                    .await?;

                    self.insert_stats(player_uuid, player_stats).await?;

                    Ok::<_, anyhow::Error>(())
                });
            }
        }

        while let Some(result) = tasks.next().await {
            result?;
        }

        Ok(())
    }
}
