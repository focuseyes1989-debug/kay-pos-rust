use anyhow::{ensure, Result};
use sqlx::PgPool;

#[derive(Clone, Debug, Default, PartialEq, sqlx::FromRow)]
pub struct CategoryRecord {
    pub id: i32,
    pub name: String,
    pub parent_id: Option<i32>,
    pub description: String,
    pub status: String,
    pub is_system: i32,
    pub products: i64,
    pub children: i64,
}

pub async fn list(pool: &PgPool) -> Result<Vec<CategoryRecord>> {
    Ok(sqlx::query_as("SELECT c.id,c.name,c.parent_id,COALESCE(c.description,'') AS description,COALESCE(c.status,'active') AS status,COALESCE(c.is_system,0) AS is_system,(SELECT COUNT(*) FROM products p WHERE p.category_id=c.id OR p.category=c.name) AS products,(SELECT COUNT(*) FROM categories ch WHERE ch.parent_id=c.id) AS children FROM categories c ORDER BY c.sort_order,c.name,c.id").fetch_all(pool).await?)
}

pub fn validate(record: &CategoryRecord, rows: &[CategoryRecord]) -> Result<()> {
    let name = record.name.trim();
    ensure!(
        !name.is_empty() && name.chars().count() <= 200,
        "Category name is required (maximum 200 characters)"
    );
    ensure!(
        !rows
            .iter()
            .any(|r| r.id != record.id && r.name.trim().to_lowercase() == name.to_lowercase()),
        "Category name already exists"
    );
    let mut seen = vec![record.id];
    let mut parent = record.parent_id;
    while let Some(id) = parent {
        ensure!(
            !seen.contains(&id),
            "Category hierarchy cannot contain a cycle"
        );
        seen.push(id);
        parent = rows
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| anyhow::anyhow!("Parent category no longer exists"))?
            .parent_id;
    }
    ensure!(
        ["active", "inactive"].contains(&record.status.as_str()),
        "Invalid category status"
    );
    Ok(())
}

pub async fn save(pool: &PgPool, record: &CategoryRecord) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("LOCK TABLE categories, products IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;
    let rows: Vec<CategoryRecord> = sqlx::query_as("SELECT id,name,parent_id,COALESCE(description,'') AS description,COALESCE(status,'active') AS status,COALESCE(is_system,0) AS is_system,0::bigint AS products,0::bigint AS children FROM categories").fetch_all(&mut *tx).await?;
    validate(record, &rows)?;
    if record.id == 0 {
        sqlx::query(
            "INSERT INTO categories(name,parent_id,description,status) VALUES($1,$2,$3,$4)",
        )
        .bind(record.name.trim())
        .bind(record.parent_id)
        .bind(&record.description)
        .bind(&record.status)
        .execute(&mut *tx)
        .await?;
    } else {
        let old = rows
            .iter()
            .find(|r| r.id == record.id)
            .ok_or_else(|| anyhow::anyhow!("Category no longer exists"))?;
        ensure!(old.is_system == 0, "System categories cannot be changed");
        sqlx::query("UPDATE categories SET name=$1,parent_id=$2,description=$3,status=$4,updated_at=CURRENT_TIMESTAMP WHERE id=$5").bind(record.name.trim()).bind(record.parent_id).bind(&record.description).bind(&record.status).bind(record.id).execute(&mut *tx).await?;
        sqlx::query("UPDATE products SET category=$1 WHERE category_id=$2 OR category=$3")
            .bind(record.name.trim())
            .bind(record.id)
            .bind(&old.name)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn delete(pool: &PgPool, id: i32) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("LOCK TABLE categories, products IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;
    let (name, system): (String, i32) =
        sqlx::query_as("SELECT name,COALESCE(is_system,0) FROM categories WHERE id=$1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    ensure!(system == 0, "System categories cannot be deleted");
    let used: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM products WHERE category_id=$1 OR category=$2) OR EXISTS(SELECT 1 FROM categories WHERE parent_id=$1)").bind(id).bind(name).fetch_one(&mut *tx).await?;
    ensure!(
        !used,
        "Category still contains products or child categories"
    );
    sqlx::query("DELETE FROM categories WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_hierarchy_and_names() {
        let parent = CategoryRecord {
            id: 1,
            name: "Parent".into(),
            status: "active".into(),
            ..Default::default()
        };
        let child = CategoryRecord {
            id: 2,
            name: "Child".into(),
            parent_id: Some(1),
            status: "active".into(),
            ..Default::default()
        };
        let rows = vec![parent.clone(), child.clone()];
        assert!(validate(&child, &rows).is_ok());
        assert!(validate(
            &CategoryRecord {
                parent_id: Some(2),
                ..parent.clone()
            },
            &rows
        )
        .is_err());
        assert!(validate(
            &CategoryRecord {
                name: " parent ".into(),
                ..child.clone()
            },
            &rows
        )
        .is_err());
        assert!(validate(
            &CategoryRecord {
                parent_id: Some(99),
                ..child
            },
            &rows
        )
        .is_err());
    }
}
