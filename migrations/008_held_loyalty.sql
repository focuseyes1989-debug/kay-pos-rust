CREATE TABLE IF NOT EXISTS rust_held_requests (
    request_id TEXT PRIMARY KEY, username TEXT NOT NULL, payload_hash TEXT NOT NULL,
    result_json TEXT NOT NULL, created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS rust_held_sessions (
    token TEXT PRIMARY KEY, username TEXT NOT NULL, held_id INTEGER NOT NULL,
    body TEXT NOT NULL, state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','returned','completed')),
    sale_id INTEGER UNIQUE REFERENCES sales(id), created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS rust_sale_points (
    sale_id INTEGER PRIMARY KEY REFERENCES sales(id), customer_id INTEGER NOT NULL REFERENCES customers(id),
    earned INTEGER NOT NULL CHECK(earned>=0), refunded BOOLEAN NOT NULL DEFAULT FALSE
);
