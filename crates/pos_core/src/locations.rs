use crate::storage as sqlx;
use anyhow::{ensure, Result};
use sqlx::PgPool;

#[derive(Clone, Default, Debug, PartialEq, sqlx::FromRow)]
pub struct Location {
    pub id: i32,
    pub name: String,
    pub parent_id: Option<i32>,
}
#[derive(Clone, PartialEq, sqlx::FromRow)]
pub struct Stock {
    pub id: i32,
    pub name: String,
    pub sku: String,
    pub category: String,
    pub location: String,
    pub quantity: f64,
    pub variant_id:Option<i32>,
}
pub const DEFAULT_SALES_LOCATION_KEY: &str = "default_sales_location_id";

pub async fn default_sales_location(pool: &PgPool) -> Result<Option<i32>> {
    let settings = crate::db::list_settings(pool).await?;
    Ok(settings.iter().find(|s| s.key == DEFAULT_SALES_LOCATION_KEY)
        .and_then(|s| s.value.as_deref()).and_then(|value| value.parse().ok()))
}

pub async fn set_default_sales_location(pool: &PgPool, actor: &crate::auth::Session, id: i32) -> Result<()> {
    ensure_schema(pool).await?;
    let mut tx=pool.begin().await?;
    actor.authorize(&mut tx,crate::auth::Permission::Manage).await?;
    sqlx::query("LOCK TABLE locations,rust_location_hierarchy IN SHARE MODE").execute(&mut *tx).await?;
    let all:Vec<Location>=sqlx::query_as("SELECT l.id,l.name,h.parent_id FROM locations l LEFT JOIN rust_location_hierarchy h ON h.location_id=l.id").fetch_all(&mut *tx).await?;
    ensure!(all.iter().any(|l| l.id == id && l.parent_id.is_none()), "Select a parent location as the sales default");
    sqlx::query("INSERT INTO settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value")
        .bind(DEFAULT_SALES_LOCATION_KEY).bind(id.to_string()).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Normalize an existing selection to its top-level selling location.
pub fn sales_parent_location(all: &[Location], requested: Option<i32>, saved: Option<i32>) -> Option<i32> {
    let mut current=requested.and_then(|id|all.iter().find(|l|l.id==id));
    let mut seen=std::collections::BTreeSet::new();
    while let Some(location)=current {
        if !seen.insert(location.id) { break; }
        match location.parent_id {
            None => return Some(location.id),
            Some(parent) => current=all.iter().find(|l|l.id==parent),
        }
    }
    initial_sales_location(all,saved).filter(|id|all.iter().any(|l|l.id==*id&&l.parent_id.is_none()))
}

pub fn initial_sales_location(all: &[Location], saved: Option<i32>) -> Option<i32> {
    saved.filter(|id| all.iter().any(|l| l.id == *id && l.parent_id.is_none()))
        .or_else(|| all.iter().find(|l| l.name.trim().eq_ignore_ascii_case("Shop") && l.parent_id.is_none()).map(|l| l.id))
        .or_else(|| all.iter().find(|l| l.parent_id.is_none()).map(|l| l.id))
        .or_else(|| all.first().map(|l| l.id))
}

pub async fn list(pool: &PgPool) -> Result<(Vec<Location>, Vec<Stock>)> {
    let locations = list_names(pool).await?;
    let stock=sqlx::query_as(STOCK_BY_LOCATION_SQL).fetch_all(pool).await?;
    Ok((locations, stock))
}
pub async fn list_names(pool: &PgPool) -> Result<Vec<Location>> {
    ensure_schema(pool).await?;
    Ok(
        sqlx::query_as("SELECT l.id,l.name,h.parent_id FROM locations l LEFT JOIN rust_location_hierarchy h ON h.location_id=l.id ORDER BY LOWER(l.name),l.id")
            .fetch_all(pool)
            .await?,
    )
}
pub async fn save(pool: &PgPool, location: &Location, delete: bool) -> Result<()> {
    ensure_schema(pool).await?;
    let name = location.name.trim();
    ensure!(!name.is_empty(), "Location name is required");
    let mut tx = pool.begin().await?;
    sqlx::query("LOCK TABLE locations,product_locations,products,stock_movements IN SHARE ROW EXCLUSIVE MODE").execute(&mut *tx).await?;
    let variants: bool =
        sqlx::query_scalar("SELECT to_regclass('variant_stock_batches') IS NOT NULL")
            .fetch_one(&mut *tx)
            .await?;
    if variants {
        sqlx::query("LOCK TABLE variant_stock_batches IN SHARE ROW EXCLUSIVE MODE")
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("LOCK TABLE rust_location_hierarchy IN SHARE ROW EXCLUSIVE MODE").execute(&mut *tx).await?;
    if !delete {
        if let Some(parent)=location.parent_id {
            ensure!(parent!=location.id,"Location cannot be its own parent");
            let all:Vec<Location>=sqlx::query_as("SELECT l.id,l.name,h.parent_id FROM locations l LEFT JOIN rust_location_hierarchy h ON h.location_id=l.id").fetch_all(&mut *tx).await?;
            ensure!(all.iter().any(|l|l.id==parent),"Parent location no longer exists");
            ensure!(!descendant_ids(&all,location.id).contains(&parent),"Parent would create a location cycle");
        }
        let duplicate: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM locations WHERE LOWER(TRIM(name))=LOWER($1) AND id<>$2)",
        )
        .bind(name)
        .bind(location.id)
        .fetch_one(&mut *tx)
        .await?;
        ensure!(!duplicate, "Location already exists");
    }
    if location.id == 0 {
        ensure!(!delete, "Select a location");
        let id:i32=sqlx::query_scalar("INSERT INTO locations(name) VALUES($1) RETURNING id").bind(name).fetch_one(&mut *tx).await?;
        if let Some(parent)=location.parent_id {sqlx::query("INSERT INTO rust_location_hierarchy(location_id,parent_id) VALUES($1,$2)").bind(id).bind(parent).execute(&mut *tx).await?;}
    } else {
        let old: String = sqlx::query_scalar("SELECT name FROM locations WHERE id=$1")
            .bind(location.id)
            .fetch_one(&mut *tx)
            .await?;
        if delete {
            let children:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM rust_location_hierarchy WHERE parent_id=$1)").bind(location.id).fetch_one(&mut *tx).await?;
            ensure!(!children,"Move child locations to another parent before deleting this location");
            let used: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM product_locations WHERE LOWER(TRIM(location))=LOWER(TRIM($1)))").bind(&old).fetch_one(&mut *tx).await?;
            ensure!(
                !used,
                "Move or remove all stock records from this location first"
            );
            if variants {
                let used: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM variant_stock_batches WHERE LOWER(TRIM(location))=LOWER(TRIM($1)))").bind(&old).fetch_one(&mut *tx).await?;
                ensure!(!used, "Move or remove variant stock records first");
            }
        }
        let replacement = if delete { "" } else { name };
        for (table, column) in [
            ("products", "warehouse"),
            ("stock_movements", "location"),
            ("product_locations", "location"),
        ] {
            sqlx::query(&format!(
                "UPDATE {table} SET {column}=$1 WHERE LOWER(TRIM({column}))=LOWER(TRIM($2))"
            ))
            .bind(replacement)
            .bind(&old)
            .execute(&mut *tx)
            .await?;
        }
        if variants && !delete {
            sqlx::query("UPDATE variant_stock_batches SET location=$1 WHERE LOWER(TRIM(location))=LOWER(TRIM($2))").bind(name).bind(&old).execute(&mut *tx).await?;
        }
        sqlx::query("DELETE FROM rust_location_hierarchy WHERE location_id=$1").bind(location.id).execute(&mut *tx).await?;
        if !delete {if let Some(parent)=location.parent_id {sqlx::query("INSERT INTO rust_location_hierarchy(location_id,parent_id) VALUES($1,$2)").bind(location.id).bind(parent).execute(&mut *tx).await?;}}
        if !delete {sqlx::query("UPDATE sale_items SET location=$1 WHERE LOWER(TRIM(location))=LOWER(TRIM($2))").bind(name).bind(&old).execute(&mut *tx).await?;}
        if delete {
            sqlx::query("DELETE FROM locations WHERE id=$1")
                .bind(location.id)
                .execute(&mut *tx)
                .await?;
        } else {
            sqlx::query("UPDATE locations SET name=$1 WHERE id=$2")
                .bind(name)
                .bind(location.id)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

const STOCK_BY_LOCATION_SQL:&str=r"WITH stock AS (
 SELECT p.id,p.name,COALESCE(p.sku,'') AS sku,COALESCE(p.category,'') AS category,
 COALESCE(pl.location,'') AS location,COALESCE(SUM(pl.quantity),0)::float8 AS quantity,NULL::integer AS variant_id
 FROM product_locations pl JOIN products p ON p.id=pl.product_id
 WHERE lower(COALESCE(p.sold_by,'Each'))<>'variants' GROUP BY p.id,p.name,p.sku,p.category,pl.location
 UNION ALL
 SELECT p.id,p.name||' / '||concat_ws(' / ',NULLIF(v.size,''),NULLIF(v.color,''),NULLIF(v.sku,'')),
 COALESCE(v.sku,p.sku,''),COALESCE(p.category,''),b.location,SUM(b.quantity)::float8,v.id
 FROM variant_stock_batches b JOIN products p ON p.id=b.product_id JOIN product_variants v ON v.id=b.variant_id AND v.product_id=p.id
 GROUP BY p.id,p.name,p.sku,p.category,v.id,v.size,v.color,v.sku,b.location
 UNION ALL
 SELECT p.id,p.name||' / '||concat_ws(' / ',NULLIF(v.size,''),NULLIF(v.color,''),NULLIF(v.sku,'')),
 COALESCE(v.sku,p.sku,''),COALESCE(p.category,''),'Legacy / unassigned',
 (v.stock-COALESCE((SELECT SUM(b.quantity) FROM variant_stock_batches b WHERE b.product_id=p.id AND b.variant_id=v.id),0))::float8,v.id
 FROM product_variants v JOIN products p ON p.id=v.product_id
 WHERE lower(COALESCE(p.sold_by,''))='variants' AND v.stock>COALESCE((SELECT SUM(b.quantity) FROM variant_stock_batches b WHERE b.product_id=p.id AND b.variant_id=v.id),0)
) SELECT id,name,sku,category,location,SUM(quantity)::float8 AS quantity,variant_id FROM stock
 GROUP BY id,name,sku,category,location,variant_id
 HAVING lower(trim(location))<>'legacy / unassigned' OR SUM(quantity)<>0
 ORDER BY LOWER(name),location,id,variant_id";


