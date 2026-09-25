use crate::auth::{Permission, Session};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Device {
    pub id: i32,
    pub device_no: i32,
    pub name: String,
    pub ip_address: String,
    pub port: i32,
    pub comm_key: i32,
    pub serial_no: String,
    pub last_sync_at: String,
    pub is_active: i32,
}
impl Default for Device {
    fn default() -> Self {
        Self {
            id: 0,
            device_no: 1,
            name: "ZKTeco Device".into(),
            ip_address: String::new(),
            port: 4370,
            comm_key: 0,
            serial_no: String::new(),
            last_sync_at: String::new(),
            is_active: 1,
        }
    }
}
const COLUMNS: &str = "id,device_no,COALESCE(name,'') AS name,ip_address,COALESCE(port,4370) AS port,COALESCE(comm_key,0) AS comm_key,COALESCE(serial_no,'') AS serial_no,COALESCE(last_sync_at::text,'') AS last_sync_at,COALESCE(is_active,1) AS is_active";
pub fn validate(device: &Device) -> Result<()> {
    ensure!(device.device_no > 0, "Device ID must be positive");
    ensure!(
        !device.name.trim().is_empty() && device.name.chars().count() <= 100,
        "Name is required (maximum 100 characters)"
    );
    let ip: std::net::IpAddr = device
        .ip_address
        .trim()
        .parse()
        .context("Enter a valid IP address")?;
    ensure!(
        !ip.is_unspecified() && !ip.is_multicast(),
        "Enter a device IP address, not a wildcard or multicast address"
    );
    ensure!(
        (1..=65535).contains(&device.port),
        "TCP port must be 1-65535"
    );
    ensure!(device.comm_key >= 0, "Comm Key cannot be negative");
    ensure!(matches!(device.is_active, 0 | 1), "Invalid status");
    Ok(())
}
pub async fn list(pool: &PgPool, actor: &Session) -> Result<Vec<Device>> {
    let mut conn = pool.acquire().await?;
    actor.authorize(&mut conn, Permission::Admin).await?;
    sqlx::query_as(&format!("SELECT {COLUMNS} FROM zkteco_devices ORDER BY device_no,id")).fetch_all(&mut *conn).await.context("Cannot load ZKTeco devices. Initialize devices in Main POS or apply migrations/004_zkteco_devices.sql as database owner")
}
pub async fn save(
    pool: &PgPool,
    actor: &Session,
    value: &Device,
    original: Option<&Device>,
) -> Result<Device> {
    validate(value)?;
    let mut tx = pool.begin().await?;
    actor.authorize(&mut tx, Permission::Admin).await?;
    let id = if value.id > 0 {
        let old: Device = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM zkteco_devices WHERE id=$1 FOR UPDATE"
        ))
        .bind(value.id)
        .fetch_one(&mut *tx)
        .await?;
        let original = original.context("Select a device before editing")?;
        // Sync metadata may change independently; protect only editable configuration.
        ensure!(
            same_config(&old, original),
            "Device configuration changed. Reload and select the device again"
        );
        sqlx::query("UPDATE zkteco_devices SET device_no=$1,name=$2,ip_address=$3,port=$4,comm_key=$5,is_active=$6 WHERE id=$7")
            .bind(value.device_no).bind(value.name.trim()).bind(value.ip_address.trim()).bind(value.port).bind(value.comm_key).bind(value.is_active).bind(value.id).execute(&mut *tx).await?;
        value.id
    } else {
        // The shared unique device number prevents duplicate entries on uncertain retries.
        let inserted:Option<i32>=sqlx::query_scalar("INSERT INTO zkteco_devices(device_no,name,ip_address,port,comm_key,is_active) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(device_no) DO NOTHING RETURNING id")
            .bind(value.device_no).bind(value.name.trim()).bind(value.ip_address.trim()).bind(value.port).bind(value.comm_key).bind(value.is_active).fetch_optional(&mut *tx).await?;
        if let Some(id) = inserted {
            id
        } else {
            let existing: Device = sqlx::query_as(&format!(
                "SELECT {COLUMNS} FROM zkteco_devices WHERE device_no=$1 FOR UPDATE"
            ))
            .bind(value.device_no)
            .fetch_one(&mut *tx)
            .await?;
            ensure!(
                same_config(&existing, value),
                "Device ID already exists. Select it to edit"
            );
            existing.id
        }
    };
    let saved = sqlx::query_as(&format!("SELECT {COLUMNS} FROM zkteco_devices WHERE id=$1"))
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(saved)
}
fn same_config(a: &Device, b: &Device) -> bool {
    a.device_no == b.device_no
        && a.name.trim() == b.name.trim()
        && a.ip_address.trim() == b.ip_address.trim()
        && a.port == b.port
        && a.comm_key == b.comm_key
        && a.is_active == b.is_active
}
pub async fn test_tcp(pool: &PgPool, actor: &Session, device: &Device) -> Result<()> {
    validate(device)?;
    {
        let mut conn = pool.acquire().await?;
        actor.authorize(&mut conn, Permission::Admin).await?;
    }
    let address = std::net::SocketAddr::new(device.ip_address.trim().parse()?, device.port as u16);
    tokio::task::spawn_blocking(move || {
        std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_secs(5))
            .map(|_| ())
            .context("TCP connection failed. Check device IP, port, power and firewall")
    })
    .await??;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_device_configuration() {
        let mut d = Device::default();
        assert!(validate(&d).is_err());
        d.ip_address = "192.168.110.246".into();
        assert!(validate(&d).is_ok());
        for ip in ["", "not-an-ip", "0.0.0.0", "224.0.0.1"] {
            d.ip_address = ip.into();
            assert!(validate(&d).is_err());
        }
        d.ip_address = "127.0.0.1".into();
        d.port = 65536;
        assert!(validate(&d).is_err());
        d.port = 4370;
        d.comm_key = -1;
        assert!(validate(&d).is_err());
    }
    #[test]
    fn sync_metadata_does_not_conflict_with_configuration() {
        let a = Device::default();
        let mut b = a.clone();
        b.serial_no = "serial".into();
        b.last_sync_at = "2026-09-22".into();
        assert!(same_config(&a, &b));
        b.comm_key = 1;
        assert!(!same_config(&a, &b));
    }
}
