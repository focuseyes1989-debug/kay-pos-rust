ALTER TABLE customers ADD COLUMN name TEXT DEFAULT 'Test customer', ADD COLUMN points INTEGER DEFAULT 0;
CREATE TABLE held_sales (
    id SERIAL PRIMARY KEY, hold_no TEXT UNIQUE NOT NULL, cart_json TEXT NOT NULL,
    customer_id INTEGER, customer_name TEXT, payment_type TEXT, note TEXT,
    total_amount DOUBLE PRECISION, item_count INTEGER,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE customer_points_log (
    id SERIAL PRIMARY KEY, customer_id INTEGER REFERENCES customers(id), points INTEGER,
    type TEXT, reference TEXT, expiry_date TEXT, created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
INSERT INTO settings VALUES ('loyalty_points_per_dollar','0.025'),('points_expiry_months','12');
