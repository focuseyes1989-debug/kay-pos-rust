-- POS-only mode. Preserve existing ERP records and suppress new queued events.
BEGIN;
SET LOCAL lock_timeout = '3s';
CREATE TABLE IF NOT EXISTS rust_erp_runtime (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    enabled BOOLEAN NOT NULL DEFAULT FALSE
);
INSERT INTO rust_erp_runtime(id, enabled) VALUES (1,FALSE)
ON CONFLICT(id) DO UPDATE SET enabled=FALSE;
CREATE OR REPLACE FUNCTION rust_erp_outbox_enabled_guard()
RETURNS TRIGGER AS $$
BEGIN
    IF NOT COALESCE((SELECT enabled FROM rust_erp_runtime WHERE id=1),FALSE) THEN
        RETURN NULL;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS trg_rust_erp_enabled ON rust_erp_outbox;
CREATE TRIGGER trg_rust_erp_enabled BEFORE INSERT ON rust_erp_outbox
FOR EACH ROW EXECUTE FUNCTION rust_erp_outbox_enabled_guard();
COMMIT;
