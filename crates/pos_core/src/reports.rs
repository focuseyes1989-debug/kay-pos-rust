use crate::auth::{Permission, Session};
use anyhow::{ensure, Result};
use rust_decimal::Decimal;
use sqlx::PgPool;

#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    Text(String),
    Money(Decimal),
    Unknown,
}
impl Cell {
    pub fn plain(&self) -> String {
        match self {
            Self::Text(s) => s.clone(),
            Self::Money(n) => n.round_dp(2).to_string(),
            Self::Unknown => "Unknown".into(),
        }
    }
}
impl From<&str> for Cell {
    fn from(s: &str) -> Self {
        Self::Text(s.into())
    }
}
impl From<String> for Cell {
    fn from(s: String) -> Self {
        Self::Text(s)
    }
}
impl From<Decimal> for Cell {
    fn from(n: Decimal) -> Self {
        Self::Money(n)
    }
}
impl From<Option<Decimal>> for Cell {
    fn from(n: Option<Decimal>) -> Self {
        n.map(Self::Money).unwrap_or(Self::Unknown)
    }
}

#[derive(Clone, Default)]
pub struct Table {
    pub title: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<Cell>>,
}
impl Table {
    fn new(title: &str, headers: &[&str]) -> Self {
        Self {
            title: title.into(),
            headers: headers.iter().map(|s| s.to_string()).collect(),
            rows: vec![],
        }
    }
}
#[derive(Clone, Default)]
pub struct Report {
    pub from: String,
    pub to: String,
    pub as_of: String,
    pub tables: Vec<Table>,
    pub warnings: Vec<String>,
}

#[derive(sqlx::FromRow)]
struct Sale {
    invoice: String,
    day: String,
    month: String,
    customer: String,
    method: String,
    status: String,
    total: Decimal,
    discount: Decimal,
    received: Decimal,
    cost: Option<Decimal>,
    partial: i64,
}
#[derive(sqlx::FromRow)]
struct Expense {
    day: String,
    month: String,
    reference: String,
    category: String,
    description: String,
    method: String,
    amount: Decimal,
}
#[derive(sqlx::FromRow)]
struct Balance {
    name: String,
    balance: Decimal,
    invoices: Decimal,
    overdue: Decimal,
}

