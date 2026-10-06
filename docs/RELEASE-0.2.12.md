KAY POS Rust 0.2.12 for Windows x64.

- Upgraded Kay Assistant with conversational follow-ups, explicit name suggestions, sales/expense period comparisons, filter and calculation explanations, and detail dialogs.
- Added Myanmar date phrases for today, yesterday, this/previous week, month and year, days ago, and day/month/year dates. Receipt category queries work directly, e.g. ဒီနေ့ ရေခဲမုန့် receipts တွေပြပါ, with amount and payment filters retained.
- Added Customer and Employee name, phone and Employee ID searches with permission-checked detail dialogs and improved dialog spacing.
- Refreshed the Assistant input box with a topic chooser, icon send button and New chat action. Removed the redundant page header.
- Added Kay Assistant and Add Expense icons to the header; moved Cashdrawer and Customer Display icon controls beside Refresh.
- Added Remember me to sign-in. It remembers only the username after successful login; the password is still required. Unchecking removes the remembered username.
- Completed automatic date/search filtering and removed redundant Apply buttons. Added SVG search indicators and simplified close controls to icons with color hover feedback.

POS-only mode remains enabled. This client release contains no database migration. Existing database connection configuration and shop data are preserved. Assistant uses offline curated queries; it is not a general-purpose language model.

Install through Settings > App Updates after finishing sales and signing out. Alternatively, close KAY POS and extract the signed ZIP into the existing client folder, preserving configuration files.
