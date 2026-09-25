use crate::auth::{Permission, Session};
use anyhow::{ensure, Context, Result};
use chrono::{NaiveDate, NaiveTime};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgConnection, PgPool};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Section {
    Employees,
    Attendance,
    Shifts,
    Assignments,
    Payroll,
    Leave,
    Documents,
    Advances,
    Commission,
    Performance,
    Cash,
}
pub const SECTIONS: &[Section] = &[
    Section::Employees,
    Section::Attendance,
    Section::Shifts,
    Section::Payroll,
    Section::Leave,
    Section::Documents,
    Section::Advances,
    Section::Performance,
    Section::Cash,
];
#[derive(Clone, Copy, PartialEq)]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: &'static str,
    pub default: &'static str,
}
macro_rules! fields { ($(($k:literal,$l:literal,$t:literal,$d:literal)),* $(,)?) => { &[$(Field{key:$k,label:$l,kind:$t,default:$d}),*] }; }
impl Section {
    pub fn title(self) -> &'static str {
        match self {
            Self::Employees => "Employees",
            Self::Attendance => "Attendance",
            Self::Shifts => "Shifts",
            Self::Assignments => "Shift Assignments",
            Self::Payroll => "Payroll",
            Self::Leave => "Leave",
            Self::Documents => "Documents",
            Self::Advances => "Advances & Commission",
            Self::Commission => "Commission Rules",
            Self::Performance => "Performance",
            Self::Cash => "Cash Sessions",
        }
    }
    pub fn table(self) -> &'static str {
        match self {
            Self::Employees => "employees",
            Self::Attendance => "attendance",
            Self::Shifts => "shifts",
            Self::Assignments => "employee_shifts",
            Self::Payroll => "payrolls",
            Self::Leave => "employee_leave",
            Self::Documents => "employee_documents",
            Self::Advances => "salary_advances",
            Self::Commission => "commission_rules",
            Self::Cash => "cash_sessions",
            Self::Performance => "",
        }
    }
    pub fn permission(self) -> &'static str {
        match self {
            Self::Employees => "employees",
            Self::Attendance => "attendance",
            Self::Shifts | Self::Assignments => "shifts",
            Self::Payroll => "payroll",
            Self::Leave => "leave",
            Self::Documents => "employee_documents",
            Self::Advances | Self::Commission => "employee_finance",
            Self::Performance => "employee_performance",
            Self::Cash => "cash_sessions",
        }
    }
    pub fn manage(self) -> String {
        if self == Self::Documents {
            "manage_employees".into()
        } else {
            format!("manage_{}", self.permission())
        }
    }
    pub fn editable(self) -> bool {
        matches!(
            self,
            Self::Employees
                | Self::Attendance
                | Self::Shifts
                | Self::Assignments
                | Self::Commission
        )
    }
    pub fn fields(self) -> &'static [Field] {
        match self {
            Self::Employees => fields![
                ("employee_no", "Employee ID", "text", ""),
                ("full_name", "Full name", "text", ""),
                ("user_id", "POS account", "user", ""),
                ("phone", "Phone", "text", ""),
                ("hire_date", "Hire date", "date", "today"),
                (
                    "employment_status",
                    "Status",
                    "Active|On Leave|Resigned|Inactive",
                    "Active"
                ),
                ("position", "Position", "text", ""),
                ("department", "Department", "text", ""),
                ("branch", "Branch", "text", ""),
                ("date_of_birth", "Date of birth", "optional-date", ""),
                ("national_id", "National ID", "text", ""),
                ("zkteco_user_id", "ZKTeco user ID", "text", ""),
                ("emergency_contact_name", "Emergency contact", "text", ""),
                ("emergency_contact_phone", "Emergency phone", "text", ""),
                ("address", "Address", "memo", ""),
                ("notes", "Notes", "memo", "")
            ],
            Self::Attendance => fields![
                ("employee_id", "Employee", "employee", ""),
                ("attendance_date", "Date", "date", "today"),
                ("check_in", "Check in", "optional-time", ""),
                ("check_out", "Check out", "optional-time", ""),
                (
                    "status",
                    "Status",
                    "Present|Late|Incomplete|Absent|Half-day|Leave",
                    "Present"
                ),
                ("notes", "Notes", "memo", ""),
                ("correction_reason", "Correction reason", "text", "")
            ],
            Self::Shifts => fields![
                ("name", "Shift name", "text", ""),
                ("start_time", "Start", "time", "08:00"),
                ("end_time", "End", "time", "17:00"),
                ("break_minutes", "Break minutes", "integer", "0"),
                ("is_overnight", "Overnight", "bool", "0")
            ],
            Self::Assignments => fields![
                ("employee_id", "Employee", "employee", ""),
                ("shift_id", "Shift", "shift", ""),
                ("effective_from", "Effective from", "date", "today"),
                ("effective_to", "Effective to", "optional-date", ""),
                (
                    "weekly_off_days",
                    "Weekly off days (0=Mon to 6=Sun)",
                    "weekdays",
                    ""
                )
            ],
            Self::Payroll => fields![
                ("employee_id", "Employee", "employee", ""),
                ("period_month", "Month", "month", "month"),
                ("basic_salary", "Basic salary", "number", "0"),
                ("allowance", "Allowance", "number", "0"),
                ("overtime_amount", "Overtime", "number", "0"),
                ("bonus", "Bonus", "number", "0"),
                ("late_deduction", "Late deduction", "number", "0"),
                ("absence_deduction", "Absence deduction", "number", "0"),
                ("advance_deduction", "Advance deduction", "number", "0"),
                ("other_deduction", "Other deduction", "number", "0"),
                ("notes", "Notes", "memo", "")
            ],
            Self::Leave => fields![
                ("employee_id", "Employee", "employee", ""),
                (
                    "leave_type",
                    "Leave type",
                    "Annual|Sick|Unpaid|Emergency",
                    "Annual"
                ),
                ("start_date", "From", "date", "today"),
                ("end_date", "To", "date", "today"),
                ("days", "Days", "number", "1"),
                ("reason", "Reason", "memo", "")
            ],
            Self::Documents => fields![
                ("employee_id", "Employee", "employee", ""),
                (
                    "document_type",
                    "Type",
                    "Contract|National ID|Certificate|License|Other",
                    "Contract"
                ),
                ("document_no", "Document number", "text", ""),
                ("file_path", "File path", "file", ""),
                ("issued_date", "Issued", "optional-date", ""),
                ("expiry_date", "Expiry", "optional-date", ""),
                ("notes", "Notes", "memo", "")
            ],
            Self::Advances => fields![
                ("employee_id", "Employee", "employee", ""),
                ("advance_date", "Date", "date", "today"),
                ("amount", "Amount", "number", "0"),
                ("notes", "Notes", "memo", "")
            ],
            Self::Commission => fields![
                ("employee_id", "Employee", "employee", ""),
                ("rate_percent", "Rate %", "number", "0"),
                ("target_amount", "Target", "number", "0")
            ],
            Self::Cash => fields![
                ("employee_id", "Employee", "employee", ""),
                ("opening_cash", "Opening cash", "number", "0"),
                ("notes", "Notes", "memo", "")
            ],
            Self::Performance => &[],
        }
    }
    pub fn columns(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Employees => &[
                ("employee_no", "Employee ID"),
                ("full_name", "Name"),
                ("position", "Position"),
                ("phone", "Phone"),
                ("department", "Department"),
                ("branch", "Branch"),
                ("hire_date", "Hire date"),
                ("username", "POS Account"),
                ("employment_status", "Status"),
            ],
            Self::Attendance => &[
                ("attendance_date", "Date"),
                ("full_name", "Employee"),
                ("check_in", "In"),
                ("check_out", "Out"),
                ("status", "Status"),
                ("late_minutes", "Late (min)"),
                ("notes", "Notes"),
                ("correction_reason", "Correction reason"),
            ],
            Self::Shifts => &[
                ("name", "Shift"),
                ("start_time", "Start"),
                ("end_time", "End"),
                ("break_minutes", "Break (min)"),
                ("is_overnight", "Overnight"),
            ],
            Self::Assignments => &[
                ("full_name", "Employee"),
                ("shift_name", "Shift"),
                ("effective_from", "From"),
                ("effective_to", "To"),
                ("weekly_off_days", "Weekly off"),
            ],
            Self::Payroll => &[
                ("payroll_no", "Payroll"),
                ("full_name", "Employee"),
                ("period_month", "Month"),
                ("basic_salary", "Basic"),
                ("net_salary", "Net salary"),
                ("status", "Status"),
                ("paid_date", "Paid date"),
            ],
            Self::Leave => &[
                ("full_name", "Employee"),
                ("leave_type", "Type"),
                ("start_date", "From"),
                ("end_date", "To"),
                ("days", "Days"),
                ("reason", "Reason"),
                ("status", "Status"),
                ("review_notes", "Review notes"),
            ],
            Self::Documents => &[
                ("full_name", "Employee"),
                ("document_type", "Type"),
                ("document_no", "Document no."),
                ("issued_date", "Issued"),
                ("expiry_date", "Expiry"),
                ("file_path", "File"),
                ("notes", "Notes"),
            ],
            Self::Advances => &[
                ("full_name", "Employee"),
                ("advance_date", "Date"),
                ("amount", "Amount"),
                ("repaid_amount", "Repaid"),
                ("balance", "Balance"),
                ("status", "Status"),
                ("notes", "Notes"),
            ],
            Self::Commission => &[
                ("full_name", "Employee"),
                ("rate_percent", "Rate %"),
                ("target_amount", "Target"),
                ("active", "Active"),
            ],
            Self::Performance => &[
                ("employee_no", "Employee ID"),
                ("full_name", "Employee"),
                ("branch", "Branch"),
                ("sale_count", "Sales"),
                ("sales_total", "Revenue"),
                ("refund_count", "Refunds"),
                ("discount_total", "Discounts"),
                ("target_amount", "Target"),
                ("commission_rate", "Rate %"),
                ("commission_amount", "Commission"),
            ],
            Self::Cash => &[
                ("full_name", "Employee"),
                ("opened_at", "Opened"),
                ("opening_cash", "Opening cash"),
                ("closed_at", "Closed"),
                ("expected_cash", "Expected"),
                ("actual_cash", "Actual"),
                ("difference", "Difference"),
                ("status", "Status"),
            ],
        }
    }
}

