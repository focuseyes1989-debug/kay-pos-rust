use crate::auth::{Permission, Session};
use anyhow::{ensure, Context, Result};
use chrono::NaiveDateTime;
use sqlx::{PgPool, Postgres, Transaction};

#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct Job {
    pub id: i32,
    pub order_no: String,
    pub job_title: String,
    pub complaint: String,
    pub internal_notes: String,
    pub status: String,
    pub received_at: NaiveDateTime,
    pub expected_at: Option<NaiveDateTime>,
    pub started_by: String,
    pub started_at: Option<NaiveDateTime>,
    pub completed_by: String,
    pub completed_at: Option<NaiveDateTime>,
    pub delivered_by: String,
    pub delivered_at: Option<NaiveDateTime>,
    pub updated_at: NaiveDateTime,
}
const JOB_FIELDS: &str = "id,order_no,COALESCE(job_title,'') AS job_title,COALESCE(complaint,'') AS complaint,COALESCE(internal_notes,'') AS internal_notes,status,received_at,expected_at,COALESCE(started_by,'') AS started_by,started_at,COALESCE(completed_by,'') AS completed_by,completed_at,COALESCE(delivered_by,'') AS delivered_by,delivered_at,updated_at";
pub fn ready(status: &str) -> bool {
    matches!(status, "ready" | "completed" | "ready_for_pickup")
}
pub fn status_label(status: &str) -> &str {
    if ready(status) {
        "Ready for Pickup"
    } else {
        match status {
            "in_progress" => "In Progress",
            "delivered" => "Delivered",
            "cancelled" => "Cancelled",
            _ => "Pending",
        }
    }
}
pub fn can_start(status: &str) -> bool {
    matches!(
        status,
        "received" | "assigned" | "waiting_parts" | "on_hold"
    )
}
pub fn can_complete(status: &str) -> bool {
    !ready(status) && !matches!(status, "delivered" | "cancelled")
}

async fn transaction(pool: &PgPool, actor: &Session) -> Result<Transaction<'static, Postgres>> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET LOCAL lock_timeout='5s'")
        .execute(&mut *tx)
        .await?;
    actor.authorize(&mut tx, Permission::Sell).await?;
    Ok(tx)
}

pub async fn list(pool: &PgPool, query: &str, status: &str, limit: i64) -> Result<Vec<Job>> {
    let sql = format!("SELECT {JOB_FIELDS} FROM service_orders WHERE ($1='' OR job_title ILIKE $1 OR complaint ILIKE $1 OR internal_notes ILIKE $1 OR order_no ILIKE $1) AND ($2='' OR ($2='pending' AND status NOT IN ('in_progress','ready','completed','ready_for_pickup','delivered','cancelled')) OR ($2='ready_for_pickup' AND status IN ('ready','completed','ready_for_pickup')) OR status=$2) ORDER BY received_at DESC,id DESC LIMIT $3");
    let query = if query.trim().is_empty() {
        String::new()
    } else {
        format!("%{}%", query.trim())
    };
    Ok(sqlx::query_as(&sql).bind(query).bind(status).bind(limit.clamp(1, 10001)).fetch_all(pool).await
        .context("Service Jobs schema unavailable. Initialize/update the existing KAY POS Lite service-order schema first.")?)
}

pub async fn reserve(pool: &PgPool, actor: &Session) -> Result<Job> {
    let mut tx = transaction(pool, actor).await?;
    loop {
        let id: i64 =
            sqlx::query_scalar("SELECT nextval(pg_get_serial_sequence('service_orders','id'))")
                .fetch_one(&mut *tx)
                .await?;
        let id = i32::try_from(id)?;
        let order_no = format!("SO{id:06}");
        let used: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM service_orders WHERE id=$1 OR order_no=$2)",
        )
        .bind(id)
        .bind(&order_no)
        .fetch_one(&mut *tx)
        .await?;
        if used {
            continue;
        }
        let now: NaiveDateTime = sqlx::query_scalar("SELECT LOCALTIMESTAMP")
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        return Ok(Job {
            id,
            order_no,
            job_title: String::new(),
            complaint: String::new(),
            internal_notes: String::new(),
            status: "received".into(),
            received_at: now,
            expected_at: None,
            started_by: String::new(),
            started_at: None,
            completed_by: String::new(),
            completed_at: None,
            delivered_by: String::new(),
            delivered_at: None,
            updated_at: now,
        });
    }
}

