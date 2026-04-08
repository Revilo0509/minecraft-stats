use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Queryable, Selectable, Insertable, Debug, Serialize, Deserialize)]
#[diesel(table_name = crate::schema::players)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Player {
    pub player_uuid: Uuid,
    pub name: String,
}

#[derive(Queryable, Selectable, Insertable, Debug, Serialize, Deserialize)]
#[diesel(table_name = crate::schema::player_stats)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct PlayerStats {
    pub player_uuid: Uuid,
    pub stat_categories_id: i32,
    pub stat_name: String,
    pub value: i32,
}

#[derive(Queryable, Selectable, Insertable, Debug, Serialize, Deserialize)]
#[diesel(table_name = crate::schema::stat_categories)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct StatCategorie {
    pub id: i32,
    pub name: String,
}

#[derive(Deserialize, Debug)]
pub struct StatsFile {
    pub stats: HashMap<String, HashMap<String, i32>>,
}
