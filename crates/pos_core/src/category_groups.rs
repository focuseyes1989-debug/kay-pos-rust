use crate::auth::{Permission, Session};
use anyhow::{ensure, Context, Result};
use sqlx::PgPool;

#[derive(Clone, Debug, PartialEq, Default, sqlx::FromRow)]
pub struct Group {
    pub id: i32,
    pub name: String,
    pub description: String,
    pub sort_order: i32,
    pub icon: String,
    pub color: String,
    pub is_favorite: i32,
    pub is_active: i32,
}
const FIELDS:&str="id,name,COALESCE(description,'') AS description,COALESCE(sort_order,0) AS sort_order,COALESCE(icon,'') AS icon,COALESCE(color,'#6c5ce7') AS color,COALESCE(is_favorite,0) AS is_favorite,COALESCE(is_active,1) AS is_active";

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn group_fields_are_bounded() {
        let mut g = Group {
            id: 1,
            name: "Paper".into(),
            color: "#00aa77".into(),
            is_active: 1,
            ..Default::default()
        };
        assert!(validate(&g).is_ok());
        g.color = "red;bad".into();
        assert!(validate(&g).is_err());
        g.color = "#00aa77".into();
        g.sort_order = -1;
        assert!(validate(&g).is_err());
        g.sort_order = 0;
        g.is_active = 2;
        assert!(validate(&g).is_err());
    }
}
pub async fn list(pool: &PgPool, actor: &Session) -> Result<Vec<Group>> {
    let mut tx = pool.begin().await?;
    actor.authorize(&mut tx, Permission::Manage).await?;
    let rows = sqlx::query_as(&format!(
        "SELECT {FIELDS} FROM category_groups ORDER BY sort_order,name,id"
    ))
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows)
}
pub async fn reserve(pool: &PgPool, actor: &Session) -> Result<Group> {
    let mut tx = pool.begin().await?;
    actor.authorize(&mut tx, Permission::Manage).await?;
    loop {
        let id: i64 =
            sqlx::query_scalar("SELECT nextval(pg_get_serial_sequence('category_groups','id'))")
                .fetch_one(&mut *tx)
                .await?;
        let id = i32::try_from(id)?;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM category_groups WHERE id=$1)")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        if !exists {
            tx.commit().await?;
            return Ok(Group {
                id,
                color: "#6c5ce7".into(),
                is_active: 1,
                ..Default::default()
            });
        }
    }
}
pub fn validate(g: &Group) -> Result<()> {
    ensure!(
        g.id > 0 && !g.name.trim().is_empty() && g.name.chars().count() <= 200,
        "Group name is required (maximum 200 characters)"
    );
    ensure!(
        g.description.len() <= 4000 && g.icon.len() <= 120 && g.sort_order >= 0,
        "Invalid group details"
    );
    ensure!(
        g.color.len() == 7
            && g.color.starts_with('#')
            && g.color[1..].bytes().all(|c| c.is_ascii_hexdigit()),
        "Choose a valid group color"
    );
    ensure!(
        [0, 1].contains(&g.is_active) && [0, 1].contains(&g.is_favorite),
        "Invalid group flags"
    );
    Ok(())
}
pub async fn save(pool: &PgPool, actor: &Session, old: Option<&Group>, new: &Group) -> Result<()> {
    validate(new)?;
    ensure!(old.is_none_or(|g| g.id == new.id), "Group identity changed");
    let mut tx = pool.begin().await?;
    actor.authorize(&mut tx, Permission::Manage).await?;
    sqlx::query("SET LOCAL lock_timeout='5s'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("LOCK TABLE category_groups IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;
    let current: Option<Group> =
        sqlx::query_as(&format!("SELECT {FIELDS} FROM category_groups WHERE id=$1"))
            .bind(new.id)
            .fetch_optional(&mut *tx)
            .await?;
    if current.as_ref() == Some(new) {
        tx.commit().await?;
        return Ok(());
    }
    ensure!(
        current.as_ref() == old,
        "Group changed on another PC. Refresh and reopen it."
    );
    let duplicate:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM category_groups WHERE lower(btrim(name))=lower(btrim($1)) AND id<>$2)").bind(&new.name).bind(new.id).fetch_one(&mut *tx).await?;
    ensure!(!duplicate, "Group name already exists");
    if old.is_some() {
        sqlx::query("UPDATE category_groups SET name=$2,description=$3,sort_order=$4,icon=$5,color=$6,is_favorite=$7,is_active=$8,updated_at=clock_timestamp() WHERE id=$1")
            .bind(new.id).bind(&new.name).bind(&new.description).bind(new.sort_order).bind(&new.icon).bind(&new.color).bind(new.is_favorite).bind(new.is_active).execute(&mut *tx).await?;
    } else {
        sqlx::query("INSERT INTO category_groups(id,name,description,sort_order,icon,color,is_favorite,is_active) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(new.id).bind(&new.name).bind(&new.description).bind(new.sort_order).bind(&new.icon).bind(&new.color).bind(new.is_favorite).bind(new.is_active).execute(&mut *tx).await?;
    }
    crate::activity::record(
        &mut tx,
        actor,
        "rust.category_group.save",
        &format!("group_id={}; active={}", new.id, new.is_active),
    )
    .await
    .context("Category group audit failed")?;
    tx.commit().await?;
    Ok(())
}
