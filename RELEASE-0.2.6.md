# KAY POS Rust 0.2.6

## Changes
- Reports > Dashboard for managers and administrators: sales, checkout receipts, credit collections, expenses, supplier ledger activity and daily sales.
- Current customer receivables, outstanding/overdue invoices, supplier payables/advances, stock alerts, open service orders and active employee count when available.
- Date presets, date-range filtering, header Refresh and links to related pages.
- Dashboard scroll fix: the whole page scrolls inside the app while the header/status bar stay in place; long tables scroll independently.

## Reporting Scope
Period activity and current balances are separate. Refunded sales use the original sale date, not the refund payment date. Expenses and supplier payments are separate source totals and may overlap; this is not a cash-on-hand or audited net-profit report. Supplier balances use supplier_payments; purchase orders without ledger entries are highlighted separately. Credit marked fully paid is an attention metric, not an automatic correction. Existing Main POS tables are required; no migrations or production balance corrections run on install.

## Install
IMPORTANT: 0.2.6 establishes a replacement signing-key baseline. The private key used for 0.2.4 and 0.2.5 could not be recovered. Those clients cannot authenticate this update through their built-in updater.

For all existing clients, finish sales and resolve pending checkouts, close the app, back up the old EXE, and replace only pos_desktop.exe from this official release ZIP. Keep database configuration and pending-data files. Reopen and confirm version 0.2.6. Never bypass signature verification. Future compatible releases will use this baseline's key and support the built-in updater. GitHub rate limits may temporarily block checking; retry after the stated reset time, not repeatedly.

Pilot on one client first. WebView2 is required. ZIP signing is not Windows Authenticode signing.

## Verification
Core/desktop unit tests, isolated database dashboard tests, desktop/mobile wheel and nested-table scrolling tests, executable version check, signed-package tamper rejection and staged updater tests.
