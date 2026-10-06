-- ERP stock snapshot comparisons. This migration never changes POS stock.
BEGIN;

CREATE TABLE IF NOT EXISTS rust_erp_reconciliation_runs (
    id BIGSERIAL PRIMARY KEY,
    provider TEXT NOT NULL,
    snapshot_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'running'
        CHECK (status IN ('running','complete','failed')),
    compared_count INTEGER NOT NULL DEFAULT 0,
    difference_count INTEGER NOT NULL DEFAULT 0,
    unmapped_count INTEGER NOT NULL DEFAULT 0,
    cursor_value TEXT,
    last_error TEXT,
    started_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at TIMESTAMP,
    UNIQUE(provider, snapshot_id)
);

CREATE TABLE IF NOT EXISTS rust_erp_stock_discrepancies (
    id BIGSERIAL PRIMARY KEY,
    run_id BIGINT NOT NULL REFERENCES rust_erp_reconciliation_runs(id) ON DELETE CASCADE,
    item_key TEXT NOT NULL,
    product_external_id TEXT NOT NULL,
    variant_external_id TEXT,
    location_external_id TEXT,
    local_product_id INTEGER,
    local_variant_id INTEGER,
    local_location_id INTEGER,
    erp_quantity DOUBLE PRECISION NOT NULL,
    pos_quantity DOUBLE PRECISION,
    difference DOUBLE PRECISION,
    status TEXT NOT NULL CHECK(status IN ('matched','difference','unmapped')),
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(run_id, item_key)
);

CREATE INDEX IF NOT EXISTS idx_rust_erp_stock_discrepancies_status
    ON rust_erp_stock_discrepancies(run_id, status);

COMMIT;