pub fn text(row: &Value, key: &str) -> String {
    match &row[key] {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        v => v.to_string(),
    }
}
fn number(v: &Value, key: &str) -> Result<f64> {
    let n = text(v, key)
        .parse::<f64>()
        .with_context(|| format!("Invalid {key}"))?;
    ensure!(
        n.is_finite() && n >= 0.0 && n <= 999999999999.0,
        "Invalid {key}"
    );
    Ok(n)
}
fn date(value: &str) -> Result<NaiveDate> {
    Ok(NaiveDate::parse_from_str(value, "%Y-%m-%d").context("Date must be YYYY-MM-DD")?)
}
pub fn defaults(section: Section) -> Value {
    let mut v = json!({});
    for f in section.fields() {
        v[f.key] = json!(match f.default {
            "today" => chrono::Local::now().format("%Y-%m-%d").to_string(),
            "month" => chrono::Local::now().format("%Y-%m").to_string(),
            s => s.to_string(),
        });
    }
    v
}

pub fn validate(section: Section, input: &Value) -> Result<Value> {
    let mut out = json!({});
    for f in section.fields() {
        let s = text(input, f.key).trim().to_string();
        ensure!(s.len() <= 4000, "{} is too long", f.label);
        out[f.key] = match f.kind {
            "number" => json!(number(input, f.key)?),
            "integer" | "employee" | "shift" | "user" => {
                if s.is_empty() && f.kind == "user" {
                    Value::Null
                } else {
                    let n = s
                        .parse::<i32>()
                        .with_context(|| format!("Select or enter {}", f.label))?;
                    ensure!(
                        n >= if f.kind == "integer" { 0 } else { 1 },
                        "Invalid {}",
                        f.label
                    );
                    json!(n)
                }
            }
            "bool" => {
                ensure!(s == "0" || s == "1", "Invalid boolean");
                json!(s.parse::<i32>()?)
            }
            "date" => {
                date(&s)?;
                json!(s)
            }
            "optional-date" => {
                if s.is_empty() {
                    Value::Null
                } else {
                    date(&s)?;
                    json!(s)
                }
            }
            "month" => {
                ensure!(s.len() == 7, "Month must be YYYY-MM");
                date(&format!("{s}-01"))?;
                json!(s)
            }
            "time" | "optional-time" => {
                if s.is_empty() && f.kind == "optional-time" {
                    Value::Null
                } else {
                    NaiveTime::parse_from_str(&s, "%H:%M").context("Time must be HH:MM")?;
                    json!(s)
                }
            }
            "weekdays" => {
                ensure!(
                    s.is_empty()
                        || s.split(',')
                            .all(|d| matches!(d, "0" | "1" | "2" | "3" | "4" | "5" | "6")),
                    "Invalid weekly off days"
                );
                json!(s)
            }
            opts if opts.contains('|') => {
                ensure!(opts.split('|').any(|x| x == s), "Invalid {}", f.label);
                json!(s)
            }
            _ => json!(s),
        };
    }
    match section {
        Section::Employees => ensure!(!text(&out, "full_name").is_empty(), "Full name is required"),
        Section::Attendance => ensure!(
            !text(&out, "correction_reason").is_empty(),
            "Correction reason is required"
        ),
        Section::Shifts => {
            ensure!(!text(&out, "name").is_empty(), "Shift name is required");
            ensure!(
                number(&out, "break_minutes")? < 1440.0,
                "Break must be less than 24 hours"
            );
        }
        Section::Leave => {
            let days = number(&out, "days")?;
            let span =
                (date(&text(&out, "end_date"))? - date(&text(&out, "start_date"))?).num_days() + 1;
            ensure!(
                days > 0.0 && days <= span as f64,
                "Leave days exceed date range"
            );
        }
        Section::Advances => ensure!(number(&out, "amount")? > 0.0, "Advance must be positive"),
        Section::Commission => ensure!(
            number(&out, "rate_percent")? <= 100.0,
            "Rate cannot exceed 100%"
        ),
        Section::Payroll => {
            let net = payroll_net(&out)?;
            ensure!(net >= 0.0, "Deductions exceed salary");
            out["net_salary"] = json!(net);
        }
        _ => (),
    }
    for (start, end) in [
        ("issued_date", "expiry_date"),
        ("effective_from", "effective_to"),
    ] {
        if !text(&out, end).is_empty() && !text(&out, start).is_empty() {
            ensure!(
                text(&out, end) >= text(&out, start),
                "End date precedes start date"
            );
        }
    }
    Ok(out)
}
fn payroll_net(v: &Value) -> Result<f64> {
    let mut net = 0.0;
    for key in ["basic_salary", "allowance", "overtime_amount", "bonus"] {
        net += number(v, key)?;
    }
    for key in [
        "late_deduction",
        "absence_deduction",
        "advance_deduction",
        "other_deduction",
    ] {
        net -= number(v, key)?;
    }
    Ok((net * 100.0).round() / 100.0)
}

