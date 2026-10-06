# Phase 7: local AI assistants

The AI page provides AI Chat (explicit intent routing), daily sales analytics,
Dashboard Assistant, Product Assistant and an on-demand summary/digest.
Analytics, Dashboard, Product and Digest reports remain deterministic and read-only.
AI Chat now optionally uses a user-configured OpenAI-compatible chat-completions
endpoint. Without a configured endpoint/model the report tabs still work; model
chat is disabled. See the upgraded chat workflow below.

Supported chat requests: `dashboard`, `sales trend`, `digest`,
`product <name / SKU / barcode>` and the Burmese aliases in `ai::chat`.
Dates are inclusive with a maximum range of 366 days. Product results are
limited to 50; stock is the current product total, not variant availability.
Amounts remain in the database's stored currency. Credit sales are not cash
received; current balances are not historical balances. Partial refunds and
invalid receipt totals block summaries rather than manufacture a total.

## Permissions and audit

Requests revalidate the authenticated account and require manager/admin access.
Non-admins additionally require Main POS `ai_pages` plus scope permissions:

- Products: `products`.
- Analytics: `sales_summary`.
- Dashboard/digest: `dashboard,sales_summary,expense,credit,products`.

Permissions combine `users.permissions` and `user_roles.permissions`.
Missing permission schema fails closed; no automatic grants are made.
Successful results require an audit commit after permission revalidation.
`rust.ai.query` records a request fingerprint, scope, period and sources, not
the raw question or answer. Failed requests attempt a metadata-only
`rust.ai.failed` event for authenticated managers. An unavailable audit store
can prevent failure logging, but successful answers are still withheld.
Queries use fixed SQL, bound parameters and a read-only snapshot. The audit
transaction is the only write. Phase 6's `user_activity_log` is required.

## Deliberate limits

Digests are generated on demand, not scheduled or persisted. Main POS's
`ai_dashboard_digests` cache is not overwritten with a different payload.
No autonomous stock, credit, pricing or financial changes are permitted.
No profit forecasting, cash-on-hand calculation or general natural-language
reasoning is claimed. Durable saved chat history remains separate follow-up work. Production migrations and release packaging are not
part of this change.


## Upgraded chat workflow

1. Open AI Chat > AI connection.
2. Enter an OpenAI-compatible **base URL** (for example `https://api.openai.com/v1`), a model supported by that endpoint, and its API key. Use a local/LAN HTTP endpoint only for a local model server; public endpoints require HTTPS.
3. Click **Use AI connection**. This validates settings locally; it does not test connectivity, install a model, or make a paid API request.
4. Choose a date range and POS context (Sales, Dashboard, Products, Digest). For Products, supply a product search term. Ask a question in Burmese or English and click Send.

The endpoint receives the question, up to six previous completed question/answer pairs for the same data scope, and the current authorized POS snapshot. The model has no SQL tools or write access. Existing scope permissions are checked before sending the snapshot and again before displaying the response. The factual snapshot remains available under each response's expandable evidence panel. Relative-date requests and recognized report intents still use the existing router; arbitrary follow-up questions use the selected POS context.

Up to 20 turns are kept in memory while the page is open. New chat clears them. API keys and connection settings are kept only in memory for that page session; no key or raw question/answer is written to the activity log or configuration files. Navigating away discards them. A missing endpoint/model/key must be configured locally; no provider or model service is provisioned automatically.

The adapter uses non-streaming Chat Completions, with redirects disabled, a 90-second request timeout, bounded request context and a 1 MB response limit. Provider failure bodies are not displayed. History is limited to the current scope, and current snapshot facts take precedence over previous conversational figures.

API contract: [official OpenAI Chat Completions reference](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create).

Source validation without producing a desktop executable:

```powershell
cargo check -p pos_desktop --tests --offline
node scripts/test-ai-upgrade-layout.cjs
```

The browser test is a DOM/CSS harness, not a live desktop/database/model test. API connectivity and model output require the user's chosen endpoint and credentials.


### Offline POS chat (default)

AI Chat starts in offline mode. No endpoint, model, API key or internet connection is needed; the configured POS PostgreSQL server must remain reachable. Optional AI connection enables the existing model adapter; Use offline chat clears that connection and conversation.

Supported English/Burmese intents: sales/profit summaries, dashboard/digest, product name/SKU/barcode search (`product <term>` / `ပစ္စည်း <term>`), low/out stock list, expense categories, customers, current customer debt, recent receipts, employees, attendance, curated usage help and known error guidance. The router uses explicit read-only queries, not generated SQL. Unsupported questions show supported examples. Payroll is not supported. No project source index or local LLM is installed.

Dates: today/yesterday, this/last week, this/last month, Burmese equivalents, or one/two whitespace-separated YYYY-MM-DD dates (Myanmar digits accepted). A bare period follow-up uses the last chat scope. Current customers, balances, employees and stock are current snapshots regardless of date range. Lists are capped at 50; charts show the first 12 sales/expense rows. CSV export escapes spreadsheet formula cells. Related-page links open existing pages without applying chat filters. Permission checks apply to each data scope before and after the snapshot; attendance requires both employees and attendance. Help and diagnostics also require ai_pages.

Validation: cargo check compiles application and tests without producing a desktop executable. Seven added SQL queries were executed in a read-only transaction against the configured server. The browser harness checks CSS at desktop/mobile sizes; it does not run the native Dioxus application.


### Product, inventory and expense prompts

