use axum::extract::State;
use axum::{extract::Path, extract::Query, routing::get, Json, Router};
use axum::{http::StatusCode, response::IntoResponse};
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use diesel_async::RunQueryDsl;
use dotenvy::dotenv;
use minecraft_stats::models::{Player, PlayerStats, StatCategorie};
use minecraft_stats::mojang_utils::UsernameCache;
use minecraft_stats::schema::players;
use minecraft_stats::{
    database::DatabaseConnection,
    schema::{player_stats, stat_categories},
};
use serde::Deserialize;
use std::env;
use std::path::PathBuf;
use uuid::Uuid;

const DEFAULT_LIMIT: i64 = 25;

#[derive(Debug, Deserialize)]
struct SearchParams {
    limit: Option<i64>,
    page: Option<i64>,
    sort_by: Option<String>,
    order: Option<String>,
}

#[derive(Clone)]
struct AppState {
    pub database_connection: DatabaseConnection,
    pub username_cache: UsernameCache,
}

async fn categories(State(app_state): State<AppState>) -> impl IntoResponse {
    match app_state.database_connection.get().await {
        Ok(mut conn) => {
            let categories: Vec<StatCategorie> = match stat_categories::table.load(&mut conn).await
            {
                Ok(data) => data,
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            };
            Json(categories).into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn categorie(
    State(app_state): State<AppState>,
    Path(categorie): Path<String>,
    Query(params): Query<SearchParams>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(DEFAULT_LIMIT);
    let offset = (params.page.unwrap_or(1).max(1) - 1) * limit;
    let (sort_by, order) = (
        params.sort_by.as_deref().unwrap_or("value"),
        params.order.as_deref().unwrap_or("desc"),
    );

    match app_state.database_connection.get().await {
        Ok(mut conn) => {
            let categories: Vec<StatCategorie> = match stat_categories::table
                .filter(stat_categories::name.eq(&categorie))
                .limit(1)
                .load(&mut conn)
                .await
            {
                Ok(data) => data,
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            };

            let category = match categories.into_iter().next() {
                Some(c) => c,
                None => return StatusCode::NOT_FOUND.into_response(),
            };

            let query = player_stats::table
                .filter(player_stats::stat_categories_id.eq(category.id))
                .into_boxed();
            let query = if sort_by == "stat_name" {
                if order == "asc" {
                    query.order(player_stats::stat_name.asc())
                } else {
                    query.order(player_stats::stat_name.desc())
                }
            } else if order == "asc" {
                query.order(player_stats::value.asc())
            } else {
                query.order(player_stats::value.desc())
            };
            let stats: Vec<PlayerStats> =
                match query.limit(limit).offset(offset).load(&mut conn).await {
                    Ok(data) => data,
                    Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
                };

            Json(serde_json::json!({ "category": category, "stats": stats })).into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn players(State(app_state): State<AppState>) -> impl IntoResponse {
    match app_state.database_connection.get().await {
        Ok(mut conn) => {
            let players: Vec<Player> = match players::table.load(&mut conn).await {
                Ok(data) => data,
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            };
            Json(players).into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn player(
    State(app_state): State<AppState>,
    Path(player_uuid): Path<Uuid>,
    Query(params): Query<SearchParams>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(DEFAULT_LIMIT);
    let offset = (params.page.unwrap_or(1).max(1) - 1) * limit;

    match app_state.database_connection.get().await {
        Ok(mut conn) => {
            let stats: Vec<PlayerStats> = match player_stats::table
                .filter(player_stats::player_uuid.eq(player_uuid))
                .limit(limit)
                .offset(offset)
                .load(&mut conn)
                .await
            {
                Ok(data) => data,
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            };

            Json(serde_json::json!({ "player_uuid": player_uuid, "stats": stats })).into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn uuid_to_username(
    State(app_state): State<AppState>,
    Path(uuid): Path<Uuid>,
) -> impl IntoResponse {
    app_state.username_cache.uuid_to_username(&uuid).unwrap()
}

async fn username_to_uuid(
    State(app_state): State<AppState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    app_state
        .username_cache
        .username_to_uuid(&name)
        .unwrap()
        .to_string()
}

#[tokio::main]
async fn main() {
    let _ = dotenv();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let database_connection = DatabaseConnection::new(&database_url)
        .await
        .expect("Could not connect to database");

    let mut world_folder = PathBuf::from(env::var("WORLD_PATH").expect("WORLD_PATH must be set"));
    world_folder.push("usercache.json");
    let username_cache = UsernameCache::from_usercache(&world_folder).unwrap();

    let state = AppState {
        database_connection,
        username_cache,
    };

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/categories", get(categories))
        .route("/categories/{categorie}", get(categorie))
        .route("/players", get(players))
        .route("/players/{player}", get(player))
        .route("/transform/uuid/{uuid}", get(uuid_to_username))
        .route("/transform/name/{name}", get(username_to_uuid))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
