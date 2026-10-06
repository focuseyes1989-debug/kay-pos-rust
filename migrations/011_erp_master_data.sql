-- ERP master-data identity mapping, pull cursor and import audit log.
BEGIN;

CREATE TABLE IF NOT EXISTS rust_erp_entity_map (
    provider TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    external_id TEXT NOT NULL,
    local_id INTEGER NOT NULL,
    source_version TEXT,
    source_hash TEXT NOT NULL,
    synced_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (provider, entity_type, external_id),
    UNIQUE (provider, entity_type, local_id)
);

CREATE TABLE IF NOT EXISTS rust_erp_sync_state (
    provider TEXT NOT NULL,
    stream TEXT NOT NULL,
    cursor_value TEXT,
    last_started_at TIMESTAMP,
    last_succeeded_at TIMESTAMP,
    last_error TEXT,
    PRIMARY KEY (provider, stream)
);

CREATE TABLE IF NOT EXISTS rust_erp_import_log (
    id BIGSERIAL PRIMARY KEY,
    provider TEXT NOT NULL,
    cursor_value TEXT,
    categories_count INTEGER NOT NULL DEFAULT 0,
    products_count INTEGER NOT NULL DEFAULT 0,
    customers_count INTEGER NOT NULL DEFAULT 0,
    suppliers_count INTEGER NOT NULL DEFAULT 0,
    locations_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

COMMIT;