pub async fn save(pool: &PgPool, actor: &Session, job: &Job, new: bool) -> Result<()> {
    ensure!(!job.job_title.trim().is_empty(), "Enter a job name");
    ensure!(
        job.job_title.chars().count() <= 500
            && job.complaint.len() <= 100_000
            && job.internal_notes.len() <= 100_000,
        "Job text is too long"
    );
    let mut tx = transaction(pool, actor).await?;
    if new {
        let inserted = sqlx::query("INSERT INTO service_orders(id,order_no,job_title,complaint,internal_notes,status,received_at,expected_at,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,'received',$6,$7,$8,LOCALTIMESTAMP,LOCALTIMESTAMP) ON CONFLICT DO NOTHING")
            .bind(job.id).bind(&job.order_no).bind(job.job_title.trim()).bind(&job.complaint).bind(&job.internal_notes).bind(job.received_at).bind(job.expected_at).bind(actor.username()).execute(&mut *tx).await?;
        if inserted.rows_affected() == 0 {
            let same: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM service_orders WHERE id=$1 AND order_no=$2 AND job_title=$3 AND complaint=$4 AND internal_notes=$5 AND received_at=$6 AND expected_at IS NOT DISTINCT FROM $7 AND created_by=$8)")
                .bind(job.id).bind(&job.order_no).bind(job.job_title.trim()).bind(&job.complaint).bind(&job.internal_notes).bind(job.received_at).bind(job.expected_at).bind(actor.username()).fetch_one(&mut *tx).await?;
            ensure!(
                same,
                "Job number already exists with different data. Refresh before continuing."
            );
        } else {
            history(&mut tx, job.id, None, "received", "Order created", actor).await?;
        }
    } else {
        let changed=sqlx::query("UPDATE service_orders SET job_title=$1,complaint=$2,internal_notes=$3,expected_at=$4,updated_at=clock_timestamp() WHERE id=$5 AND updated_at=$6 AND status NOT IN ('delivered','cancelled')")
            .bind(job.job_title.trim()).bind(&job.complaint).bind(&job.internal_notes).bind(job.expected_at).bind(job.id).bind(job.updated_at).execute(&mut *tx).await?;
        ensure!(
            changed.rows_affected() == 1,
            "Job changed on another workstation or is closed. Refresh and reopen it."
        );
    }
    tx.commit().await?;
    Ok(())
}
async fn history(
    tx: &mut Transaction<'_, Postgres>,
    id: i32,
    from: Option<&str>,
    to: &str,
    note: &str,
    actor: &Session,
) -> Result<()> {
    sqlx::query("INSERT INTO service_order_status_history(service_order_id,from_status,to_status,note,changed_by,changed_at) VALUES($1,$2,$3,$4,$5,LOCALTIMESTAMP)")
        .bind(id).bind(from).bind(to).bind(note).bind(actor.username()).execute(&mut **tx).await?;
    Ok(())
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Start,
    Complete,
    Collect,
    Cancel,
    Delete,
}
impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Self::Start => "Start Job",
            Self::Complete => "Complete Job",
            Self::Collect => "Mark as Collected",
            Self::Cancel => "Cancel Job",
            Self::Delete => "Delete Job",
        }
    }
    pub fn allowed(self, status: &str) -> bool {
        match self {
            Self::Start => can_start(status),
            Self::Complete => can_complete(status),
            Self::Collect => ready(status),
            Self::Cancel => !matches!(status, "cancelled" | "delivered" | "completed"),
            Self::Delete => status == "cancelled",
        }
    }
}
pub async fn act(
    pool: &PgPool,
    actor: &Session,
    job: &Job,
    action: Action,
    note: &str,
) -> Result<()> {
    let mut tx = transaction(pool, actor).await?;
    if action == Action::Delete {
        actor.authorize(&mut tx, Permission::Manage).await?;
    }
    let (status, owner, version): (String, Option<String>, NaiveDateTime) = sqlx::query_as(
        "SELECT status,started_by,updated_at FROM service_orders WHERE id=$1 FOR UPDATE",
    )
    .bind(job.id)
    .fetch_one(&mut *tx)
    .await?;
    ensure!(
        version == job.updated_at && status == job.status,
        "Job changed on another workstation. Refresh before continuing."
    );
    ensure!(
        action.allowed(&status),
        "This action is not available for this job status"
    );
    if action == Action::Start {
        ensure!(
            owner.as_deref().unwrap_or("").is_empty() || owner.as_deref() == Some(actor.username()),
            "Another staff member has already started this job"
        );
    }
    if action == Action::Delete {
        let linked: bool=sqlx::query_scalar("SELECT sale_id IS NOT NULL OR COALESCE(deposit_amount,0)<>0 OR checkout_started_at IS NOT NULL OR EXISTS(SELECT 1 FROM service_order_payments WHERE service_order_id=$1) FROM service_orders WHERE id=$1").bind(job.id).fetch_one(&mut *tx).await?;
        ensure!(
            !linked,
            "Jobs linked to payments or checkout cannot be deleted"
        );
        sqlx::query("DELETE FROM service_orders WHERE id=$1")
            .bind(job.id)
            .execute(&mut *tx)
            .await?;
    } else {
        let target = match action {
            Action::Start => "in_progress",
            Action::Complete => "ready_for_pickup",
            Action::Collect => "delivered",
            Action::Cancel => "cancelled",
            Action::Delete => unreachable!(),
        };
        sqlx::query("UPDATE service_orders SET status=$2,updated_at=clock_timestamp(),started_by=CASE WHEN $2='in_progress' THEN $3 ELSE started_by END,started_at=CASE WHEN $2='in_progress' THEN COALESCE(started_at,LOCALTIMESTAMP) ELSE started_at END,completed_by=CASE WHEN $2='ready_for_pickup' THEN $3 ELSE completed_by END,completed_at=CASE WHEN $2='ready_for_pickup' THEN LOCALTIMESTAMP ELSE completed_at END,delivered_by=CASE WHEN $2='delivered' THEN $3 ELSE delivered_by END,delivered_at=CASE WHEN $2='delivered' THEN LOCALTIMESTAMP ELSE delivered_at END WHERE id=$1")
            .bind(job.id).bind(target).bind(actor.username()).execute(&mut *tx).await?;
        history(&mut tx, job.id, Some(&status), target, note, actor).await?;
        if action == Action::Complete {
            sqlx::query("INSERT INTO service_order_notifications(service_order_id,event,channel,recipient,message,status,attempts,created_at) SELECT id,'ready_for_pickup','queue',COALESCE(customer_phone,''),COALESCE(NULLIF(customer_name,''),'Customer')||', service order '||order_no||' is ready for pickup.','pending',0,LOCALTIMESTAMP FROM service_orders WHERE id=$1").bind(job.id).execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    Ok(())
}
#[derive(Clone, PartialEq, sqlx::FromRow)]
pub struct History {
    pub to_status: String,
    pub note: String,
    pub changed_by: String,
    pub changed_at: NaiveDateTime,
}
pub async fn history_list(pool: &PgPool, id: i32) -> Result<Vec<History>> {
    Ok(sqlx::query_as("SELECT to_status,COALESCE(note,'') AS note,changed_by,changed_at FROM service_order_status_history WHERE service_order_id=$1 ORDER BY changed_at DESC,id DESC").bind(id).fetch_all(pool).await?)
}

#[derive(Clone, Debug, PartialEq, Default, sqlx::FromRow)]
pub struct Prompt {
    pub id: i32,
    pub title: String,
    pub category: String,
    pub prompt_text: String,
    pub image_data: String,
    pub image_name: String,
    pub sort_order: i32,
    pub active: i32,
    pub updated_at: Option<NaiveDateTime>,
}
pub async fn prompts(pool: &PgPool) -> Result<Vec<Prompt>> {
    Ok(sqlx::query_as("SELECT id,title,COALESCE(category,'') AS category,prompt_text,''::text AS image_data,COALESCE(image_name,'') AS image_name,sort_order,active,updated_at FROM service_order_design_prompts ORDER BY sort_order,title,id").fetch_all(pool).await?)
}
pub async fn prompt_image(pool: &PgPool, id: i32) -> Result<String> {
    Ok(sqlx::query_scalar("SELECT COALESCE(image_data,'') FROM service_order_design_prompts WHERE id=$1").bind(id).fetch_one(pool).await?)
}
pub async fn prompt(pool: &PgPool, id: i32) -> Result<Prompt> {
    Ok(sqlx::query_as("SELECT id,title,COALESCE(category,'') AS category,prompt_text,COALESCE(image_data,'') AS image_data,COALESCE(image_name,'') AS image_name,sort_order,active,updated_at FROM service_order_design_prompts WHERE id=$1").bind(id).fetch_one(pool).await?)
}
pub fn validate_prompt(p: &Prompt) -> Result<()> {
    ensure!(
        !p.title.trim().is_empty() && !p.prompt_text.trim().is_empty(),
        "Title and prompt text are required"
    );
    ensure!(
        p.category.chars().count() <= 120
            && p.image_name.chars().count() <= 255
            && p.sort_order >= 0,
        "Invalid category, image name or sort order"
    );
    ensure!(
        p.image_data.len() <= 5_000_000,
        "Image must be smaller than 3.5 MB"
    );
    if !p.image_data.is_empty() {
        use base64::Engine;
        let (header, encoded) = p.image_data.split_once(',').context("Invalid image")?;
        ensure!(
            matches!(
                header,
                "data:image/png;base64" | "data:image/jpeg;base64" | "data:image/webp;base64"
            ),
            "Choose a PNG, JPEG or WebP image"
        );
        let bytes = base64::engine::general_purpose::STANDARD.decode(encoded)?;
        let png = bytes.starts_with(b"\x89PNG\r\n\x1a\n");
        let jpg = bytes.starts_with(&[255, 216, 255]);
        let webp = bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP");
        ensure!(
            bytes.len() <= 3_500_000
                && ((header.contains("png") && png)
                    || (header.contains("jpeg") && jpg)
                    || (header.contains("webp") && webp)),
            "Invalid or oversized image"
        );
    }
    Ok(())
}
pub async fn save_prompt(pool: &PgPool, actor: &Session, p: &Prompt) -> Result<()> {
    validate_prompt(p)?;
    let mut tx = transaction(pool, actor).await?;
    if p.id == 0 {
        sqlx::query("INSERT INTO service_order_design_prompts(title,category,prompt_text,image_data,image_name,sort_order,active,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,LOCALTIMESTAMP,LOCALTIMESTAMP)")
            .bind(p.title.trim()).bind(p.category.trim()).bind(&p.prompt_text).bind(&p.image_data).bind(&p.image_name).bind(p.sort_order).bind(p.active).execute(&mut *tx).await?;
    } else {
        let result=sqlx::query("UPDATE service_order_design_prompts SET title=$1,category=$2,prompt_text=$3,image_data=$4,image_name=$5,sort_order=$6,active=$7,updated_at=clock_timestamp() WHERE id=$8 AND updated_at IS NOT DISTINCT FROM $9")
            .bind(p.title.trim()).bind(p.category.trim()).bind(&p.prompt_text).bind(&p.image_data).bind(&p.image_name).bind(p.sort_order).bind(p.active).bind(p.id).bind(p.updated_at).execute(&mut *tx).await?;
        ensure!(
            result.rows_affected() == 1,
            "Prompt changed on another workstation. Refresh and reopen it."
        );
    }
    tx.commit().await?;
    Ok(())
}
pub fn render_prompt(text: &str, job: &Job) -> String {
    // One pass prevents job text containing template tokens from being expanded again.
    let values = [
        ("{job_title}", job.job_title.clone()),
        ("{details}", job.complaint.clone()),
        ("{notes}", job.internal_notes.clone()),
        (
            "{appointment}",
            job.expected_at
                .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_default(),
        ),
        ("{status}", status_label(&job.status).to_string()),
    ];
    let mut output = String::new();
    let mut remaining = text;
    while !remaining.is_empty() {
        if let Some((key, value)) = values.iter().find(|(key, _)| remaining.starts_with(key)) {
            output.push_str(value);
            remaining = &remaining[key.len()..];
        } else {
            let c = remaining.chars().next().unwrap();
            output.push(c);
            remaining = &remaining[c.len_utf8()..];
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lifecycle() {
        for state in ["ready", "completed", "ready_for_pickup"] {
            assert!(Action::Collect.allowed(state));
            assert!(!Action::Complete.allowed(state));
        }
        assert!(Action::Start.allowed("received"));
        assert!(!Action::Start.allowed("in_progress"));
        assert!(!Action::Collect.allowed("received"));
        assert!(!Action::Delete.allowed("delivered"));
        assert!(Action::Delete.allowed("cancelled"));
    }
    #[test]
    fn prompt_validation() {
        let mut p = Prompt {
            title: "Poster".into(),
            prompt_text: "Design {job_title}".into(),
            active: 1,
            ..Default::default()
        };
        assert!(validate_prompt(&p).is_ok());
        p.image_data = "data:image/svg+xml;base64,PHN2Zz4=".into();
        assert!(validate_prompt(&p).is_err());
    }
}
