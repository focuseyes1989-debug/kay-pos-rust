use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool};
use subtle::ConstantTimeEq;

#[derive(Clone, PartialEq)]
pub struct Session {
    id: i32,
    username: String,
    role: String,
    credential_hash: String,
}

#[derive(Clone, Copy)]
pub enum Permission {
    Sell,
    Manage,
    Admin,
}

impl Session {
    pub fn username(&self) -> &str {
        &self.username
    }
    pub fn allows(&self, permission: Permission) -> bool {
        allows(&self.role, permission)
    }

    pub async fn authorize(
        &self,
        connection: &mut PgConnection,
        permission: Permission,
    ) -> Result<()> {
        let row: Option<(String, String, String)> = sqlx::query_as(
            "SELECT username,role,password_hash FROM users WHERE id=$1 AND is_active=1",
        )
        .bind(self.id)
        .fetch_optional(connection)
        .await?;
        let (username, role, hash) = row.context("Session expired. Sign in again")?;
        ensure!(
            username == self.username
                && hash == self.credential_hash
                && role == self.role
                && allows(&role, permission),
            "Permission denied or account changed. Sign in again"
        );
        Ok(())
    }
}

fn allows(role: &str, permission: Permission) -> bool {
    match permission {
        Permission::Sell => matches!(
            role.to_ascii_lowercase().as_str(),
            "cashier" | "manager" | "admin"
        ),
        Permission::Manage => matches!(role.to_ascii_lowercase().as_str(), "manager" | "admin"),
        Permission::Admin => role.eq_ignore_ascii_case("admin"),
    }
}

pub async fn login(pool: &PgPool, username: &str, password: &str) -> Result<Session> {
    let row: Option<(i32, String, String, String, String)> = sqlx::query_as(
        "SELECT id,username,role,password_hash,salt FROM users WHERE username=$1 AND is_active=1",
    )
    .bind(username.trim())
    .fetch_optional(pool)
    .await?;
    let (id, username, role, hash, salt) = row.context("Invalid username or password")?;
    let mut actual = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(
        password.as_bytes(),
        &hex::decode(salt)?,
        100000,
        &mut actual,
    );
    ensure!(
        bool::from(actual.as_slice().ct_eq(&hex::decode(&hash)?))
            && allows(&role, Permission::Sell),
        "Invalid username or password"
    );
    Ok(Session {
        id,
        username,
        role,
        credential_hash: hash,
    })
}

pub fn fingerprint(value: &[u8]) -> String {
    hex::encode(Sha256::digest(value))
}

pub fn is_database_error(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| cause.downcast_ref::<sqlx::Error>().is_some())
}

pub fn new_request_id() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    format!("RUST-{}", hex::encode(bytes))
}

#[test]
fn role_permissions() {
    assert!(allows("Cashier", Permission::Sell));
    assert!(!allows("Cashier", Permission::Manage));
    assert!(allows("Manager", Permission::Manage));
    assert!(!allows("Manager", Permission::Admin));
    assert!(allows("Admin", Permission::Admin));
    assert!(!allows("unknown", Permission::Sell));
    assert_ne!(new_request_id(), new_request_id());
}