pub async fn permissions(conn: &mut PgConnection, actor: &Session) -> Result<Vec<String>> {
    actor.authorize(conn, Permission::Sell).await?;
    let (user,role,force):(String,String,i32)=sqlx::query_as("SELECT COALESCE(u.permissions,''),COALESCE(r.permissions,''),COALESCE(u.force_password_change,0) FROM users u LEFT JOIN user_roles r ON lower(r.name)=lower(u.role) WHERE u.username=$1")
        .bind(actor.username()).fetch_one(conn).await?;
    ensure!(force == 0, "Change your password in Main POS first");
    if actor.allows(Permission::Admin) {
        return Ok(vec!["*".into()]);
    }
    Ok(user
        .split(',')
        .chain(role.split(','))
        .map(|s| s.trim().to_string())
        .collect())
}
pub fn has(perms: &[String], key: &str) -> bool {
    perms.iter().any(|s| s == "*" || s == key)
}
async fn authorize(
    conn: &mut PgConnection,
    actor: &Session,
    section: Section,
    write: bool,
) -> Result<()> {
    let p = permissions(conn, actor).await?;
    ensure!(
        has(&p, "employees")
            && has(&p, section.permission())
            && (!write || has(&p, &section.manage())),
        "Permission denied for {}",
        section.title()
    );
    Ok(())
}