pub const HIERARCHY_SCHEMA:&str=include_str!("../../../migrations/location-hierarchy.sql");
pub async fn ensure_schema(pool:&PgPool)->Result<()> {
    let exists:bool=sqlx::query_scalar("SELECT to_regclass('rust_location_hierarchy') IS NOT NULL").fetch_one(pool).await?;
    if exists{return Ok(());}
    let mut tx=pool.begin().await?;
    sqlx::query("LOCK TABLE locations IN SHARE ROW EXCLUSIVE MODE").execute(&mut *tx).await?;
    for statement in HIERARCHY_SCHEMA.trim_start_matches('\u{feff}').split(';').filter(|s|!s.trim().is_empty()) {sqlx::query(statement).execute(&mut *tx).await?;}
    tx.commit().await?;Ok(())
}
pub fn descendant_ids(all:&[Location],root:i32)->std::collections::BTreeSet<i32> {
    let mut ids=std::collections::BTreeSet::from([root]);
    loop {let before=ids.len();for l in all {if l.parent_id.is_some_and(|id|ids.contains(&id)){ids.insert(l.id);}}if before==ids.len(){break;}}
    ids
}
pub fn scope_names(all:&[Location],root:i32)->Result<Vec<String>> {
    ensure!(all.iter().any(|l|l.id==root),"Selected sales location no longer exists");
    let ids=descendant_ids(all,root);
    Ok(all.iter().filter(|l|ids.contains(&l.id)).map(|l|l.name.trim().to_lowercase()).collect())
}
pub async fn sale_scope(conn:&mut sqlx::PgConnection,root:i32)->Result<Vec<String>> {
    sqlx::query("LOCK TABLE locations,rust_location_hierarchy IN SHARE MODE").execute(&mut *conn).await?;
    let all:Vec<Location>=sqlx::query_as("SELECT l.id,l.name,h.parent_id FROM locations l LEFT JOIN rust_location_hierarchy h ON h.location_id=l.id").fetch_all(&mut *conn).await?;
    scope_names(&all,root)
}
/// Project catalog stock into one location tree without changing stored totals.
pub fn apply_sales_scope(products:&mut [crate::Product],stock:&[Stock],names:&[String]) {
    for p in products {
        let mode=p.sold_by.as_deref().unwrap_or("Each").to_lowercase();
        if mode=="service"||mode=="restaurant" {continue;}
        if !p.variants.is_empty() {for v in &mut p.variants {v.stock=stock.iter().filter(|s|s.id==p.id&&s.variant_id==Some(v.variant_id)&&names.contains(&s.location.trim().to_lowercase())).map(|s|s.quantity.max(0.0)).sum();}p.stock=p.variants.iter().map(|v|v.stock).sum();}
        else {p.stock=stock.iter().filter(|s|s.id==p.id&&s.variant_id.is_none()&&names.contains(&s.location.trim().to_lowercase())).map(|s|s.quantity.max(0.0)).sum();}
    }
}


