CREATE TABLE players (
    player_uuid UUID PRIMARY KEY,
    name TEXT NOT NULL
);

CREATE TABLE stat_categories (
    id SERIAL PRIMARY KEY,
    name TEXT UNIQUE NOT NULL
);

CREATE TABLE player_stats (
    player_uuid UUID REFERENCES players(player_uuid),
    stat_categories_id INT REFERENCES stat_categories(id),
    stat_name TEXT NOT NULL,
    value INTEGER NOT NULL,

    PRIMARY KEY (player_uuid, stat_categories_id, stat_name)
);