#[derive(Clone, PartialEq)]
pub struct Page {
    pub rows: Vec<Value>,
    pub employees: Vec<Value>,
    pub users: Vec<Value>,
    pub shifts: Vec<Value>,
    pub permissions: Vec<String>,
    pub summary: Value,
}
async fn json_rows(conn: &mut PgConnection, sql: &str) -> Result<Vec<Value>> {
    sqlx::query_scalar::<_, String>(sql)
        .fetch_all(conn)
        .await?
        .into_iter()
        .map(|s| Ok(serde_json::from_str(&s)?))
        .collect()
}
fn record_sql(section: Section) -> String {
    if section == Section::Employees {
        "(to_jsonb(t)-'photo_data') || jsonb_build_object('photo_base64',encode(t.photo_data,'base64'))".into()
    } else {
        "to_jsonb(t)".into()
    }
}
pub async fn list(
    pool: &PgPool,
    actor: &Session,
    section: Section,
    from: &str,
    to: &str,
) -> Result<Page> {
    date(from)?;
    date(to)?;
    ensure!(from <= to, "Invalid date range");
    let mut conn = pool.acquire().await?;
    authorize(&mut conn, actor, section, false).await?;
    let perms = permissions(&mut conn, actor).await?;
    let mut rows = if section == Section::Performance {
        let query="SELECT row_to_json(q)::text FROM (SELECT e.id,e.employee_no,e.full_name,e.branch,COUNT(s.id) AS sale_count,COALESCE(SUM(CASE WHEN s.status='completed' THEN s.total ELSE 0 END),0)::float8 AS sales_total,COUNT(s.id) FILTER(WHERE s.status='refunded') AS refund_count,COALESCE(SUM(s.discount_amount),0)::float8 AS discount_total,COALESCE(cr.rate_percent,0)::float8 AS commission_rate,COALESCE(cr.target_amount,0)::float8 AS target_amount FROM employees e LEFT JOIN users u ON u.id=e.user_id LEFT JOIN sales s ON s.created_by=u.username AND s.created_at::date BETWEEN $1::date AND $2::date LEFT JOIN commission_rules cr ON cr.employee_id=e.id AND cr.active=1 WHERE e.employment_status='Active' GROUP BY e.id,cr.rate_percent,cr.target_amount ORDER BY sales_total DESC) q";
        let raw = sqlx::query_scalar::<_, String>(query)
            .bind(from)
            .bind(to)
            .fetch_all(&mut *conn)
            .await?;
        let mut rows = Vec::new();
        for s in raw {
            let mut v: Value = serde_json::from_str(&s)?;
            let sales = number(&v, "sales_total")?;
            let rate = number(&v, "commission_rate")?;
            v["commission_amount"] = json!(if sales >= number(&v, "target_amount")? {
                sales * rate / 100.0
            } else {
                0.0
            });
            rows.push(v);
        }
        rows
    } else {
        let filter = match section {
            Section::Attendance => "t.attendance_date BETWEEN $1 AND $2",
            Section::Payroll => "t.period_month BETWEEN left($1,7) AND left($2,7)",
            Section::Leave => "t.start_date <= $2 AND t.end_date >= $1",
            Section::Advances => "t.advance_date BETWEEN $1 AND $2",
            Section::Cash => "t.opened_at::date BETWEEN $1::date AND $2::date",
            _ => "$1::text IS NOT NULL AND $2::text IS NOT NULL",
        };
        let query = format!(
            "SELECT ({})::text FROM {} t WHERE {filter} ORDER BY t.id DESC LIMIT 10001",
            record_sql(section),
            section.table()
        );
        let raw = sqlx::query_scalar::<_, String>(&query)
            .bind(from)
            .bind(to)
            .fetch_all(&mut *conn)
            .await?;
        ensure!(
            raw.len() <= 10000,
            "More than 10,000 records; narrow the date range"
        );
        raw.into_iter()
            .map(|s| Ok(serde_json::from_str(&s)?))
            .collect::<Result<Vec<Value>>>()?
    };
    let employees=json_rows(&mut conn,"SELECT json_build_object('id',id,'employee_no',employee_no,'full_name',full_name,'employment_status',employment_status)::text FROM employees ORDER BY full_name").await?;
    let users = if has(&perms, "manage_employees") {
        json_rows(&mut conn,"SELECT json_build_object('id',id,'username',username)::text FROM users WHERE is_active=1 ORDER BY username").await?
    } else {
        vec![]
    };
    let shifts = if matches!(section, Section::Shifts | Section::Assignments) {
        json_rows(&mut conn,"SELECT json_build_object('id',id,'name',name)::text FROM shifts WHERE is_active=1 ORDER BY name").await?
    } else {
        vec![]
    };
    for row in &mut rows {
        let revision = crate::auth::fingerprint(row.to_string().as_bytes());
        row["revision"] = json!(revision);
        if let Some(emp) = employees.iter().find(|e| e["id"] == row["employee_id"]) {
            row["full_name"] = emp["full_name"].clone();
            row["employee_no"] = emp["employee_no"].clone();
        }
        if let Some(user) = users.iter().find(|u| u["id"] == row["user_id"]) {
            row["username"] = user["username"].clone();
        }
        if let Some(shift) = shifts.iter().find(|s| s["id"] == row["shift_id"]) {
            row["shift_name"] = shift["name"].clone();
        }
        if section == Section::Advances {
            row["balance"] = json!(number(row, "amount")? - number(row, "repaid_amount")?);
        }
    }
    let mut summary = json!({});
    summary["active"] = json!(employees
        .iter()
        .filter(|e| text(e, "employment_status") == "Active")
        .count());
    if has(&perms, "leave") {
        summary["pending_leave"] = json!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM employee_leave WHERE status='Pending'"
            )
            .fetch_one(&mut *conn)
            .await?
        );
    }
    if has(&perms, "employee_documents") {
        summary["expiring"]=json!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM employee_documents WHERE NULLIF(expiry_date,'') IS NOT NULL AND expiry_date <= to_char(CURRENT_DATE+30,'YYYY-MM-DD')").fetch_one(&mut *conn).await?);
    }
    if has(&perms, "employee_finance") {
        summary["advances"]=json!(sqlx::query_scalar::<_,f64>("SELECT COALESCE(SUM(amount-COALESCE(repaid_amount,0)),0)::float8 FROM salary_advances WHERE status='Outstanding'").fetch_one(&mut *conn).await?);
    }
    Ok(Page {
        rows,
        employees,
        users,
        shifts,
        permissions: perms,
        summary,
    })
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Command {
    pub request_id: String,
    pub section: Section,
    pub action: String,
    pub id: Option<i32>,
    pub revision: String,
    pub values: Value,
}

