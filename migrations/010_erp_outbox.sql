-- Reliable ERP integration outbox. Apply as the schema owner before enabling ERP sync.
BEGIN;

CREATE TABLE IF NOT EXISTS rust_erp_outbox (
    id BIGSERIAL PRIMARY KEY,
    event_key TEXT NOT NULL UNIQUE,
    event_type TEXT NOT NULL,
    aggregate_type TEXT NOT NULL,
    aggregate_id TEXT NOT NULL,
    payload JSONB NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'processing', 'sent', 'dead')),
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    locked_until TIMESTAMP,
    last_error TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    sent_at TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_rust_erp_outbox_ready
    ON rust_erp_outbox (next_attempt_at, id)
    WHERE status IN ('pending', 'processing');

-- Every inventory mutation becomes an ERP event in the same transaction as the stock change.
CREATE OR REPLACE FUNCTION rust_enqueue_stock_movement_for_erp()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO rust_erp_outbox (
        event_key, event_type, aggregate_type, aggregate_id, payload
    ) VALUES (
        'inventory.movement:' || NEW.id,
        'inventory.movement',
        'stock_movement',
        NEW.id::TEXT,
        jsonb_build_object(
            'schema_version', 1,
            'movement_id', NEW.id,
            'product_id', NEW.product_id,
            'variant_id', NEW.variant_id,
            'movement_type', NEW.type,
            'quantity', NEW.quantity,
            'old_stock', NEW.old_stock,
            'new_stock', NEW.new_stock,
            'reason', NEW.reason,
            'reference', NEW.reference,
            'location', NEW.location,
            'created_by', NEW.created_by,
            'created_at', NEW.created_at
        )
    ) ON CONFLICT (event_key) DO NOTHING;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_rust_erp_stock_movement ON stock_movements;
CREATE TRIGGER trg_rust_erp_stock_movement
AFTER INSERT ON stock_movements
FOR EACH ROW EXECUTE FUNCTION rust_enqueue_stock_movement_for_erp();

COMMIT;
