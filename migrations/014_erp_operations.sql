-- Worker health and operator controls for ERP operations.
BEGIN;

CREATE TABLE IF NOT EXISTS rust_erp_worker_health (
    worker_name TEXT PRIMARY KEY,
    status TEXT NOT NULL CHECK(status IN ('healthy','error')),
    last_started_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_finished_at TIMESTAMP,
    last_success_at TIMESTAMP,
    last_error TEXT,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_rust_erp_outbox_failed
    ON rust_erp_outbox(status, id DESC)
    WHERE status IN ('pending','dead');

COMMIT;