pub fn uncertain(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<sqlx::Error>(),
            Some(
                sqlx::Error::Io(_)
                    | sqlx::Error::Tls(_)
                    | sqlx::Error::Protocol(_)
                    | sqlx::Error::WorkerCrashed
            )
        )
    })
}

pub async fn execute(pool: &PgPool, actor: &Session, cmd: &Command) -> Result<i32> {
    ensure!(cmd.section != Section::Performance, "Report is read only");
    ensure!(
        !cmd.request_id.is_empty() && cmd.request_id.len() < 100,
        "Invalid request ID"
    );
    let mut tx = pool.begin().await?;
    authorize(&mut tx, actor, cmd.section, true).await?;
    let user: i32 = sqlx::query_scalar("SELECT id FROM users WHERE username=$1")
        .bind(actor.username())
        .fetch_one(&mut *tx)
        .await?;
    let hash = crate::auth::fingerprint(serde_json::to_string(cmd)?.as_bytes());
    // A separate ledger keeps uncertain network retries from repeating financial writes.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(&cmd.request_id)
        .execute(&mut *tx)
        .await?;
    let existing:Option<(i32,String,i32)>=sqlx::query_as("SELECT user_id,payload_hash,record_id FROM rust_employee_requests WHERE request_id=$1")
        .bind(&cmd.request_id).fetch_optional(&mut *tx).await.context("Employee request ledger unavailable. Apply migrations/003_employee_requests.sql on the server")?;
    if let Some((owner, fingerprint, id)) = existing {
        ensure!(
            owner == user && fingerprint == hash,
            "Request ID already used for a different operation"
        );
        return Ok(id);
    }
    // Serialize employee commands; legacy Main POS also takes normal row/table write locks.
    sqlx::query(&format!(
        "LOCK TABLE {} IN SHARE ROW EXCLUSIVE MODE",
        cmd.section.table()
    ))
    .execute(&mut *tx)
    .await?;
    let old = if let Some(id) = cmd.id {
        let raw: String = sqlx::query_scalar(&format!(
            "SELECT ({})::text FROM {} t WHERE id=$1 FOR UPDATE",
            record_sql(cmd.section),
            cmd.section.table()
        ))
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        let v: Value = serde_json::from_str(&raw)?;
        ensure!(
            crate::auth::fingerprint(v.to_string().as_bytes()) == cmd.revision,
            "Record changed. Refresh before editing"
        );
        Some(v)
    } else {
        None
    };
    let id = write_command(&mut tx, actor, user, cmd, old.as_ref()).await?;
    sqlx::query("INSERT INTO rust_employee_requests(request_id,user_id,operation,payload_hash,record_id) VALUES($1,$2,$3,$4,$5)")
        .bind(&cmd.request_id).bind(user).bind(format!("{:?}.{}",cmd.section,cmd.action)).bind(hash).bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(id)
}

