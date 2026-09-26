use crate::auth::{Permission, Session};
use anyhow::Result;
use sqlx::{PgConnection, PgPool};

#[derive(Clone, Debug)]
pub struct Check {
    pub area: String,
    pub object: String,
    pub ok: bool,
    pub detail: String,
}

pub async fn load(pool: &PgPool, actor: &Session) -> Result<Vec<Check>> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout='15s'")
        .execute(&mut *tx)
        .await?;
    actor.authorize(&mut tx, Permission::Admin).await?;
    let result = inspect(&mut tx).await?;
    tx.commit().await?;
    Ok(result)
}

// Also used by the owner's preflight tool; no initialization or writes occur here.
pub async fn inspect(conn: &mut PgConnection) -> Result<Vec<Check>> {
    let requirements = [
        ("Audit", "user_activity_log", "id user_id username action details ip_address created_at"),
        ("Category groups", "category_groups", "id name description sort_order icon color is_favorite is_active created_at updated_at"),
        ("Category groups", "categories", "id name group_id parent_id description status is_system sort_order updated_at"),
        ("Service orders", "service_orders", "id order_no customer_name customer_phone job_title complaint internal_notes status received_at expected_at updated_at"),
        ("Phase 4", "rust_management_requests", "request_id username payload_hash result_id"),
        ("Phase 4", "rust_expense_files", "attachment_id content sha256"),
        ("Phase 5", "rust_held_requests", "request_id username payload_hash result_json"),
        ("Phase 5", "rust_held_sessions", "token username held_id body state sale_id"),
        ("Phase 5", "rust_sale_points", "sale_id customer_id earned refunded"),
        ("Purchases", "purchase_orders", "id po_no supplier_id order_date total_amount status discount tax payment_status received_by notes"),
        ("Purchases", "purchase_order_items", "id po_id product_id quantity unit_price total"),
        ("Purchases", "rust_purchase_drafts", "id po_no supplier_id order_date body total_amount status revision po_id created_by updated_at"),
        ("Purchases", "rust_purchase_requests", "request_id username payload_hash result_id cancelled"),
        ("Purchases", "rust_purchase_items", "po_item_id variant_id location batch_no expire_date"),
        ("Purchases", "rust_purchase_movements", "movement_id po_id"),
        ("Reports","sales","id invoice_no total payment change_amount customer_id status payment_type discount_amount created_at"),
        ("Reports","sale_items","sale_id qty cost refunded_qty"),
        ("Reports","customers","id name current_balance"),
        ("Reports","expenses","id expense_no category description amount expense_date payment_method"),
        ("Reports","credit_sales","customer_id balance_amount due_date status"),
        ("Reports","credit_payments","amount payment_date"),
        ("Reports","suppliers","id name"),
        ("Reports","supplier_payments","supplier_id amount payment_type payment_date"),
        ("Checkout","rust_checkout_requests","request_id"),
        ("Employee writes","rust_employee_requests","request_id user_id operation payload_hash record_id created_at"),
        ("ZKTeco","zkteco_devices","id device_no name ip_address port comm_key serial_no last_sync_at is_active"),
        ("ZKTeco","zkteco_employee_mappings","id device_id employee_id device_user_id"),
        ("ZKTeco","zkteco_attendance_logs","id device_id device_user_id employee_id punch_time status punch verification_type is_valid validation_note"),
        ("Inventory","products","id stock cost sold_by"),
        ("Inventory","product_variants","id product_id stock"),
        ("Inventory","product_locations","id product_id location batch_no expire_date quantity"),
        ("Inventory","variant_stock_batches","product_id variant_id location batch_no expire_date quantity expiry_unknown"),
        ("Inventory","stock_movements","id product_id variant_id quantity old_stock new_stock"),
    ];
    let mut checks = Vec::new();
    for (area, table, columns) in requirements {
        checks.push(
            check_table(
                conn,
                area,
                table,
                &columns.split_whitespace().collect::<Vec<_>>(),
            )
            .await?,
        );
    }
    use crate::employees::Section;
    for (table, update, sequence) in [("user_activity_log",false,true),("category_groups",true,true),("rust_management_requests",false,false),("rust_expense_files",false,false),("rust_held_requests",false,false),("rust_held_sessions",true,false),("rust_sale_points",true,false)] {
        let exists:bool=sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL").bind(table).fetch_one(&mut *conn).await?;
        if !exists {continue}
        let insert:bool=sqlx::query_scalar("SELECT has_table_privilege($1,'INSERT')").bind(table).fetch_one(&mut *conn).await?;
        let can_update:bool=sqlx::query_scalar("SELECT has_table_privilege($1,'UPDATE')").bind(table).fetch_one(&mut *conn).await?;
        checks.push(Check{area:"Phase 4-6 writes".into(),object:table.into(),ok:insert&&(!update||can_update),detail:format!("INSERT: {insert}; UPDATE required: {update}; UPDATE available: {can_update}")});
        if sequence {
            let id_exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass($1) AND attname='id' AND NOT attisdropped)").bind(table).fetch_one(&mut *conn).await?;
            let usable=if id_exists {sqlx::query_scalar::<_,Option<bool>>("SELECT has_sequence_privilege(pg_get_serial_sequence($1,'id'),'USAGE')").bind(table).fetch_one(&mut *conn).await?}else{Some(false)};
            checks.push(Check{area:"Phase 6 writes".into(),object:format!("{table}.id sequence"),ok:usable==Some(true),detail:"Sequence USAGE is required for audited writes and new groups".into()});
        }
    }
    for (table, update) in [
        ("rust_purchase_drafts", true),
        ("rust_purchase_requests", false),
        ("rust_purchase_items", false),
        ("rust_purchase_movements", false),
    ] {
        let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(table)
            .fetch_one(&mut *conn)
            .await?;
        if exists {
            let insert: bool = sqlx::query_scalar("SELECT has_table_privilege($1,'INSERT')")
                .bind(table)
                .fetch_one(&mut *conn)
                .await?;
            let can_update: bool = sqlx::query_scalar("SELECT has_table_privilege($1,'UPDATE')")
                .bind(table)
                .fetch_one(&mut *conn)
                .await?;
            checks.push(Check {
                area: "Purchase writes".into(),
                object: table.into(),
                ok: insert && (!update || can_update),
                detail: format!(
                    "INSERT: {insert}; UPDATE required: {update}; UPDATE available: {can_update}"
                ),
            });
            if update {
                let usable:Option<bool>=sqlx::query_scalar("SELECT has_sequence_privilege(pg_get_serial_sequence('rust_purchase_drafts','id'),'USAGE')").fetch_one(&mut *conn).await?;
                checks.push(Check {
                    area: "Purchase writes".into(),
                    object: "Draft ID sequence".into(),
                    ok: usable == Some(true),
                    detail: "Sequence USAGE is required to save new orders".into(),
                });
            }
        }
    }
    for section in [
        Section::Employees,
        Section::Attendance,
        Section::Shifts,
        Section::Assignments,
        Section::Payroll,
        Section::Leave,
        Section::Documents,
        Section::Advances,
        Section::Commission,
        Section::Cash,
    ] {
        let mut columns = vec!["id"];
        columns.extend(section.fields().iter().map(|f| f.key));
        checks.push(check_table(conn, "Employees", section.table(), &columns).await?);
    }
    let exists: bool =
        sqlx::query_scalar("SELECT to_regclass('rust_employee_requests') IS NOT NULL")
            .fetch_one(&mut *conn)
            .await?;
    if exists {
        let insert: bool =
            sqlx::query_scalar("SELECT has_table_privilege('rust_employee_requests','INSERT')")
                .fetch_one(&mut *conn)
                .await?;
        let unique:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_constraint c WHERE c.conrelid=to_regclass('rust_employee_requests') AND c.contype IN ('p','u') AND c.conkey=ARRAY[(SELECT attnum FROM pg_attribute WHERE attrelid=c.conrelid AND attname='request_id')]::smallint[])").fetch_one(&mut *conn).await?;
        checks.push(Check {
            area: "Employee writes".into(),
            object: "Request ledger safety".into(),
            ok: insert && unique,
            detail: format!("INSERT permission: {insert}; unique request ID: {unique}"),
        });
    }
    Ok(checks)
}

