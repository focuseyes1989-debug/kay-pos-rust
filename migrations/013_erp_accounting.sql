-- Balanced accounting documents and configurable chart-of-account mappings.
BEGIN;

CREATE TABLE IF NOT EXISTS rust_erp_account_mappings (
    mapping_key TEXT PRIMARY KEY,
    account_code TEXT NOT NULL,
    account_name TEXT NOT NULL,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO rust_erp_account_mappings(mapping_key,account_code,account_name) VALUES
    ('cash','1000','Cash and payment clearing'),
    ('accounts_receivable','1100','Accounts receivable'),
    ('inventory','1200','Inventory'),
    ('accounts_payable','2000','Accounts payable'),
    ('tax_payable','2100','Tax payable'),
    ('sales_revenue','4000','Sales revenue'),
    ('sales_discount','4050','Sales discounts'),
    ('cogs','5000','Cost of goods sold'),
    ('expense','6000','Operating expenses')
ON CONFLICT(mapping_key) DO NOTHING;

CREATE TABLE IF NOT EXISTS rust_erp_accounting_documents (
    id BIGSERIAL PRIMARY KEY,
    event_key TEXT NOT NULL UNIQUE,
    event_type TEXT NOT NULL,
    source_type TEXT NOT NULL,
    source_id INTEGER NOT NULL,
    document_date TEXT NOT NULL,
    currency TEXT NOT NULL DEFAULT 'MMK',
    total_debit NUMERIC(18,2) NOT NULL,
    total_credit NUMERIC(18,2) NOT NULL,
    lines JSONB NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK(total_debit = total_credit),
    CHECK(total_debit >= 0)
);

COMMIT;