async fn write_record(
    conn: &mut PgConnection,
    section: Section,
    id: Option<i32>,
    values: &Value,
) -> Result<i32> {
    let keys = values
        .as_object()
        .context("Invalid record")?
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    // All column names come from validate() or explicitly assigned server-side fields.
    let sql = if id.is_some() {
        format!("UPDATE {} t SET ({})=(SELECT {} FROM jsonb_populate_record(NULL::{},$1::jsonb)) WHERE id=$2 RETURNING id",section.table(),keys.join(","),keys.join(","),section.table())
    } else {
        format!("INSERT INTO {} ({}) SELECT {} FROM jsonb_populate_record(NULL::{},$1::jsonb) WHERE $2::integer IS NULL RETURNING id",section.table(),keys.join(","),keys.join(","),section.table())
    };
    Ok(sqlx::query_scalar(&sql)
        .bind(values.to_string())
        .bind(id)
        .fetch_one(conn)
        .await?)
}

async fn next_number(conn: &mut PgConnection, section: Section) -> Result<String> {
    let (column, prefix) = if section == Section::Employees {
        ("employee_no", "EMP")
    } else {
        ("payroll_no", "PAY")
    };
    let names: Vec<String> =
        sqlx::query_scalar(&format!("SELECT {column} FROM {}", section.table()))
            .fetch_all(conn)
            .await?;
    let n = names
        .iter()
        .filter_map(|s| s.strip_prefix(&format!("{prefix}-"))?.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .context("Number range exhausted")?;
    Ok(format!("{prefix}-{n:04}"))
}

async fn write_command(
    conn: &mut PgConnection,
    actor: &Session,
    user: i32,
    cmd: &Command,
    old: Option<&Value>,
) -> Result<i32> {
    let section = cmd.section;
    if cmd.action == "save" {
        ensure!(
            cmd.id.is_none() || section.editable(),
            "This record cannot be edited"
        );
        let mut v = validate(section, &cmd.values)?;
        if let Some(id) = v["employee_id"].as_i64() {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM employees WHERE id=$1)")
                    .bind(id as i32)
                    .fetch_one(&mut *conn)
                    .await?;
            ensure!(exists, "Employee no longer exists");
        }
        match section {
            Section::Employees => {
                if text(&v, "employee_no").is_empty() {
                    ensure!(cmd.id.is_none(), "Employee number is required");
                    v["employee_no"] = json!(next_number(conn, section).await?);
                }
                v["updated_at"] = json!(chrono::Local::now().naive_local().to_string());
            }
            Section::Attendance => {
                v["corrected_by"] = json!(user);
                v["updated_at"] = json!(chrono::Local::now().naive_local().to_string());
                let late = attendance_late(conn, &v).await?;
                v["late_minutes"] = json!(late);
            }
            Section::Assignments => {
                let valid: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM shifts WHERE id=$1 AND is_active=1)",
                )
                .bind(v["shift_id"].as_i64().unwrap() as i32)
                .fetch_one(&mut *conn)
                .await?;
                ensure!(valid, "Select an active shift");
            }
            Section::Payroll => {
                v["payroll_no"] = json!(next_number(conn, section).await?);
                v["created_by"] = json!(user);
                v["status"] = json!("Draft");
            }
            Section::Advances => {
                v["created_by"] = json!(user);
                v["repaid_amount"] = json!(0);
                v["status"] = json!("Outstanding");
            }
            Section::Commission => {
                if let Some(old) = old {
                    ensure!(
                        old["employee_id"] == v["employee_id"],
                        "Employee cannot change on a commission rule"
                    );
                }
                v["active"] = json!(1);
                v["updated_at"] = json!(chrono::Local::now().naive_local().to_string());
            }
            Section::Cash => {
                let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cash_sessions WHERE employee_id=$1 AND status='Open')").bind(v["employee_id"].as_i64().unwrap() as i32).fetch_one(&mut *conn).await?;
                ensure!(!exists, "Employee already has an open cash session");
                v["opened_by"] = json!(user);
                v["opened_at"] = json!(chrono::Local::now().naive_local().to_string());
                v["status"] = json!("Open");
            }
            _ => (),
        }
        let id = write_record(conn, section, cmd.id, &v).await?;
        if section == Section::Employees {
            if let Some(photo) = cmd.values.get("photo_base64") {
                use base64::Engine;
                let bytes = base64::engine::general_purpose::STANDARD.decode(
                    photo
                        .as_str()
                        .unwrap_or_default()
                        .split_whitespace()
                        .collect::<String>(),
                )?;
                ensure!(bytes.len() <= 2 * 1024 * 1024, "Photo must be at most 2 MB");
                ensure!(
                    bytes.is_empty()
                        || bytes.starts_with(b"\x89PNG\r\n\x1a\n")
                        || bytes.starts_with(&[255, 216, 255]),
                    "Use PNG or JPEG photo"
                );
                sqlx::query("UPDATE employees SET photo_data=$1,photo_path='',updated_at=CURRENT_TIMESTAMP WHERE id=$2").bind(if bytes.is_empty(){None}else{Some(bytes)}).bind(id).execute(&mut *conn).await?;
            }
        }
        if matches!(section, Section::Shifts | Section::Assignments) {
            reclassify(conn).await?;
        }
        return Ok(id);
    }
    let old = old.context("Select a record")?;
    let id = cmd.id.context("Select a record")?;
    let values = &cmd.values;
    match (section, cmd.action.as_str()) {
        (Section::Assignments, "delete") => {
            sqlx::query("DELETE FROM employee_shifts WHERE id=$1")
                .bind(id)
                .execute(&mut *conn)
                .await?;
            reclassify(conn).await?;
        }
        (Section::Leave, "review") => {
            let status = text(values, "status");
            let current = text(old, "status");
            ensure!(
                (current == "Pending"
                    && matches!(status.as_str(), "Approved" | "Rejected" | "Cancelled"))
                    || (current == "Approved" && status == "Cancelled"),
                "Leave already reviewed"
            );
            sqlx::query("UPDATE employee_leave SET status=$1,reviewed_by=$2,reviewed_at=CURRENT_TIMESTAMP,review_notes=$3 WHERE id=$4").bind(status).bind(user).bind(text(values,"review_notes")).bind(id).execute(&mut *conn).await?;
            reclassify(conn).await?;
        }
        (Section::Advances, "repay") => {
            let amount = number(values, "amount")?;
            let repaid = number(old, "repaid_amount")?;
            let total = number(old, "amount")?;
            ensure!(
                amount > 0.0 && amount <= total - repaid,
                "Repayment exceeds balance or is zero"
            );
            sqlx::query("UPDATE salary_advances SET repaid_amount=$1,status=$2 WHERE id=$3")
                .bind(repaid + amount)
                .bind(if repaid + amount >= total {
                    "Repaid"
                } else {
                    "Outstanding"
                })
                .bind(id)
                .execute(&mut *conn)
                .await?;
        }
        (Section::Payroll, "pay") => {
            ensure!(
                text(old, "status") == "Draft",
                "Only draft payroll can be paid"
            );
            let day = text(values, "paid_date");
            date(&day)?;
            let method = text(values, "payment_method");
            ensure!(!method.trim().is_empty(), "Payment method required");
            let salary = number(old, "net_salary")?;
            let name: String = sqlx::query_scalar("SELECT full_name FROM employees WHERE id=$1")
                .bind(old["employee_id"].as_i64().context("Missing employee")? as i32)
                .fetch_one(&mut *conn)
                .await?;
            let expense:i32=sqlx::query_scalar("INSERT INTO expenses(expense_no,category,description,amount,expense_date,payment_method,created_by,notes) VALUES($1,'Salaries',$2,$3,$4,$5,$6,$7) RETURNING id")
                .bind(format!("SAL-{}",text(old,"payroll_no"))).bind(format!("Salary - {name} ({})",text(old,"period_month"))).bind(salary).bind(&day).bind(&method).bind(actor.username()).bind(text(old,"payroll_no")).fetch_one(&mut *conn).await?;
            sqlx::query("UPDATE payrolls SET status='Paid',paid_date=$1,payment_method=$2,expense_id=$3 WHERE id=$4").bind(day).bind(method).bind(expense).bind(id).execute(&mut *conn).await?;
        }
        (Section::Cash, "close") => {
            ensure!(text(old, "status") == "Open", "Session already closed");
            let actual = number(values, "actual_cash")?;
            let username:Option<String>=sqlx::query_scalar("SELECT u.username FROM employees e LEFT JOIN users u ON u.id=e.user_id WHERE e.id=$1").bind(old["employee_id"].as_i64().context("Missing employee")? as i32).fetch_one(&mut *conn).await?;
            ensure!(
                username.is_some(),
                "Employee needs a linked POS account before cash reconciliation"
            );
            let cash:f64=sqlx::query_scalar("SELECT COALESCE(SUM(total),0)::float8 FROM sales WHERE created_by=$1 AND created_at >= $2::timestamp AND created_at<=CURRENT_TIMESTAMP AND status='completed' AND LOWER(COALESCE(payment_type,'cash'))='cash'").bind(username).bind(text(old,"opened_at")).fetch_one(&mut *conn).await?;
            let expected = number(old, "opening_cash")? + cash;
            sqlx::query("UPDATE cash_sessions SET closed_at=CURRENT_TIMESTAMP,expected_cash=$1,actual_cash=$2,difference=$3,status='Closed',closed_by=$4 WHERE id=$5").bind(expected).bind(actual).bind(actual-expected).bind(user).bind(id).execute(&mut *conn).await?;
        }
        _ => anyhow::bail!("Unsupported operation"),
    }
    Ok(id)
}

