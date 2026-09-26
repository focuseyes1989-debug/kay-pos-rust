-- Run as the reviewed database owner, in a transaction after a verified backup.
-- Drafts remain separate: Main POS only sees purchases when goods are received.
CREATE TABLE IF NOT EXISTS rust_purchase_drafts (
    id SERIAL PRIMARY KEY,
    po_no TEXT UNIQUE NOT NULL,
    supplier_id INTEGER NOT NULL REFERENCES suppliers(id),
    order_date TEXT NOT NULL,
    body TEXT NOT NULL,
    total_amount NUMERIC(18,2) NOT NULL CHECK(total_amount >= 0),
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','received','cancelled')),
    revision INTEGER NOT NULL DEFAULT 1,
    po_id INTEGER UNIQUE REFERENCES purchase_orders(id),
    created_by TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS rust_purchase_requests (
    request_id TEXT PRIMARY KEY,
    username TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    result_id INTEGER,
    cancelled INTEGER NOT NULL DEFAULT 0 CHECK(cancelled IN (0,1)),
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS rust_purchase_items (
    po_item_id INTEGER PRIMARY KEY REFERENCES purchase_order_items(id),
    variant_id INTEGER REFERENCES product_variants(id),
    location TEXT NOT NULL,
    batch_no TEXT NOT NULL,
    expire_date TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS rust_purchase_movements (
    movement_id INTEGER PRIMARY KEY REFERENCES stock_movements(id),
    po_id INTEGER NOT NULL REFERENCES purchase_orders(id)
);
CREATE INDEX IF NOT EXISTS idx_rust_purchase_drafts_date ON rust_purchase_drafts(order_date,id);
CREATE INDEX IF NOT EXISTS idx_rust_purchase_movements_order ON rust_purchase_movements(po_id);
-- Client role: SELECT, INSERT, UPDATE on drafts; SELECT, INSERT on the other
-- three tables; USAGE on rust_purchase_drafts_id_seq. No DELETE grant is needed.