async fn check_table(
    conn: &mut PgConnection,
    area: &str,
    table: &str,
    required: &[&str],
) -> Result<Check> {
    let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
        .bind(table)
        .fetch_one(&mut *conn)
        .await?;
    if !exists {
        return Ok(Check {
            area: area.into(),
            object: table.into(),
            ok: false,
            detail: "Missing table; apply the reviewed server migration.".into(),
        });
    }
    let columns:Vec<(String,String)>=sqlx::query_as("SELECT a.attname::text,t.typname::text FROM pg_attribute a JOIN pg_type t ON t.oid=a.atttypid WHERE a.attrelid=to_regclass($1) AND a.attnum>0 AND NOT a.attisdropped").bind(table).fetch_all(&mut *conn).await?;
    let missing: Vec<&str> = required
        .iter()
        .copied()
        .filter(|c| !columns.iter().any(|(v, _)| v == c))
        .collect();
    let incompatible: Vec<String> = columns
        .iter()
        .filter(|(column, kind)| {
            required.contains(&column.as_str()) && !compatible_type(table, column, kind)
        })
        .map(|(column, kind)| format!("{column} ({kind})"))
        .collect();
    let readable: bool = sqlx::query_scalar("SELECT has_table_privilege($1,'SELECT')")
        .bind(table)
        .fetch_one(&mut *conn)
        .await?;
    let ok = missing.is_empty() && incompatible.is_empty() && readable;
    Ok(Check {
        area: area.into(),
        object: table.into(),
        ok,
        detail: if ok {
            "Required columns, checked types and SELECT permission available".into()
        } else {
            format!(
                "Missing columns: {}; incompatible types: {}; SELECT permission: {readable}",
                missing.join(", "),
                incompatible.join(", ")
            )
        },
    })
}

fn compatible_type(table: &str, column: &str, kind: &str) -> bool {
    if table == "sales" && column == "created_at" {
        return kind == "timestamp";
    }
    if matches!(
        (table, column),
        ("expenses", "expense_date")
            | ("credit_payments", "payment_date")
            | ("supplier_payments", "payment_date")
            | ("credit_sales", "due_date")
    ) {
        return matches!(kind, "text" | "varchar" | "bpchar");
    }
    if matches!(
        column,
        "total"
            | "payment"
            | "change_amount"
            | "discount_amount"
            | "qty"
            | "cost"
            | "refunded_qty"
            | "current_balance"
            | "amount"
            | "balance_amount"
            | "stock"
            | "quantity"
            | "old_stock"
            | "new_stock"
    ) {
        return matches!(
            kind,
            "numeric" | "float8" | "float4" | "int2" | "int4" | "int8"
        );
    }
    true
}
