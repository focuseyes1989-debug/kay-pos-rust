-- Run once as the application database role, after taking a backup.
-- Existing identifiers are not rewritten. Never reset these sequences.
BEGIN;
CREATE SEQUENCE IF NOT EXISTS rust_invoice_number_seq;
CREATE SEQUENCE IF NOT EXISTS rust_batch_number_seq;
COMMIT;
