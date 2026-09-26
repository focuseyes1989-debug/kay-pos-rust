# Phase 7: local AI assistants

The AI page provides AI Chat (explicit intent routing), daily sales analytics,
Dashboard Assistant, Product Assistant and an on-demand summary/digest.
This is a deterministic, read-only assistant matching the Main POS approach,
not a general-purpose LLM chatbot. No external provider receives data.

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
reasoning is claimed. External model integration and saved chat history remain
separate follow-up work. Production migrations and release packaging are not
part of this change.
