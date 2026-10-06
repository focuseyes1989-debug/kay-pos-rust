# ERP Phase 3: Purchasing and Stock Reconciliation

Phase 3 adds outbound procurement events and inbound, non-destructive stock comparison.

## Outbound Events

- `purchase.order.upserted`: a new or revised pending purchase order
- `purchase.order.cancelled`: a cancelled pending order and its reason
- `purchase.goods_received`: completed purchase, supplier, items, batches, locations and stock-movement IDs
- `supplier.payment_recorded`: allocated or unallocated supplier payment

These events use the Phase 1 outbox and are committed in the same transaction as the purchase operation. Goods receipt also produces the existing `inventory.movement` events for each stock movement.

## Stock Snapshot Contract

The POS sends `GET` with `limit=500` and a `cursor` for later pages. Each page must retain the same immutable `snapshot_id`.

```json
{
  "snapshot_id": "warehouse-close-2026-09-27T18:00:00Z",
  "cursor": null,
  "has_more": false,
  "items": [
    {
      "product_external_id": "product-1",
      "variant_external_id": null,
      "location_external_id": "location-1",
      "quantity": 25
    }
  ]
}
```

Identity resolution uses mappings created by Phase 2. Results are stored as `matched`, `difference`, or `unmapped`. This process never updates product, variant, batch or location stock. Differences require an explicitly reviewed adjustment through the POS inventory workflow.

Responses are limited to 10 MB and 1000 items per page. Quantities must be finite and non-negative. Failed or partial runs are marked for inspection; retrying the same snapshot replaces its partial rows.

## Installation

Phase 1 migration `010` and Phase 2 migration `011` must already be installed. Then run:

```powershell
./scripts/erp-phase3-database.ps1
./scripts/erp-phase3-database.ps1 -Apply
```

Enable reconciliation only after the endpoint is available:

```dotenv
ZAY_POS_ERP_STOCK_SNAPSHOT_URL=https://erp.example.com/api/pos/stock-snapshot
ZAY_POS_ERP_RECONCILIATION_INTERVAL_SECONDS=900
```

Leaving the snapshot URL empty disables reconciliation. Procurement events continue to queue through the Phase 1 outbox.
