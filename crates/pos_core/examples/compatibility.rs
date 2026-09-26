#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pool = pos_core::connect(&pos_core::DatabaseConfig::from_env()?).await?;
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION READ ONLY")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout='15s'")
        .execute(&mut *tx)
        .await?;
    let checks = pos_core::compatibility::inspect(&mut tx).await?;
    let failed = checks.iter().filter(|c| !c.ok).count();
    for c in checks {
        println!(
            "{} | {} | {} | {}",
            if c.ok { "OK" } else { "ACTION" },
            c.area,
            c.object,
            c.detail
        );
    }
    tx.commit().await?;
    anyhow::ensure!(
        failed == 0,
        "{failed} compatibility check(s) need attention"
    );
    Ok(())
}
