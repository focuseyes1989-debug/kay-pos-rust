-- Apply once to the server database before deploying this client version.
-- Run as the same database role used by the client, or grant that role
-- SELECT, INSERT, UPDATE on this table after creation.
BEGIN;
CREATE TABLE IF NOT EXISTS rust_checkout_requests (
    request_id text PRIMARY KEY,
    fingerprint text NOT NULL,
    -- NULL is a cancelled request tombstone: a delayed retry must not sell it.
    sale_id integer REFERENCES sales(id),
    created_at timestamp NOT NULL DEFAULT CURRENT_TIMESTAMP
);
COMMIT;
