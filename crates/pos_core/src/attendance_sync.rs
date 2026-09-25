use crate::{auth::Session, employees, zkteco::Device};
use anyhow::{ensure, Context, Result};
use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime};
use sqlx::{PgConnection, PgPool};
use std::collections::{BTreeSet, HashSet};

pub fn date_range(today: NaiveDate, preset: &str) -> (String, String) {
    let start = match preset {
        "week" => today - chrono::Duration::days(i64::from(today.weekday().num_days_from_monday())),
        "month" => today.with_day(1).unwrap(),
        _ => today,
    };
    (start.to_string(), today.to_string())
}
async fn authorize(conn: &mut PgConnection, actor: &Session) -> Result<()> {
    let perms = employees::permissions(conn, actor).await?;
    for permission in [
        "employees",
        "settings",
        "manage_attendance",
        "manage_employees",
    ] {
        ensure!(
            employees::has(&perms, permission),
            "Attendance sync permission required: {permission}"
        );
    }
    Ok(())
}
#[derive(Clone, PartialEq, sqlx::FromRow)]
pub struct DeviceChoice {
    pub id: i32,
    pub device_no: i32,
    pub name: String,
    pub mappings: i64,
}
pub async fn devices(pool: &PgPool, actor: &Session) -> Result<Vec<DeviceChoice>> {
    let mut conn = pool.acquire().await?;
    authorize(&mut conn, actor).await?;
    sqlx::query_as("SELECT d.id,d.device_no,COALESCE(d.name,'ZKTeco Device') AS name,COUNT(e.id) AS mappings FROM zkteco_devices d LEFT JOIN zkteco_employee_mappings m ON m.device_id=d.id LEFT JOIN employees e ON e.id=m.employee_id AND e.employment_status='Active' WHERE d.is_active=1 GROUP BY d.id ORDER BY d.device_no")
        .fetch_all(&mut *conn).await.context("Cannot load attendance devices. Initialize ZKTeco in Main POS or apply migration 005")
}
#[derive(Clone, PartialEq, sqlx::FromRow)]
struct Mapping {
    employee_id: i32,
    device_user_id: String,
}
#[derive(Clone)]
pub struct SyncPlan {
    device: Device,
    mappings: Vec<Mapping>,
}
async fn read_plan(conn: &mut PgConnection, id: i32) -> Result<SyncPlan> {
    let device=sqlx::query_as::<_,Device>("SELECT id,device_no,COALESCE(name,'') AS name,ip_address,COALESCE(port,4370) AS port,COALESCE(comm_key,0) AS comm_key,COALESCE(serial_no,'') AS serial_no,COALESCE(last_sync_at::text,'') AS last_sync_at,COALESCE(is_active,1) AS is_active FROM zkteco_devices WHERE id=$1")
        .bind(id).fetch_one(&mut *conn).await?;
    ensure!(device.is_active == 1, "Device is inactive");
    crate::zkteco::validate(&device)?;
    let mappings=sqlx::query_as::<_,Mapping>("SELECT m.employee_id,m.device_user_id FROM zkteco_employee_mappings m JOIN employees e ON e.id=m.employee_id WHERE m.device_id=$1 AND e.employment_status='Active' ORDER BY m.employee_id,m.id")
        .bind(id).fetch_all(&mut *conn).await?;
    ensure!(
        !mappings.is_empty(),
        "No active employee mappings. Configure this device's employee mappings in Main POS first"
    );
    Ok(SyncPlan { device, mappings })
}
pub async fn prepare(pool: &PgPool, actor: &Session, id: i32) -> Result<SyncPlan> {
    let mut conn = pool.acquire().await?;
    authorize(&mut conn, actor).await?;
    read_plan(&mut conn, id).await
}
#[derive(Clone)]
pub struct Punch {
    pub user_id: String,
    pub time: NaiveDateTime,
    pub status: i32,
    pub punch: i32,
}
pub struct Snapshot {
    pub serial: String,
    pub device_time: NaiveDateTime,
    pub users: HashSet<String>,
    pub punches: Vec<Punch>,
}
#[derive(Default)]
pub struct SyncResult {
    pub inserted: usize,
    pub duplicates: usize,
    pub invalid: usize,
    pub unmapped: usize,
    pub days: usize,
    pub preserved: usize,
}
impl SyncResult {
    pub fn message(&self) -> String {
        format!("Sync complete: {} new punches, {} duplicates, {} attendance days; {} manual corrections preserved, {} invalid timestamps, {} unmapped punches skipped.",self.inserted,self.duplicates,self.days,self.preserved,self.invalid,self.unmapped)
    }
}
fn connection_error(error: &rustzk::ZKError) -> String {
    use rustzk::{ZKError, ZKErrorCode};
    match error {
        ZKError::Connection(ZKErrorCode::Unauthorized,_) | ZKError::Response(ZKErrorCode::Unauthorized,_) =>
            "Device rejected authentication. Check that the saved Comm Key matches the device.".into(),
        _ if error.is_timeout()=>"No response (timeout). Check device power, IP/port, LAN connection and firewall.".into(),
        ZKError::Network(e) if e.kind()==std::io::ErrorKind::ConnectionRefused =>
            "Connection refused. Check the device port and whether another application is using the device.".into(),
        ZKError::Network(_)=>"Network connection failed. Check the device IP/port and LAN connection.".into(),
        _=>"Device protocol handshake failed. Check device compatibility and connection settings.".into(),
    }
}
fn connect_device(
    mut connect: impl FnMut(rustzk::ZKProtocol) -> rustzk::ZKResult<()>,
) -> Result<()> {
    use rustzk::{ZKError, ZKErrorCode, ZKProtocol};
    match connect(ZKProtocol::TCP) {
        Ok(()) => Ok(()),
        Err(tcp) => {
            // An authentication rejection is definitive, not a transport timeout.
            if matches!(
                tcp,
                ZKError::Connection(ZKErrorCode::Unauthorized, _)
                    | ZKError::Response(ZKErrorCode::Unauthorized, _)
            ) {
                anyhow::bail!("{}", connection_error(&tcp));
            }
            connect(ZKProtocol::UDP).map_err(|udp| {
                anyhow::anyhow!(
                    "TCP: {} UDP: {}",
                    connection_error(&tcp),
                    connection_error(&udp)
                )
            })
        }
    }
}
fn read_device(device: Device) -> Result<Snapshot> {
    static READING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = READING.try_lock().map_err(|_| {
        anyhow::anyhow!("Another device read is still running. Try again after it finishes")
    })?;
    let mut zk = rustzk::ZK::new(&device.ip_address, device.port as u16);
    zk.set_password(device.comm_key as u32);
    zk.set_timeout(std::time::Duration::from_secs(10));
    let result = (|| -> Result<Snapshot> {
        connect_device(|protocol| zk.connect(protocol))?;
        let serial = zk.get_serial_number()?;
        let device_time = zk.get_time()?.naive_local();
        // Explicit user read must succeed; never guess a UID-to-employee mapping.
        let users = zk
            .get_users()?
            .iter()
            .map(|u| u.user_id().to_string())
            .collect();
        let punches = zk
            .get_attendance()?
            .iter()
            .map(|p| Punch {
                user_id: p.user_id().to_string(),
                time: p.timestamp(),
                status: i32::from(p.status()),
                punch: i32::from(p.punch()),
            })
            .collect::<Vec<_>>();
        ensure!(
            punches.len() <= 1_000_000,
            "Device returned too many attendance records"
        );
        Ok(Snapshot {
            serial,
            device_time,
            users,
            punches,
        })
    })();
    let _ = zk.disconnect();
    result
}
pub async fn sync(pool: &PgPool, actor: &Session, id: i32) -> Result<SyncResult> {
    let plan = prepare(pool, actor, id).await?;
    let device = plan.device.clone();
    // Hardware I/O holds no database connection/transaction or employee row lock.
    let snapshot = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        tokio::task::spawn_blocking(move || read_device(device)),
    ).await.context("Device read exceeded two minutes; no attendance was imported. A read may still be finishing, so wait before retrying")???;
    import_snapshot(pool, actor, &plan, snapshot).await
}
pub async fn import_snapshot(
    pool: &PgPool,
    actor: &Session,
    plan: &SyncPlan,
    snapshot: Snapshot,
) -> Result<SyncResult> {
    let mut tx = pool.begin().await?;
    authorize(&mut tx, actor).await?;
    // Serialize raw-log insertion and aggregation, including Main POS writers.
    sqlx::query("LOCK TABLE zkteco_devices,zkteco_employee_mappings,employees,zkteco_attendance_logs,attendance IN SHARE ROW EXCLUSIVE MODE").execute(&mut *tx).await?;
    let current = read_plan(&mut tx, plan.device.id).await?;
    ensure!(
        current.device.ip_address == plan.device.ip_address
            && current.device.port == plan.device.port
            && current.device.comm_key == plan.device.comm_key
            && current.mappings == plan.mappings,
        "Device configuration or employee mappings changed during sync. Refresh and retry"
    );
    for m in &plan.mappings {
        ensure!(
            snapshot.users.contains(&m.device_user_id),
            "Mapped device user {} was not found on this device",
            m.device_user_id
        );
    }
    let cutoff = snapshot.device_time + chrono::Duration::days(1);
    let mut result = SyncResult::default();
    let mut days = BTreeSet::new();
    for log in &snapshot.punches {
        let Some(mapping) = plan
            .mappings
            .iter()
            .find(|m| m.device_user_id == log.user_id)
        else {
            result.unmapped += 1;
            continue;
        };
        let valid = log.time <= cutoff;
        let inserted=sqlx::query("INSERT INTO zkteco_attendance_logs(device_id,device_user_id,employee_id,punch_time,status,punch,is_valid,validation_note) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(device_id,device_user_id,punch_time,punch) DO NOTHING")
            .bind(plan.device.id).bind(&log.user_id).bind(mapping.employee_id).bind(log.time).bind(log.status).bind(log.punch).bind(i32::from(valid)).bind(if valid{None}else{Some("Future timestamp beyond device time")}).execute(&mut *tx).await?.rows_affected();
        if inserted == 1 {
            result.inserted += 1;
        } else {
            result.duplicates += 1;
            let owner:Option<i32>=sqlx::query_scalar("SELECT employee_id FROM zkteco_attendance_logs WHERE device_id=$1 AND device_user_id=$2 AND punch_time=$3 AND punch=$4").bind(plan.device.id).bind(&log.user_id).bind(log.time).bind(log.punch).fetch_one(&mut *tx).await?;
            ensure!(owner==Some(mapping.employee_id),"Historical punch belongs to another employee. Review the device mapping in Main POS");
        }
        if valid {
            days.insert((mapping.employee_id, log.time.date()));
        } else {
            result.invalid += 1;
        }
    }
    for (employee, day) in days {
        let date = day.to_string();
        let (cin,cout):(Option<NaiveDateTime>,Option<NaiveDateTime>)=sqlx::query_as("SELECT MIN(punch_time) FILTER(WHERE punch IN (0,4)),MAX(punch_time) FILTER(WHERE punch IN (1,5)) FROM zkteco_attendance_logs WHERE employee_id=$1 AND is_valid=1 AND punch_time>=$2 AND punch_time<$3")
            .bind(employee).bind(day.and_hms_opt(0,0,0).unwrap()).bind((day+chrono::Duration::days(1)).and_hms_opt(0,0,0).unwrap()).fetch_one(&mut *tx).await?;
        let leave:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM employee_leave WHERE employee_id=$1 AND status='Approved' AND $2 BETWEEN start_date AND end_date)").bind(employee).bind(&date).fetch_one(&mut *tx).await?;
        let shift:Option<String>=sqlx::query_scalar("SELECT s.start_time FROM employee_shifts a JOIN shifts s ON s.id=a.shift_id WHERE a.employee_id=$1 AND a.effective_from<=$2 AND (NULLIF(a.effective_to,'') IS NULL OR a.effective_to>=$2) ORDER BY a.effective_from DESC,a.id DESC LIMIT 1").bind(employee).bind(&date).fetch_optional(&mut *tx).await?;
        let (status, late) = classify(cin, cout, leave, shift.as_deref())?;
        let changed=sqlx::query("INSERT INTO attendance(employee_id,attendance_date,check_in,check_out,status,late_minutes,notes) VALUES($1,$2,$3,$4,$5,$6,'ZKTeco sync') ON CONFLICT(employee_id,attendance_date) DO UPDATE SET check_in=EXCLUDED.check_in,check_out=EXCLUDED.check_out,status=EXCLUDED.status,late_minutes=EXCLUDED.late_minutes,notes=EXCLUDED.notes,updated_at=CURRENT_TIMESTAMP WHERE COALESCE(attendance.correction_reason,'')=''")
            .bind(employee).bind(&date).bind(cin.map(|d|d.format("%H:%M").to_string())).bind(cout.map(|d|d.format("%H:%M").to_string())).bind(status).bind(late).execute(&mut *tx).await?.rows_affected();
        if changed == 0 {
            result.preserved += 1;
        } else {
            result.days += 1;
        }
    }
    sqlx::query(
        "UPDATE zkteco_devices SET serial_no=$1,last_sync_at=CURRENT_TIMESTAMP WHERE id=$2",
    )
    .bind(snapshot.serial)
    .bind(plan.device.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(result)
}
fn classify(
    cin: Option<NaiveDateTime>,
    cout: Option<NaiveDateTime>,
    leave: bool,
    shift: Option<&str>,
) -> Result<(&'static str, i32)> {
    if leave {
        return Ok(("Leave", 0));
    }
    match (cin, cout) {
        (None, None) => Ok(("Absent", 0)),
        (Some(a), Some(b)) if a.format("%H:%M").to_string() != b.format("%H:%M").to_string() => {
            let late = if let Some(start) = shift {
                let start = NaiveTime::parse_from_str(start, "%H:%M")
                    .or_else(|_| NaiveTime::parse_from_str(start, "%H:%M:%S"))?;
                (a.time() - start).num_minutes().max(0) as i32
            } else {
                0
            };
            Ok((if late > 0 { "Late" } else { "Present" }, late))
        }
        _ => Ok(("Incomplete", 0)),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn connection_falls_back_to_udp_only_after_tcp_failure() {
        use rustzk::{ZKError, ZKProtocol};
        let mut attempts = Vec::new();
        connect_device(|protocol| {
            attempts.push(protocol);
            if protocol == ZKProtocol::TCP {
                Err(ZKError::Network(std::io::Error::from(
                    std::io::ErrorKind::TimedOut,
                )))
            } else {
                Ok(())
            }
        })
        .unwrap();
        assert_eq!(attempts, vec![ZKProtocol::TCP, ZKProtocol::UDP]);
        attempts.clear();
        connect_device(|protocol| {
            attempts.push(protocol);
            Ok(())
        })
        .unwrap();
        assert_eq!(attempts, vec![ZKProtocol::TCP]);
    }
    #[test]
    fn authentication_and_timeouts_are_distinguished() {
        use rustzk::{ZKError, ZKErrorCode};
        let mut count = 0;
        let error = connect_device(|_| {
            count += 1;
            Err(ZKError::Connection(
                ZKErrorCode::Unauthorized,
                "rejected".into(),
            ))
        })
        .unwrap_err();
        assert_eq!(count, 1);
        assert!(error.to_string().contains("Comm Key"));
        let error = connect_device(|_| {
            Err(ZKError::Network(std::io::Error::from(
                std::io::ErrorKind::TimedOut,
            )))
        })
        .unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("TCP:") && message.contains("UDP:") && message.contains("timeout")
        );
        assert!(!message.contains("authentication") && !message.contains("os error"));
    }
    #[test]
    fn quick_ranges_cross_year_and_leap_month() {
        let date = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        assert_eq!(
            date_range(date, "week"),
            ("2024-12-30".into(), "2025-01-01".into())
        );
        let leap = NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
        assert_eq!(
            date_range(leap, "month"),
            ("2024-02-01".into(), "2024-02-29".into())
        );
        assert_eq!(
            date_range(leap, "today"),
            (leap.to_string(), leap.to_string())
        );
    }
}