- `products` / `ပစ္စည်း`: first 50 products, with SKU, barcode, price and current stock.
- `product Milk`, `product ABC-001`, `ပစ္စည်း Milk ဈေးနှုန်း`: literal name/SKU/barcode search.
- `inventory` / `ကုန်လက်ကျန်`: current stock list; `stock Milk` / `Milk လက်ကျန်`: name/SKU/barcode filter.
- `low stock` / `လက်ကျန်နည်း`: stock at/below reorder threshold; `out of stock` / `လက်ကျန်ကုန်`: stock <= 0. Service/restaurant items are excluded; variants use summed stock.
- `expense`, `ဒီလ အသုံးစရိတ်`, `expenses yesterday`: expense amount grouped by category in the selected/resolved period.
- `expenses fuel this month`: case-insensitive literal category/description filter; dates may be supplied in YYYY-MM-DD form.

`python scripts/check-ai-catalog-queries.py` validates nine query variants against the configured database in a rolled-back read-only transaction. It prints row counts only.


### Chatbox-only interaction

AI Chat has one message textarea and Send button. Date range, POS context and product search controls remain absent from this tab; report tabs retain their date pickers. Every new chat question defaults to today's period unless its text supplies a relative/explicit date. Bare product names such as `kit kat` are interpreted as literal product searches. Topic/date words route supported business queries automatically. A short date follow-up keeps the previous topic. The offline router covers defined POS queries rather than arbitrary general-knowledge answers.


### Single AI Chat page

Analytics, Dashboard Assistant, Product Assistant and Summary/Digest tabs have been removed. Their data handlers remain accessible through chat prompts (`sales this month`, `dashboard`, `product <name>` / bare name, `digest`). The workspace now contains one conversation and one message composer, with no report mode or date/filter controls.

### Receipt amount queries and details

Ask `ဒီနေ့ 10000 ကျပ် ရောင်းရတဲ့ receipt ပြပါ` or `5.10.2026 ရက်နေ့10000 ကျပ် receipt ပြပေးပါ`. Explicit dates accept day.month.year, day/month/year, or year-month-day, including Myanmar digits and dates next to Burmese text. One amount is matched exactly against the completed receipt's final Total, with inclusive start date and exclusive next-day boundary. Details show at most 50 receipts and disclose additional matches. Click a receipt number to open its existing detail panel in a centered native modal dialog; chat details are read-only and revalidate AI/receipt permissions before and after loading. Close or Escape dismisses it. Long details scroll within the dialog while its Close button stays visible; native modal focus handling isolates the background and returns focus to the receipt after dismissal. The CSS/browser fixture in `scripts/test-ai-codex-layout.cjs` checks modal dimensions, scrolling, focus, Escape and Close at four viewport widths in both themes.

`python scripts/check-ai-receipt-queries.py` checks exact amounts, date boundaries, completed status and decimal amounts with read-only SQL fixtures, then verifies matching receipts and detail IDs on the configured database. It makes no data changes. Parser regression tests are compiled by `cargo check -p pos_desktop --tests --offline`; this check does not execute the Rust tests or produce an app executable.

### Category sales chat

### Slash topic picker

Payment-filtered receipt questions support KPay/KBZPay, WavePay, Cash and Credit, e.g. `ဒီနေ့ kpay နဲ့ရောင်းထားတဲ့ Receipts တွေပြပေးပါ`. KPay matches configured account labels such as `Kpay (510)` and `Kpay(889)`. Completed receipt filters can combine payment, category and exact total; date follow-ups retain all three. Payment matching uses bound fixed patterns, never patterns from user text. Receipt query checks cover account suffixes, aliases and combined filters.

Type `/` at the start of the AI chat message to open a topic picker above the composer. English command fragments or Myanmar labels filter its 15 topics. Choose with the mouse or Up/Down and Enter; selection inserts an editable prompt and never sends it automatically. Escape dismisses the menu. Send stays disabled for slash input, and regular dates containing `/` do not open it. The CSS/browser fixture checks menu placement, viewport bounds and internal scrolling at four widths in light/dark themes; `cargo check` verifies the Rust event handlers compile.

Receipt category prompts also work: `ဒီနေ့ ရေခဲမုန့် (category) ရောင်းထားတဲ့ Receipt ပြပေးပါ` or `5.10.2026 Service (category) 10000 ကျပ် receipt ပြပါ`. Matching uses an `EXISTS` condition on sale items and current product categories; multiple matching items do not duplicate a receipt. Only completed receipts with remaining matching item quantities are included. Unknown categories produce an error instead of falling back to all receipts. Displayed Total remains the entire receipt total. Receipt number links open the detail dialog, and date follow-ups retain category and amount filters. The receipt SQL check covers category inclusion/exclusion, duplicate prevention, returned item exclusion and combined amount/category conditions.

Ask `ဒီနေ့ Service ရောင်းရငွေ ပြပါ`, `5.10.2026 Services ရောင်းရငွေ ပြပါ`, `sales CCTV this month`, or `ဒီနေ့ categories အလိုက် ရောင်းရငွေ ပြပါ`. Category names match exactly ignoring case; `Service` also resolves to `Services` if there is no exact `Service` category. Unknown categories produce a clarification error. Category date follow-ups retain the category name.

Completed receipt totals are allocated to categories in proportion to sale item totals, so a mixed-category receipt contributes only its category share, including receipt discount/tax. Cash and credit sales both contribute. Partial refunds on matching receipts stop aggregation for review. Results use current product category assignments, not historical snapshots. `python scripts/check-ai-category-sales.py` verifies category allocation, exact matching, date/status exclusion and partial refunds in read-only fixtures and against the configured database.