pub async fn load(pool: &PgPool, actor: &Session, from: &str, to: &str) -> Result<Report> {
    let (start, end) = crate::sale_summary::dates(from, to)?;
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout='20s'")
        .execute(&mut *tx)
        .await?;
    actor.authorize(&mut tx, Permission::Manage).await?;
    let as_of: String =
        sqlx::query_scalar("SELECT to_char(CURRENT_TIMESTAMP,'YYYY-MM-DD HH24:MI:SS TZ')")
            .fetch_one(&mut *tx)
            .await?;
    let sales: Vec<Sale> = sqlx::query_as(include_str!("report_sales.sql"))
        .bind(start.and_hms_opt(0, 0, 0).unwrap())
        .bind(end.and_hms_opt(0, 0, 0).unwrap())
        .fetch_all(&mut *tx)
        .await?;
    // A partial refund has no consistently recorded refund amount/date across Main versions.
    // Refuse an apparently precise report instead of guessing discount/refund allocation.
    ensure!(!sales.iter().any(|r| r.status == "completed" && r.partial > 0),
        "This period contains partial refunds. Reconcile the refund amounts with Main POS before using these reports.");
    let expenses: Vec<Expense> = sqlx::query_as("SELECT COALESCE(expense_date,'') AS day,left(expense_date,7) AS month,COALESCE(expense_no,'') AS reference,COALESCE(category,'Uncategorized') AS category,COALESCE(description,'') AS description,COALESCE(payment_method,'') AS method,COALESCE(amount,0)::numeric AS amount FROM expenses WHERE left(expense_date,10)>=$1 AND left(expense_date,10)<$2 ORDER BY expense_date,id")
        .bind(start.to_string()).bind(end.to_string()).fetch_all(&mut *tx).await?;
    let collections: Decimal = sqlx::query_scalar("SELECT COALESCE(SUM(amount::numeric),0) FROM credit_payments WHERE left(payment_date,10)>=$1 AND left(payment_date,10)<$2")
        .bind(start.to_string()).bind(end.to_string()).fetch_one(&mut *tx).await?;
    let ledger: (Decimal,Decimal) = sqlx::query_as("SELECT COALESCE(SUM(amount::numeric) FILTER(WHERE payment_type='Purchase'),0),COALESCE(SUM(amount::numeric) FILTER(WHERE payment_type<>'Purchase'),0) FROM supplier_payments WHERE left(payment_date,10)>=$1 AND left(payment_date,10)<$2")
        .bind(start.to_string()).bind(end.to_string()).fetch_one(&mut *tx).await?;
    let customers: Vec<Balance> = sqlx::query_as("SELECT COALESCE(c.name,'Customer') || ' #' || c.id::text AS name,COALESCE(c.current_balance,0)::numeric AS balance,COALESCE(i.invoices,0)::numeric AS invoices,COALESCE(i.overdue,0)::numeric AS overdue FROM customers c LEFT JOIN (SELECT customer_id,SUM(balance_amount::numeric) AS invoices,COALESCE(SUM(balance_amount::numeric) FILTER(WHERE NULLIF(due_date,'')<to_char(CURRENT_DATE,'YYYY-MM-DD')),0) AS overdue FROM credit_sales WHERE balance_amount>0 AND COALESCE(status,'pending')<>'refunded' GROUP BY customer_id) i ON i.customer_id=c.id WHERE COALESCE(c.current_balance,0)<>0 OR COALESCE(i.invoices,0)<>0 ORDER BY c.current_balance DESC,c.id")
        .fetch_all(&mut *tx).await?;
    let suppliers: Vec<(String,Decimal)> = sqlx::query_as("SELECT COALESCE(s.name,'Supplier') || ' #' || s.id::text,SUM(CASE WHEN p.payment_type='Purchase' THEN p.amount::numeric WHEN p.payment_type<>'Purchase' THEN -p.amount::numeric ELSE 0 END) AS balance FROM suppliers s JOIN supplier_payments p ON p.supplier_id=s.id GROUP BY s.id,s.name HAVING SUM(CASE WHEN p.payment_type='Purchase' THEN p.amount::numeric WHEN p.payment_type<>'Purchase' THEN -p.amount::numeric ELSE 0 END)<>0 ORDER BY balance DESC,s.id")
        .fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(assemble(
        from,
        to,
        as_of,
        sales,
        expenses,
        collections,
        ledger,
        customers,
        suppliers,
    ))
}

