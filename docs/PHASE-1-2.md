# Phase 1 and 2: database compatibility and reports

## Scope

Uses the existing Main POS PostgreSQL database. Restaurant, network printing and
AI remain out of scope. No existing financial records are repaired or rewritten.
No new reporting database or duplicated balances are created.

## Phase 1

Settings > Database > Check compatibility is administrator-only. It reads the
configured connection's schema and privileges, checks required report columns
and relevant numeric/date types, checkout/inventory/ZKTeco prerequisites, employee
form columns and request-ledger uniqueness/INSERT permission. It does not migrate
on startup. A successful check is not a complete integrity audit of every Main
module or proof that all business totals agree.

The missing `rust_employee_requests` table is supplied by migration 003. It makes
employee command retries idempotent; application authorization still runs on each
command. Never delete its rows to bypass pending-request recovery.

`scripts/phase1-database.ps1` is read-only by default. `-ApplyEmployeeLedger` first
creates a full custom-format pg_dump backup in ignored `database-backups`, restricts
the folder ACL and verifies the archive table of contents. Then it applies only
migration 003 under an advisory lock in a transaction with lock/statement timeouts.
It never changes pre-existing business rows, replaces schema, or grants broad
client privileges. Backup listing is not a full restore rehearsal; schedule one
on a separate secured server. Do not restore over the live database.

Run under the reviewed database owner/client role. If an owner different from the
client creates the ledger, the DBA must grant that client SELECT and INSERT on
`rust_employee_requests`; no sequence is used. Repeat the compatibility check with
each distinct client database role. Existing broad database roles are not tightened
automatically, since Main POS shares them. No new write privileges on existing
tables are granted.

The owner CLI example `cargo run -p pos_core --example compatibility` reads
`ZAY_POS_DATABASE_URL`, emits no credentials, and exits nonzero on failed checks.
Never put the URL/password in version control or shell transcript output.

## Phase 2

Reports navigation is available to managers/admins. Core revalidates the session
against users on each load, and exports revalidate before writing the workbook.
All six report tabs share one read-only repeatable-read snapshot with a 20-second
per-statement timeout. Date bounds are inclusive local calendar dates using the
shared timestamp-without-time-zone convention. Today/week/month and the global
header refresh reload the snapshot. Tables paginate at 50 rows; XLSX includes all
rows, all six tabs, snapshot time, currency label and accounting notes. No exchange
rate conversion is performed. Text is written as string cells, never formulas.

| Tab | Basis |
| --- | --- |
| Sales | Completed and fully refunded receipts by original sale date, with status. Recorded totals on refunded rows are not new sales. |
| Expenses | Recorded expenses by expense_date, including existing payroll expenses. |
| Profit & Loss | Completed sale totals minus recorded sale-item historical costs, minus recorded expenses; monthly and overall totals. Discounts already included in sale totals are not subtracted again. |
| Financial Summary | Separate sales, discounts, checkout receipts net of change, credit collections, expense and supplier ledger totals. No cash-on-hand/net-cash claim. |
| Receivables | Current customer balance, invoice outstanding, overdue and difference, including customer credits. Not historical balances at the selected end date. |
| Payables | Current Main supplier ledger balances; purchases are debit and other non-null types credit. Payables and advances remain separate. |

Money calculations use PostgreSQL numeric and Rust Decimal. Null item costs or
receipts without items make affected COGS/profit Unknown, not zero and not today's
product cost. Zero stored costs remain recorded zero; accuracy depends on the
original entries. Completed receipts with refunded item quantities are rejected
with a reconciliation error because Main versions lack a uniform partial-refund
amount/date model. No automatic correction is attempted.

Refund metrics use original sale date, not refund date. Current balances are
labelled as such. Supplier credits may include adjustments and supplier payments
may overlap expenses; no misleading combined cash-out total is produced. Existing
customer balances may legitimately differ from invoice balances because of opening
balances/adjustments. These reports do not silently reconcile either source.

## Verification

- Isolated PostgreSQL fixtures: phase1_compatibility_and_idempotent_employee_migration,
  phase2_reports_financial_semantics_and_permissions,
  phase2_reports_unknown_cost_and_partial_refund_fail_closed.
- Existing checkout/inventory concurrency and employee financial retry tests.
- Desktop XLSX and exact-decimal presentation tests; XML validation confirms 60
  data rows beyond one UI page and no formulas from untrusted text.
- Playwright CSS harness `scripts/test-reports-layout.cjs`: light/dark at widths
  1366/768/390, visible export/title, tab bounds, scrolling and fixed shell chrome.
  This is a representative markup harness, not a logged-in native-app E2E test.
- Live configured schema preflight after the additive migration. Other client PCs,
  production write workflows and a full backup restore still require a pilot.

No release publication or client installation is performed by these changes.
