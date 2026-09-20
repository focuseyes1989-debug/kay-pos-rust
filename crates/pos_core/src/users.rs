use anyhow::{ensure, Context, Result};
use sqlx::PgPool;
use subtle::ConstantTimeEq;
#[derive(Clone, Default, PartialEq, sqlx::FromRow)]
pub struct User {
    pub id: i32,
    pub username: String,
    pub full_name: String,
    pub role: String,
    pub active: i32,
    pub avatar: Option<String>,
}
pub async fn list(pool: &PgPool) -> Result<Vec<User>> {
    Ok(sqlx::query_as("SELECT u.id,u.username,COALESCE(u.full_name,'') AS full_name,u.role,COALESCE(u.is_active,1) AS active,(SELECT 'data:image/jpeg;base64,'||encode(e.photo_data,'base64') FROM employees e WHERE e.user_id=u.id AND e.photo_data IS NOT NULL ORDER BY e.id DESC LIMIT 1) AS avatar FROM users u ORDER BY LOWER(u.username),u.id").fetch_all(pool).await?)
}
pub async fn save(
    pool: &PgPool,
    u: &User,
    password: &str,
    delete: bool,
    admin: &str,
    secret: &str,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("LOCK TABLE users IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;
    let auth:Option<(i32,String,String)>=sqlx::query_as("SELECT id,password_hash,salt FROM users WHERE username=$1 AND LOWER(role)='admin' AND is_active=1").bind(admin.trim()).fetch_optional(&mut *tx).await?;
    let (admin_id, hash, salt) = auth.context("Active administrator credentials required")?;
    let mut actual = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(
        secret.as_bytes(),
        &hex::decode(salt)?,
        100000,
        &mut actual,
    );
    ensure!(
        bool::from(actual.as_slice().ct_eq(&hex::decode(hash)?)),
        "Invalid administrator credentials"
    );
    if u.id != 0 {
        let (role, active): (String, i32) =
            sqlx::query_as("SELECT role,COALESCE(is_active,1) FROM users WHERE id=$1")
                .bind(u.id)
                .fetch_one(&mut *tx)
                .await?;
        if delete || u.active == 0 || !u.role.eq_ignore_ascii_case("admin") {
            ensure!(
                u.id != admin_id,
                "You cannot delete, deactivate or demote the authorizing administrator"
            );
            if role.eq_ignore_ascii_case("admin") && active == 1 {
                let others:i64=sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE LOWER(role)='admin' AND is_active=1 AND id<>$1").bind(u.id).fetch_one(&mut *tx).await?;
                ensure!(others > 0, "Keep at least one active administrator");
            }
        }
    }
    if delete {
        ensure!(u.id > 0, "Select a user");
        sqlx::query("DELETE FROM users WHERE id=$1")
            .bind(u.id)
            .execute(&mut *tx)
            .await?;
    } else {
        ensure!(!u.username.trim().is_empty(), "Username is required");
        ensure!(
            ["admin", "manager", "cashier"].contains(&u.role.to_lowercase().as_str()),
            "Invalid role"
        );
        ensure!(
            u.id != 0 || !password.is_empty(),
            "Password is required for a new user"
        );
        let credentials = if !password.is_empty() {
            use rand::RngCore;
            let mut salt = [0u8; 16];
            rand::rngs::OsRng.fill_bytes(&mut salt);
            let mut hash = [0u8; 32];
            pbkdf2::pbkdf2_hmac::<sha2::Sha256>(password.as_bytes(), &salt, 100000, &mut hash);
            Some((hex::encode(hash), hex::encode(salt)))
        } else {
            None
        };
        if u.id == 0 {
            let (hash, salt) = credentials.unwrap();
            sqlx::query("INSERT INTO users(username,full_name,role,is_active,password_hash,salt) VALUES($1,$2,$3,$4,$5,$6)").bind(u.username.trim()).bind(u.full_name.trim()).bind(&u.role).bind(u.active).bind(hash).bind(salt).execute(&mut *tx).await?;
        } else {
            sqlx::query(
                "UPDATE users SET username=$1,full_name=$2,role=$3,is_active=$4 WHERE id=$5",
            )
            .bind(u.username.trim())
            .bind(u.full_name.trim())
            .bind(&u.role)
            .bind(u.active)
            .bind(u.id)
            .execute(&mut *tx)
            .await?;
            if let Some((hash, salt)) = credentials {
                sqlx::query("UPDATE users SET password_hash=$1,salt=$2 WHERE id=$3")
                    .bind(hash)
                    .bind(salt)
                    .bind(u.id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }
    tx.commit().await?;
    Ok(())
}