async fn attendance_late(conn: &mut PgConnection, v: &Value) -> Result<i32> {
    if text(v, "check_in").is_empty() {
        return Ok(0);
    }
    let start:Option<String>=sqlx::query_scalar("SELECT s.start_time FROM employee_shifts a JOIN shifts s ON s.id=a.shift_id WHERE a.employee_id=$1 AND a.effective_from<=$2 AND (a.effective_to IS NULL OR a.effective_to='' OR a.effective_to>=$2) ORDER BY a.effective_from DESC,a.id DESC LIMIT 1")
        .bind(v["employee_id"].as_i64().context("Missing employee")? as i32).bind(text(v,"attendance_date")).fetch_optional(conn).await?;
    if let Some(start) = start {
        let start = parse_time(&start)?;
        let check = parse_time(&text(v, "check_in"))?;
        Ok((check - start).num_minutes().max(0) as i32)
    } else {
        Ok(0)
    }
}
fn parse_time(value: &str) -> Result<NaiveTime> {
    Ok(NaiveTime::parse_from_str(value, "%H:%M")
        .or_else(|_| NaiveTime::parse_from_str(value, "%H:%M:%S"))?)
}
async fn reclassify(conn: &mut PgConnection) -> Result<()> {
    let rows=json_rows(conn,"SELECT to_jsonb(a)::text FROM attendance a WHERE COALESCE(correction_reason,'')='' ORDER BY id FOR UPDATE").await?;
    for v in rows {
        let late = attendance_late(conn, &v).await?;
        let leave:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM employee_leave WHERE employee_id=$1 AND status='Approved' AND $2 BETWEEN start_date AND end_date)").bind(v["employee_id"].as_i64().context("Missing employee")? as i32).bind(text(&v,"attendance_date")).fetch_one(&mut *conn).await?;
        let (cin, cout) = (text(&v, "check_in"), text(&v, "check_out"));
        let status = if leave {
            "Leave"
        } else if cin.is_empty() && cout.is_empty() {
            "Absent"
        } else if cin.is_empty() || cout.is_empty() || cin == cout {
            "Incomplete"
        } else if late > 0 {
            "Late"
        } else {
            "Present"
        };
        sqlx::query("UPDATE attendance SET status=$1,late_minutes=$2,updated_at=CURRENT_TIMESTAMP WHERE id=$3").bind(status).bind(if status=="Late"{late}else{0}).bind(v["id"].as_i64().unwrap() as i32).execute(&mut *conn).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_employee_and_payroll() {
        let mut e = defaults(Section::Employees);
        assert!(validate(Section::Employees, &e).is_err());
        e["full_name"] = json!("Employee");
        assert!(validate(Section::Employees, &e).is_ok());
        let mut p = defaults(Section::Payroll);
        p["employee_id"] = json!(1);
        p["basic_salary"] = json!(100);
        p["other_deduction"] = json!(101);
        assert!(validate(Section::Payroll, &p).is_err());
        p["other_deduction"] = json!(10);
        assert_eq!(
            validate(Section::Payroll, &p).unwrap()["net_salary"],
            json!(90.0)
        );
    }
    #[test]
    fn validates_leave_dates_and_amounts() {
        let mut v = defaults(Section::Leave);
        v["employee_id"] = json!(1);
        v["days"] = json!(2);
        assert!(validate(Section::Leave, &v).is_err());
        v["days"] = json!(0.5);
        assert!(validate(Section::Leave, &v).is_ok());
        let mut c = defaults(Section::Commission);
        c["employee_id"] = json!(1);
        c["rate_percent"] = json!(101);
        assert!(validate(Section::Commission, &c).is_err());
        c["rate_percent"] = json!("NaN");
        assert!(validate(Section::Commission, &c).is_err());
    }
}
