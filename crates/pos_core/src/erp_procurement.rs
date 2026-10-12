use crate::storage as sqlx;
use anyhow::{Context, Result};
use serde_json::Value;
use sqlx::PgConnection;

async fn enqueue(
    conn: &mut PgConnection,
    event_key: String,
    event_type: &str,
    aggregate_type: &str,
    aggregate_id: i32,
    payload: Value,
) -> Result<()> {
    sqlx::query("INSERT INTO rust_erp_outbox(event_key,event_type,aggregate_type,aggregate_id,payload) VALUES($1,$2,$3,$4,$5) ON CONFLICT(event_key) DO NOTHING")
        .bind(event_key)
        .bind(event_type)
        .bind(aggregate_type)
        .bind(aggregate_id.to_string())
        .bind(payload)
        .execute(&mut *conn)
        .await
        .context("ERP outbox unavailable; apply migrations/010_erp_outbox.sql")?;
    Ok(())
}

pub async fn enqueue_order(conn: &mut PgConnection, draft_id: i32, event_type: &str) -> Result<()> {
    if !crate::ERP_ENABLED { return Ok(()); }
    let payload: Value = sqlx::query_scalar(
        r#"
        SELECT jsonb_build_object(
            'schema_version', 1,
            'order', to_jsonb(d),
            'draft', d.body::jsonb,
            'supplier', jsonb_build_object(
                'id', s.id,
                'name', s.name,
                'company_name', s.company_name,
                'tax_number', s.tax_number
            )
        )
        FROM rust_purchase_drafts d
        JOIN suppliers s ON s.id=d.supplier_id
        WHERE d.id=$1
        "#,
    )
    .bind(draft_id)
    .fetch_optional(&mut *conn)
    .await?
    .context("Purchase draft not found while creating ERP event")?;
    let revision: i32 = sqlx::query_scalar("SELECT revision FROM rust_purchase_drafts WHERE id=$1")
        .bind(draft_id)
        .fetch_one(&mut *conn)
        .await?;
    enqueue(
        conn,
        format!("{event_type}:{draft_id}:{revision}"),
        event_type,
        "purchase_order",
        draft_id,
        payload,
    )
    .await
}

pub async fn enqueue_goods_received(conn: &mut PgConnection, draft_id: i32) -> Result<()> {
    if !crate::ERP_ENABLED { return Ok(()); }
    let (po_id, payload): (i32, Value) = sqlx::query_as(
        r#"
        SELECT p.id, jsonb_build_object(
            'schema_version', 1,
            'purchase_order', to_jsonb(p),
            'supplier', jsonb_build_object(
                'id', s.id,
                'name', s.name,
                'company_name', s.company_name,
                'tax_number', s.tax_number
            ),
            'items', COALESCE((
                SELECT jsonb_agg(
                    to_jsonb(i) || jsonb_build_object(
                        'variant_id', ri.variant_id,
                        'location', ri.location,
                        'batch_no', ri.batch_no,
                        'expire_date', ri.expire_date
                    ) ORDER BY i.id
                )
                FROM purchase_order_items i
                LEFT JOIN rust_purchase_items ri ON ri.po_item_id=i.id
                WHERE i.po_id=p.id
            ), '[]'::jsonb),
            'stock_movement_ids', COALESCE((
                SELECT jsonb_agg(m.movement_id ORDER BY m.movement_id)
                FROM rust_purchase_movements m WHERE m.po_id=p.id
            ), '[]'::jsonb)
        )
        FROM rust_purchase_drafts d
        JOIN purchase_orders p ON p.id=d.po_id
        LEFT JOIN suppliers s ON s.id=p.supplier_id
        WHERE d.id=$1
        "#,
    )
    .bind(draft_id)
    .fetch_optional(&mut *conn)
    .await?
    .context("Received purchase not found while creating ERP event")?;
    enqueue(
        conn,
        format!("purchase.goods_received:{po_id}"),
        "purchase.goods_received",
        "purchase_receipt",
        po_id,
        payload,
    )
    .await
}

pub async fn enqueue_supplier_payment(conn: &mut PgConnection, payment_id: i32) -> Result<()> {
    if !crate::ERP_ENABLED { return Ok(()); }
    let payload: Value = sqlx::query_scalar(
        r#"
        SELECT jsonb_build_object(
            'schema_version', 1,
            'payment', to_jsonb(p),
            'supplier', jsonb_build_object(
                'id', s.id,
                'name', s.name,
                'company_name', s.company_name,
                'tax_number', s.tax_number
            ),
            'purchase_order', CASE WHEN o.id IS NULL THEN NULL ELSE to_jsonb(o) END
        )
        FROM supplier_payments p
        JOIN suppliers s ON s.id=p.supplier_id
        LEFT JOIN purchase_orders o ON o.id=p.purchase_order_id
        WHERE p.id=$1
        "#,
    )
    .bind(payment_id)
    .fetch_optional(&mut *conn)
    .await?
    .context("Supplier payment not found while creating ERP event")?;
    enqueue(
        conn,
        format!("supplier.payment_recorded:{payment_id}"),
        "supplier.payment_recorded",
        "supplier_payment",
        payment_id,
        payload,
    )
    .await
}
