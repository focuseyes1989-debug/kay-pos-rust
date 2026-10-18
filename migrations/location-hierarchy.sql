CREATE TABLE IF NOT EXISTS rust_location_hierarchy (
    location_id INTEGER PRIMARY KEY,
    parent_id INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS rust_location_hierarchy_parent ON rust_location_hierarchy(parent_id);
