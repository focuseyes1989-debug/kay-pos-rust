use anyhow::{ensure, Result};
use sqlx::PgPool;

#[derive(Clone, Debug, Default, PartialEq, sqlx::FromRow)]
pub struct Supplier {
    pub id: i32,
    pub name: String,
    pub company_name: String,
    pub contact_person: String,
    pub phone: String,
    pub email: String,
    pub address: String,
    pub tax_number: String,
    pub website: String,
    pub payment_terms: String,
    pub bank_account: String,
    pub status: String,
}

pub async fn list(pool: &PgPool) -> Result<Vec<Supplier>> {
    Ok(sqlx::query_as("SELECT id,name,COALESCE(company_name,'') AS company_name,COALESCE(contact_person,'') AS contact_person,COALESCE(phone,'') AS phone,COALESCE(email,'') AS email,COALESCE(address,'') AS address,COALESCE(tax_number,'') AS tax_number,COALESCE(website,'') AS website,COALESCE(payment_terms,'') AS payment_terms,COALESCE(bank_account,'') AS bank_account,COALESCE(status,'Active') AS status FROM suppliers ORDER BY lower(name),id").fetch_all(pool).await?)
}

pub async fn save(pool: &PgPool, s: &Supplier) -> Result<()> {
    ensure!(!s.name.trim().is_empty(), "Supplier name is required");
    ensure!(
        matches!(s.status.as_str(), "Active" | "Inactive"),
        "Select a valid status"
    );
    let sql = if s.id == 0 {
        "INSERT INTO suppliers (name,company_name,contact_person,phone,email,address,tax_number,website,payment_terms,bank_account,status) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)"
    } else {
        "UPDATE suppliers SET name=$1,company_name=$2,contact_person=$3,phone=$4,email=$5,address=$6,tax_number=$7,website=$8,payment_terms=$9,bank_account=$10,status=$11 WHERE id=$12"
    };
    let query = sqlx::query(sql)
        .bind(s.name.trim())
        .bind(s.company_name.trim())
        .bind(s.contact_person.trim())
        .bind(s.phone.trim())
        .bind(s.email.trim())
        .bind(s.address.trim())
        .bind(s.tax_number.trim())
        .bind(s.website.trim())
        .bind(s.payment_terms.trim())
        .bind(s.bank_account.trim())
        .bind(&s.status);
    let result = if s.id == 0 {
        query.execute(pool).await?
    } else {
        query.bind(s.id).execute(pool).await?
    };
    ensure!(
        result.rows_affected() == 1,
        "Supplier no longer exists. Refresh the list."
    );
    Ok(())
}
