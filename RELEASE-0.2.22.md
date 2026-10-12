# KAY POS Rust 0.2.22

- Add unsold-product questions in KAY Assistant: ask in Myanmar or English, including a selected period. Results show products with current stock and no completed positive-quantity sales in that period. Refunded sales still count as previous sales; Service/Restaurant products are excluded.
- Make Activity Log actions and details easier to read, with original audit codes available under an expandable disclosure.
- Keep the existing interface as Myanmar and add English interface text for Assistant, Activity Log and related system messages. Product/customer names and original audit records stay unchanged.
- Add icon-only Myanmar and UK flags after Refresh in the menu bar. Click a flag to save and apply the language selection.
- Correct amount-based receipt question parsing so request wording is not mistaken for a category name.
- Improve expense budget and alert layouts, amount formatting and status indicators.

Finish sales and resolve pending checkout requests, then sign out and use Settings > App Updates or the login-screen updater to install. The signed Windows x64 ZIP contains only pos_desktop.exe. Preserve existing database configuration and pending files.

No database migration is included. Language choices are saved in existing settings. Uses the existing release signing key. Microsoft Edge WebView2 Runtime is required.