/// Parent-first display order, with each subtree kept together.
pub fn tree_order(all: &[Location]) -> Vec<(&Location, usize)> {
    fn visit<'a>(all: &'a [Location], location: &'a Location, depth: usize,
        seen: &mut std::collections::BTreeSet<i32>, rows: &mut Vec<(&'a Location, usize)>) {
        if !seen.insert(location.id) { return; }
        rows.push((location, depth));
        for child in all.iter().filter(|child| child.parent_id == Some(location.id)) {
            visit(all, child, depth + 1, seen, rows);
        }
    }
    let mut rows = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for root in all.iter().filter(|l| l.parent_id.map(|id| !all.iter().any(|p| p.id == id)).unwrap_or(true)) {
        visit(all, root, 0, &mut seen, &mut rows);
    }
    // Retain malformed legacy entries without looping or hiding locations.
    for location in all { visit(all, location, 0, &mut seen, &mut rows); }
    rows
}

pub fn display_path(all:&[Location],id:i32)->String {
    let mut parts=Vec::new();let mut current=Some(id);let mut seen=std::collections::BTreeSet::new();
    while let Some(id)=current {if !seen.insert(id){break;}if let Some(l)=all.iter().find(|l|l.id==id){parts.push(l.name.clone());current=l.parent_id;}else{break;}}
    parts.reverse();parts.join(" / ")
}


