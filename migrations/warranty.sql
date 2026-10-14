CREATE TABLE IF NOT EXISTS rust_serial_units (
 id TEXT PRIMARY KEY, serial TEXT NOT NULL UNIQUE,
 sale_item_id INTEGER NOT NULL REFERENCES sale_items(id),
 sale_id INTEGER NOT NULL REFERENCES sales(id),
 customer_id INTEGER NOT NULL REFERENCES customers(id),
 starts TEXT NOT NULL, ends TEXT NOT NULL,
 created_by TEXT NOT NULL, created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS rust_serial_units_item ON rust_serial_units(sale_item_id);
CREATE TABLE IF NOT EXISTS rust_warranty_claims (
 id TEXT PRIMARY KEY, unit_id TEXT NOT NULL REFERENCES rust_serial_units(id),
 issue TEXT NOT NULL, status TEXT NOT NULL CHECK(status IN ('open','in_progress','resolved','rejected')),
 covered INTEGER NOT NULL CHECK(covered IN (0,1)), created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS rust_warranty_claims_unit ON rust_warranty_claims(unit_id);
CREATE TABLE IF NOT EXISTS rust_warranty_events (
 id TEXT PRIMARY KEY, claim_id TEXT NOT NULL REFERENCES rust_warranty_claims(id),
 status TEXT NOT NULL, note TEXT NOT NULL, actor TEXT NOT NULL, created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS rust_warranty_events_claim ON rust_warranty_events(claim_id);
