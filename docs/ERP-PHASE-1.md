# ERP Phase 1: Reliable Outbound Sync

Phase 1 records business events in PostgreSQL in the same transaction as the POS operation. A background worker later sends those events to one configured ERP webhook. ERP or network downtime therefore does not lose a completed local sale.

## Events

- `sale.completed`: complete sale row and sale-item rows
- `sale.refunded`: updated sale row and refunded sale-item rows
- `inventory.movement`: stock movement identifiers, quantities, location and reference

Every request has an `Idempotency-Key` header and a JSON envelope:

```json
{
  "event_id": 42,
  "idempotency_key": "sale.completed:123",
  "event_type": "sale.completed",
  "aggregate_type": "sale",
  "aggregate_id": "123",
  "occurred_at": "2026-09-27T12:00:00",
  "payload": {}
}
```

The receiver must persist the idempotency key and return any `2xx` response only after accepting the event durably. Failed deliveries retry with exponential backoff. After 12 attempts the event remains in `dead` status for inspection and replay in a later operations phase.

## Database Installation

First run the read-only preflight:

```powershell
./scripts/erp-phase1-database.ps1
```

Review `migrations/010_erp_outbox.sql`, then apply it with a verified backup:

```powershell
./scripts/erp-phase1-database.ps1 -Apply
```

The application database role needs `SELECT`, `INSERT`, and `UPDATE` on `rust_erp_outbox`, plus sequence usage on `rust_erp_outbox_id_seq` when a different schema-owner role applies the migration.

## Configuration

Set these values in the deployed `.env` only after the ERP receiver is ready:

```dotenv
ZAY_POS_ERP_WEBHOOK_URL=https://erp.example.com/api/pos/events
ZAY_POS_ERP_API_TOKEN=replace-with-a-secret
ZAY_POS_ERP_TENANT=shop-or-company-id
ZAY_POS_ERP_SYNC_INTERVAL_SECONDS=30
ZAY_POS_ERP_SYNC_BATCH_SIZE=25
```

Leaving `ZAY_POS_ERP_WEBHOOK_URL` empty disables delivery while continuing to record events. Existing historical sales and stock movements are not backfilled by this migration.