#[cfg(test)]
mod tree_tests {
    use super::*;
    #[test]
    fn sales_default_uses_valid_parent_and_falls_back_safely() {
        let all=vec![Location{id:1,name:"Shop".into(),parent_id:Some(2)},Location{id:2,name:"Main".into(),parent_id:None},Location{id:3,name:"Warehouse".into(),parent_id:None}];
        assert_eq!(initial_sales_location(&all,Some(3)),Some(3));
        assert_eq!(initial_sales_location(&all,Some(1)),Some(2));
        assert_eq!(initial_sales_location(&all,Some(99)),Some(2));
        assert_eq!(initial_sales_location(&[],None),None);
        assert_eq!(sales_parent_location(&all,Some(3),Some(2)),Some(3));
        assert_eq!(sales_parent_location(&all,Some(1),Some(3)),Some(2));
        assert_eq!(sales_parent_location(&all,None,Some(3)),Some(3));
        assert_eq!(sales_parent_location(&all,Some(99),Some(3)),Some(3));

    }
    #[test]
    fn display_groups_children_below_their_parent() {
        let all = vec![
            Location { id:3, name:"Child".into(), parent_id:Some(2) },
            Location { id:4, name:"Grandchild".into(), parent_id:Some(3) },
            Location { id:1, name:"Other".into(), parent_id:None },
            Location { id:2, name:"Parent".into(), parent_id:None },
            Location { id:5, name:"Orphan".into(), parent_id:Some(99) },
        ];
        assert_eq!(tree_order(&all).iter().map(|(l,d)|(l.id,*d)).collect::<Vec<_>>(),
            vec![(1,0),(2,0),(3,1),(4,2),(5,0)]);
        let cycle = vec![Location{id:6,name:"A".into(),parent_id:Some(7)},Location{id:7,name:"B".into(),parent_id:Some(6)}];
        assert_eq!(tree_order(&cycle).len(),2);
    }
    #[test]
    fn scope_contains_all_descendants_but_not_parent_or_warehouse() {
        let all=vec![Location{id:1,name:"Shop".into(),parent_id:None},Location{id:2,name:"Room".into(),parent_id:Some(1)},Location{id:3,name:"Shelf".into(),parent_id:Some(2)},Location{id:4,name:"Warehouse".into(),parent_id:None}];
        assert_eq!(scope_names(&all,1).unwrap(),vec!["shop","room","shelf"]);
        assert_eq!(scope_names(&all,2).unwrap(),vec!["room","shelf"]);
        assert_eq!(scope_names(&all,3).unwrap(),vec!["shelf"]);
        assert!(scope_names(&all,999).is_err());
        assert_eq!(display_path(&all,3),"Shop / Room / Shelf");
    }
}
