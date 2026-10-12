use crate::storage as sqlx;
use crate::auth::{Permission, Session};
use anyhow::{ensure, Context, Result};
use sqlx::PgPool;

#[derive(Clone, Default)]
pub struct Transfer {
    pub product_id: i32,
    pub variant_id: Option<i32>,
    pub from: String,
    pub to: String,
    pub quantity: i32,
    pub reason: String,
    pub notes: String,
    pub reference: String,
    pub expected_available: f64,
}
impl Transfer {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.product_id > 0 && self.variant_id.is_none_or(|id| id > 0),
            "Select a product and valid variant"
        );
        ensure!(self.quantity > 0, "Quantity must be greater than zero");
        ensure!(
            !self.from.trim().is_empty() && !self.to.trim().is_empty(),
            "Select From and To locations"
        );
        ensure!(
            self.from.trim() != self.to.trim(),
            "From and To locations cannot be the same"
        );
        ensure!(!self.reason.trim().is_empty(), "Reason is required");
        ensure!(
            !self.reference.trim().is_empty() && self.reference.len() <= 100,
            "Transfer reference is required"
        );
        ensure!(
            self.expected_available.is_finite() && self.expected_available >= 0.0,
            "Refresh available stock"
        );
        ensure!(
            self.from.len() <= 200
                && self.to.len() <= 200
                && self.reason.len() <= 1000
                && self.notes.len() <= 4000,
            "Transfer text is too long"
        );
        Ok(())
    }
}
pub async fn save(pool: &PgPool, actor: &Session, r: &Transfer) -> Result<()> {
    r.validate()?;
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL lock_timeout='5s'")
        .execute(&mut *tx)
        .await?;
    actor.authorize(&mut tx, Permission::Manage).await?;
    sqlx::query("LOCK TABLE locations IN SHARE MODE")
        .execute(&mut *tx)
        .await?;
    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM locations WHERE name=$1) AND (EXISTS(SELECT 1 FROM locations WHERE name=$2) OR ($2='Legacy / unassigned' AND $3::integer IS NOT NULL) OR EXISTS(SELECT 1 FROM product_locations WHERE product_id=$4 AND location=$2) OR EXISTS(SELECT 1 FROM variant_stock_batches WHERE product_id=$4 AND variant_id=$3 AND location=$2))")
  .bind(r.to.trim()).bind(r.from.trim()).bind(r.variant_id).bind(r.product_id).fetch_one(&mut *tx).await?;
    ensure!(valid, "Location no longer exists. Refresh locations");
    let (master,mode):(f64,String)=sqlx::query_as("SELECT COALESCE(stock,0)::float8,COALESCE(sold_by,'Each') FROM products WHERE id=$1 FOR UPDATE").bind(r.product_id).fetch_optional(&mut *tx).await?.context("Product no longer exists")?;
    ensure!(master.is_finite() && master >= 0.0, "Invalid product stock");
    ensure!(
        !matches!(mode.to_lowercase().as_str(), "service" | "restaurant"),
        "This product does not track stock"
    );
    ensure!(
        mode.eq_ignore_ascii_case("variants") == r.variant_id.is_some(),
        "Select the correct variant"
    );
    let duplicate: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM stock_movements WHERE reference=$1)")
            .bind(&r.reference)
            .fetch_one(&mut *tx)
            .await?;
    ensure!(
        !duplicate,
        "Transfer already recorded. Refresh before creating another transfer"
    );
    let variant_clause = r
        .variant_id
        .map(|id| format!(" AND variant_id={id}"))
        .unwrap_or_default();
    let table = if r.variant_id.is_some() {
        "variant_stock_batches"
    } else {
        "product_locations"
    };
    let old = if let Some(id) = r.variant_id {
        let stock:i32=sqlx::query_scalar("SELECT stock FROM product_variants WHERE product_id=$1 AND id=$2 AND COALESCE(active,1)=1 FOR UPDATE").bind(r.product_id).bind(id).fetch_optional(&mut *tx).await?.context("Variant unavailable")?;
        f64::from(stock)
    } else {
        master
    };
    let invalid:bool=sqlx::query_scalar(&format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE product_id=$1{variant_clause} AND (quantity IS NULL OR quantity<0 OR quantity::text IN ('NaN','Infinity','-Infinity')))")).bind(r.product_id).fetch_one(&mut *tx).await?;
    ensure!(!invalid && old >= 0.0, "Invalid location stock");
    let tracked: f64 = sqlx::query_scalar(&format!(
        "SELECT COALESCE(SUM(quantity),0)::float8 FROM {table} WHERE product_id=$1{variant_clause}"
    ))
    .bind(r.product_id)
    .fetch_one(&mut *tx)
    .await?;
    if let Some(id) = r.variant_id {
        ensure!(
            tracked <= old && tracked.fract() == 0.0,
            "Variant batches exceed stock. Reconcile in Main POS"
        );
        if tracked < old {
            sqlx::query("INSERT INTO variant_stock_batches(product_id,variant_id,location,batch_no,expire_date,quantity,expiry_unknown) VALUES($1,$2,'Legacy / unassigned','LEGACY-OPENING','',$3,1) ON CONFLICT(product_id,variant_id,location,batch_no,expire_date) DO UPDATE SET quantity=variant_stock_batches.quantity+EXCLUDED.quantity,expiry_unknown=1").bind(r.product_id).bind(id).bind((old-tracked) as i32).execute(&mut *tx).await?;
        }
    } else {
        ensure!(
            (tracked - old).abs() < 0.000001,
            "Location stock differs from total. Reconcile in Main POS first"
        );
    }
    let unknown = if r.variant_id.is_some() {
        "COALESCE(expiry_unknown,0)"
    } else {
        "0::integer"
    };
    let allocation_order = if r.variant_id.is_some() {
        "COALESCE(NULLIF(expire_date,''),'9999-12-31'),COALESCE(batch_no,''),ctid"
    } else {
        "id"
    };
    let rows:Vec<(String,f64,String,String,i32)>=sqlx::query_as(&format!("SELECT ctid::text,quantity::float8,COALESCE(batch_no,''),COALESCE(expire_date,''),{unknown} FROM {table} WHERE product_id=$1{variant_clause} AND location=$2 AND quantity>0 ORDER BY {allocation_order} FOR UPDATE")).bind(r.product_id).bind(r.from.trim()).fetch_all(&mut *tx).await?;
    let available = rows.iter().map(|row| row.1).sum::<f64>();
    ensure!(
        available == r.expected_available,
        "Source stock changed. Refresh before transferring"
    );
    ensure!(
        available >= f64::from(r.quantity),
        "Insufficient stock at source location"
    );
    let mut remaining = f64::from(r.quantity);
    let mut changes = vec![];
    for (id, amount, batch, expiry, unknown) in rows {
        let take = remaining.min(amount);
        if take <= 0.0 {
            break;
        }
        if r.variant_id.is_some() {
            sqlx::query("UPDATE variant_stock_batches SET quantity=quantity-$1 WHERE ctid=$2::tid")
                .bind(take as i32)
                .bind(id)
                .execute(&mut *tx)
                .await?;
        } else {
            sqlx::query("UPDATE product_locations SET quantity=quantity-$1,last_updated=CURRENT_TIMESTAMP WHERE ctid=$2::tid").bind(take).bind(id).execute(&mut *tx).await?;
        }
        let target:Option<String>=sqlx::query_scalar(&format!("SELECT ctid::text FROM {table} WHERE product_id=$1{variant_clause} AND location=$2 AND COALESCE(batch_no,'')=$3 AND COALESCE(expire_date,'')=$4 ORDER BY ctid LIMIT 1 FOR UPDATE")).bind(r.product_id).bind(r.to.trim()).bind(&batch).bind(&expiry).fetch_optional(&mut *tx).await?;
        if let Some(target) = target {
            if r.variant_id.is_some() {
                sqlx::query("UPDATE variant_stock_batches SET quantity=quantity+$1,expiry_unknown=GREATEST(COALESCE(expiry_unknown,0),$3) WHERE ctid=$2::tid").bind(take as i32).bind(target).bind(unknown).execute(&mut *tx).await?;
            } else {
                sqlx::query("UPDATE product_locations SET quantity=quantity+$1,last_updated=CURRENT_TIMESTAMP WHERE ctid=$2::tid").bind(take).bind(target).execute(&mut *tx).await?;
            }
        } else if let Some(vid) = r.variant_id {
            sqlx::query("INSERT INTO variant_stock_batches(product_id,variant_id,location,batch_no,expire_date,quantity,expiry_unknown) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(r.product_id).bind(vid).bind(r.to.trim()).bind(&batch).bind(&expiry).bind(take as i32).bind(unknown).execute(&mut *tx).await?;
        } else {
            sqlx::query("INSERT INTO product_locations(product_id,location,batch_no,expire_date,quantity) VALUES($1,$2,$3,$4,$5)").bind(r.product_id).bind(r.to.trim()).bind(&batch).bind(&expiry).bind(take).execute(&mut *tx).await?;
        }
        changes.push((batch, expiry, take, unknown));
        remaining -= take;
    }
    ensure!(remaining.abs() < 0.000001, "Transfer allocation incomplete");
    let destinations = if r.variant_id.is_some() {
        vec![(
            "transfer",
            format!("{} -> {}", r.from.trim(), r.to.trim()),
            r.reason.clone(),
        )]
    } else {
        vec![
            (
                "out",
                r.from.trim().into(),
                format!("Transfer to {}: {}", r.to.trim(), r.reason),
            ),
            (
                "in",
                r.to.trim().into(),
                format!("Transfer from {}: {}", r.from.trim(), r.reason),
            ),
        ]
    };
    for (kind, location, reason) in destinations {
        let movement:i32=sqlx::query_scalar("INSERT INTO stock_movements(product_id,variant_id,type,quantity,old_stock,new_stock,reason,reference,created_by,location,notes) VALUES($1,$2,$3,$4,$5,$5,$6,$7,$8,$9,$10) RETURNING id").bind(r.product_id).bind(r.variant_id).bind(kind).bind(f64::from(r.quantity)).bind(old).bind(reason).bind(&r.reference).bind(actor.username()).bind(location).bind(&r.notes).fetch_one(&mut *tx).await?;
        if let Some(vid) = r.variant_id {
            for (batch, expiry, take, unknown) in &changes {
                for (location, sign) in [(r.from.trim(), -1), (r.to.trim(), 1)] {
                    sqlx::query("INSERT INTO variant_batch_changes(movement_id,product_id,variant_id,location,batch_no,expire_date,delta,expiry_unknown) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(movement).bind(r.product_id).bind(vid).bind(location).bind(batch).bind(expiry).bind((*take as i32)*sign).bind(unknown).execute(&mut *tx).await?;
                }
            }
        }
    }
    actor.authorize(&mut tx, Permission::Manage).await?;
    crate::activity::record(
        &mut tx,
        actor,
        "rust.locations.transfer",
        &format!("product_id={}; reference={}", r.product_id, r.reference),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_transfer() {
        let mut r = Transfer {
            product_id: 1,
            from: "Shop".into(),
            to: "Warehouse".into(),
            quantity: 10,
            reason: "Move".into(),
            reference: "TRF-test".into(),
            expected_available: 10.0,
            ..Default::default()
        };
        assert!(r.validate().is_ok());
        r.to = "Shop".into();
        assert!(r.validate().is_err());
        r.to = "Warehouse".into();
        r.quantity = 0;
        assert!(r.validate().is_err());
        r.quantity = 1;
        r.expected_available = f64::NAN;
        assert!(r.validate().is_err());
    }
}

#[cfg(test)]
mod database_tests {
    use super::*;
    #[tokio::test]
    #[ignore = "Requires KAY_TRANSFER_TEST_CONFIG; uses only session-local temporary tables"]
    async fn transfer_transactions_on_temporary_tables() -> Result<()> {
        let path = std::env::var("KAY_TRANSFER_TEST_CONFIG")?;
        let bytes = std::fs::read_to_string(path)?;
        let config: serde_json::Value = serde_json::from_str(bytes.trim_start_matches('\u{feff}'))?;
        let options = ::sqlx::postgres::PgConnectOptions::new()
            .host(config["host"].as_str().context("host")?)
            .port(config["port"].as_u64().unwrap_or(5432) as u16)
            .database(config["database"].as_str().context("database")?)
            .username(config["username"].as_str().context("username")?)
            .password(config["password"].as_str().context("password")?);
        let pool:PgPool = ::sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .min_connections(1)
            .max_lifetime(None)
            .idle_timeout(None)
            .connect_with(options)
            .await?.into();
        for sql in [
   "CREATE TEMP TABLE users(id integer PRIMARY KEY,username text,role text,password_hash text,salt text,is_active integer)",
   "CREATE TEMP TABLE locations(id integer,name text)",
   "CREATE TEMP TABLE products(id integer PRIMARY KEY,stock float8,sold_by text)",
   "CREATE TEMP TABLE product_variants(id integer PRIMARY KEY,product_id integer,stock integer,active integer)",
   "CREATE TEMP TABLE product_locations(id serial,product_id integer,quantity float8,location text,batch_no text,expire_date text,last_updated timestamp DEFAULT CURRENT_TIMESTAMP)",
   "CREATE TEMP TABLE variant_stock_batches(product_id integer,variant_id integer,location text,batch_no text,expire_date text,quantity integer,expiry_unknown integer,UNIQUE(product_id,variant_id,location,batch_no,expire_date))",
   "CREATE TEMP TABLE stock_movements(id serial,product_id integer,variant_id integer,type text,quantity float8,old_stock float8,new_stock float8,reason text,reference text,created_by text,location text,notes text)",
   "CREATE TEMP TABLE variant_batch_changes(movement_id integer,product_id integer,variant_id integer,location text,batch_no text,expire_date text,delta integer,expiry_unknown integer)",
   "CREATE TEMP TABLE user_activity_log(user_id integer,username text,action text,details text,ip_address text,created_at timestamp)",
  ] {sqlx::query(sql).execute(&pool).await?;}
        let mut hash = [0u8; 32];
        pbkdf2::pbkdf2_hmac::<sha2::Sha256>(b"test", b"salt", 100000, &mut hash);
        sqlx::query("INSERT INTO users VALUES(1,'test-manager','Manager',$1,$2,1),(2,'test-cashier','Cashier',$1,$2,1)").bind(hex::encode(hash)).bind(hex::encode(b"salt")).execute(&pool).await?;
        for sql in ["INSERT INTO locations VALUES(1,'Shop'),(2,'Warehouse')","INSERT INTO products VALUES(1,10,'Each'),(2,10,'Variants')","INSERT INTO product_variants VALUES(20,2,10,1)","INSERT INTO product_locations(product_id,quantity,location,batch_no,expire_date) VALUES(1,8,'Shop','B1','2027-01-01'),(1,2,'Warehouse','B1','2027-01-01')","INSERT INTO variant_stock_batches VALUES(2,20,'Shop','VB1','2027-02-01',4,0)"] {sqlx::query(sql).execute(&pool).await?;}
        let actor = crate::auth::login(&pool, "test-manager", "test").await?;
        let mut r = Transfer {
            product_id: 1,
            from: "Shop".into(),
            to: "Warehouse".into(),
            quantity: 5,
            reason: "Move".into(),
            reference: "TRF-1".into(),
            expected_available: 8.0,
            ..Default::default()
        };
        save(&pool, &actor, &r).await?;
        let source:f64=sqlx::query_scalar("SELECT SUM(quantity)::float8 FROM product_locations WHERE product_id=1 AND location='Shop'").fetch_one(&pool).await?;
        assert_eq!(source, 3.0);
        let target:f64=sqlx::query_scalar("SELECT SUM(quantity)::float8 FROM product_locations WHERE product_id=1 AND location='Warehouse'").fetch_one(&pool).await?;
        assert_eq!(target, 7.0);
        let total: f64 = sqlx::query_scalar("SELECT stock FROM products WHERE id=1")
            .fetch_one(&pool)
            .await?;
        assert_eq!(total, 10.0);
        let movements: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM stock_movements WHERE reference='TRF-1' AND old_stock=new_stock",
        )
        .fetch_one(&pool)
        .await?;
        assert_eq!(movements, 2);
        assert!(save(&pool, &actor, &r).await.is_err());
        r.reference = "TRF-2".into();
        assert!(save(&pool, &actor, &r).await.is_err());
        r.expected_available = 3.0;
        r.quantity = 4;
        assert!(save(&pool, &actor, &r).await.is_err());
        let cashier = crate::auth::login(&pool, "test-cashier", "test").await?;
        r.quantity = 1;
        assert!(save(&pool, &cashier, &r).await.is_err());
        r.product_id = 2;
        r.variant_id = Some(20);
        r.reference = "TRF-V1".into();
        r.expected_available = 4.0;
        r.quantity = 2;
        save(&pool, &actor, &r).await?;
        let qty: i64 = sqlx::query_scalar(
            "SELECT SUM(quantity) FROM variant_stock_batches WHERE product_id=2 AND variant_id=20",
        )
        .fetch_one(&pool)
        .await?;
        assert_eq!(qty, 10);
        let changes: i64 =
            sqlx::query_scalar("SELECT SUM(delta)::bigint FROM variant_batch_changes")
                .fetch_one(&pool)
                .await?;
        assert_eq!(changes, 0);
        let expiry: String = sqlx::query_scalar(
            "SELECT expire_date FROM variant_stock_batches WHERE location='Warehouse'",
        )
        .fetch_one(&pool)
        .await?;
        assert_eq!(expiry, "2027-02-01");
        r.from = "Legacy / unassigned".into();
        r.expected_available = 6.0;
        r.quantity = 5;
        r.reference = "TRF-V2".into();
        save(&pool, &actor, &r).await?;
        let stock: i32 = sqlx::query_scalar("SELECT stock FROM product_variants WHERE id=20")
            .fetch_one(&pool)
            .await?;
        assert_eq!(stock, 10);
        // A failure after batch movement must roll back every allocation and movement record.
        sqlx::query("ALTER TABLE user_activity_log ADD CONSTRAINT reject_new_logs CHECK (action='never') NOT VALID").execute(&pool).await?;
        r.product_id = 1;
        r.variant_id = None;
        r.from = "Shop".into();
        r.expected_available = 3.0;
        r.quantity = 1;
        r.reference = "TRF-FAIL".into();
        assert!(save(&pool, &actor, &r).await.is_err());
        let source:f64=sqlx::query_scalar("SELECT SUM(quantity)::float8 FROM product_locations WHERE product_id=1 AND location='Shop'").fetch_one(&pool).await?;
        assert_eq!(source, 3.0);
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM stock_movements WHERE reference='TRF-FAIL'")
                .fetch_one(&pool)
                .await?;
        assert_eq!(count, 0);
        pool.close().await;
        Ok(())
    }
}
