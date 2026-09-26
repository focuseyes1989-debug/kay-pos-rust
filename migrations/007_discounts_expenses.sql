-- Additive Phase 4 support. Existing Main POS tables remain authoritative.
CREATE TABLE IF NOT EXISTS rust_management_requests (
    request_id TEXT PRIMARY KEY,
    username TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    result_id INTEGER NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS rust_expense_files (
    attachment_id INTEGER PRIMARY KEY REFERENCES expense_attachments(id) ON DELETE CASCADE,
    content BYTEA NOT NULL CHECK(octet_length(content) BETWEEN 1 AND 10485760),
    sha256 TEXT NOT NULL
);
