-- Apply once as the database owner. Employee tables remain owned by Main KAY POS.
CREATE TABLE IF NOT EXISTS rust_employee_requests (
    request_id TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id),
    operation TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    record_id INTEGER NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
-- Grant SELECT, INSERT on this table to the same client role used for checkout.
-- Never grant clients permission to delete completed request/audit records.