fn assemble(
    from: &str,
    to: &str,
    as_of: String,
    sales: Vec<Sale>,
    expenses: Vec<Expense>,
    collections: Decimal,
    ledger: (Decimal, Decimal),
    customers: Vec<Balance>,
    suppliers: Vec<(String, Decimal)>,
) -> Report {
    let mut sales_table = Table::new(
        "Sales",
        &[
            "Date",
            "Invoice",
            "Customer",
            "Payment",
            "Status",
            "Recorded total",
            "Discount",
            "Received at checkout",
            "Historical cost",
        ],
    );
    let mut expense_table = Table::new(
        "Expenses",
        &[
            "Date",
            "Reference",
            "Category",
            "Description",
            "Payment",
            "Amount",
        ],
    );
    let mut profit = Table::new(
        "Profit & Loss",
        &[
            "Month",
            "Completed sales",
            "Historical cost",
            "Gross profit",
            "Expenses",
            "Net profit",
            "Cost status",
        ],
    );
    let mut finance = Table::new("Financial Summary", &["Metric", "Amount"]);
    let mut receivables = Table::new(
        "Receivables",
        &[
            "Customer",
            "Current balance",
            "Outstanding invoices",
            "Overdue invoices",
            "Balance difference",
        ],
    );
    let mut payables = Table::new(
        "Payables",
        &["Supplier", "Payable (ledger)", "Advance (ledger)"],
    );
    let mut months = std::collections::BTreeMap::<String, (Decimal, Decimal, Decimal, bool)>::new();
    let mut total = Decimal::ZERO;
    let mut received = Decimal::ZERO;
    let mut credit = Decimal::ZERO;
    let mut discounts = Decimal::ZERO;
    let mut refunded = Decimal::ZERO;
    let mut unknown = 0usize;
    for row in sales {
        if row.status == "completed" {
            total += row.total;
            received += row.received;
            discounts += row.discount;
            if row.method.trim().eq_ignore_ascii_case("credit") {
                credit += row.total;
            }
            let m = months.entry(row.month.clone()).or_default();
            m.0 += row.total;
            if let Some(cost) = row.cost {
                m.1 += cost;
            } else {
                m.3 = true;
                unknown += 1;
            }
        } else {
            refunded += row.total;
        }
        sales_table.rows.push(vec![
            row.day.into(),
            row.invoice.into(),
            row.customer.into(),
            row.method.into(),
            row.status.into(),
            row.total.into(),
            row.discount.into(),
            row.received.into(),
            row.cost.into(),
        ]);
    }
    let mut expense_total = Decimal::ZERO;
    for row in expenses {
        expense_total += row.amount;
        months.entry(row.month.clone()).or_default().2 += row.amount;
        expense_table.rows.push(vec![
            row.day.into(),
            row.reference.into(),
            row.category.into(),
            row.description.into(),
            row.method.into(),
            row.amount.into(),
        ]);
    }
    for (month, (sales, cost, expense, missing)) in &months {
        profit.rows.push(vec![
            month.clone().into(),
            (*sales).into(),
            (!missing).then_some(*cost).into(),
            (!missing).then_some(sales - cost).into(),
            (*expense).into(),
            (!missing).then_some(sales - cost - expense).into(),
            if *missing {
                "Incomplete".into()
            } else {
                "Recorded".into()
            },
        ]);
    }
    let cost: Decimal = months.values().map(|m| m.1).sum();
    profit.rows.push(vec![
        "TOTAL".into(),
        total.into(),
        (unknown == 0).then_some(cost).into(),
        (unknown == 0).then_some(total - cost).into(),
        expense_total.into(),
        (unknown == 0)
            .then_some(total - cost - expense_total)
            .into(),
        if unknown > 0 {
            "Incomplete".into()
        } else {
            "Recorded".into()
        },
    ]);
    for (label, amount) in [
        ("Completed sales", total),
        ("Discounts (already included in sale total)", discounts),
        ("Credit-labelled sales", credit),
        ("Received at checkout (completed sales)", received),
        ("Credit collections", collections),
        ("Recorded expenses", expense_total),
        ("Refunded sales (original sale date)", refunded),
        ("Supplier purchases (ledger)", ledger.0),
        ("Supplier credits (payments / adjustments)", ledger.1),
    ] {
        finance.rows.push(vec![label.into(), amount.into()]);
    }
    for row in customers {
        receivables.rows.push(vec![
            row.name.into(),
            row.balance.into(),
            row.invoices.into(),
            row.overdue.into(),
            (row.balance - row.invoices).into(),
        ]);
    }
    for (name, balance) in suppliers {
        payables.rows.push(vec![
            name.into(),
            balance.max(Decimal::ZERO).into(),
            (-balance).max(Decimal::ZERO).into(),
        ]);
    }
    for table in [&mut receivables, &mut payables] {
        if !table.rows.is_empty() {
            let mut totals = vec!["TOTAL".into()];
            for c in 1..table.headers.len() {
                totals.push(Cell::Money(
                    table
                        .rows
                        .iter()
                        .filter_map(|r| {
                            if let Cell::Money(n) = r[c] {
                                Some(n)
                            } else {
                                None
                            }
                        })
                        .sum(),
                ));
            }
            table.rows.push(totals);
        }
    }
    let mut warnings=vec!["Refunds are grouped by original sale date; these are not refund cash-flow totals.".into(),"Receivables and payables are current balances at the snapshot time, not balances at the selected end date.".into(),"Supplier credits can include adjustments. Expenses and supplier ledger entries may overlap; do not add them as cash outflow.".into()];
    if unknown > 0 {
        warnings.push(format!("{unknown} completed sale(s) lack historical item costs. Affected profit totals are Unknown; current product costs were not substituted."));
    }
    Report {
        from: from.into(),
        to: to.into(),
        as_of,
        tables: vec![
            sales_table,
            expense_table,
            profit,
            finance,
            receivables,
            payables,
        ],
        warnings,
    }
}
