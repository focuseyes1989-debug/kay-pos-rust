# KAY POS Rust 0.2.9

## Changes
- Single AI chat workspace with topic selection using `/`, conversational sales and expense answers, and date follow-ups.
- Receipt queries by date, exact total, category and payment method, including KPay account labels. Receipt details open in a centered dialog.
- Category sales reporting, including Service/Services matching and Burmese category spelling normalization. Category totals allocate receipt discount/tax proportionally; current product category assignments are used.
- Date range calendar with presets and SVG dropdown icons across client pages.
- Employee attendance filters and guidance when the selected period has no records; employee profile creation action.
- ERP Operations page and navigation icon, with ERP delivery, synchronization, reconciliation and accounting support.

## Installation and database prerequisites
Finish sales, resolve pending checkouts and sign out before installing through the built-in updater. The ZIP contains only pos_desktop.exe; preserve existing database configuration and pending files.

This binary uses the existing release signing key. Older clients with a different signing key require manual replacement of pos_desktop.exe after closing the app and backing up the existing executable; do not bypass signature verification. Microsoft Edge WebView2 Runtime is required.

ERP Operations requires server migrations 010–014 from the Rust project. They are not included or applied automatically by this update. The configured server was prepared separately; other servers must have the required schema installed before using ERP Operations. Existing reporting and employee database prerequisites still apply.

AI chat runs supported read-only POS queries locally; it is not an unrestricted language model. Access permissions are checked. Category queries use current product assignments. Partial refunds can block category summaries for review. Payment filters match supported payment labels and KPay account suffixes.

## Verification
Rust unit tests and desktop checks; AI, date range and SVG browser layout fixtures; read-only database checks for product, category, expense and receipt queries; executable version self-check and signed ZIP verification. Browser fixtures do not replace a native client smoke test. Database integration tests requiring an isolated test cluster are not run against the production database.
