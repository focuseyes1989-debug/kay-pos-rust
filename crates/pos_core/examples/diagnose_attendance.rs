use anyhow::Result;
#[tokio::main]
async fn main()->Result<()> {
 let c:serde_json::Value=serde_json::from_slice(&std::fs::read("kay-pos-db.json")?)?;
 let options=sqlx::postgres::PgConnectOptions::new().host(c["host"].as_str().unwrap()).port(c["port"].as_str().unwrap_or("5432").parse()?).database(c["database"].as_str().unwrap()).username(c["username"].as_str().unwrap()).password(c["password"].as_str().unwrap());
 let pool=sqlx::postgres::PgPoolOptions::new().connect_with(options).await?;
 let (ip,port,key):(String,i32,i32)=sqlx::query_as("SELECT ip_address,COALESCE(port,4370),COALESCE(comm_key,0) FROM zkteco_devices WHERE id=1").fetch_one(&pool).await?;
 let mapped:Vec<String>=sqlx::query_scalar("SELECT m.device_user_id FROM zkteco_employee_mappings m JOIN employees e ON e.id=m.employee_id WHERE m.device_id=1 AND e.employment_status='Active'").fetch_all(&pool).await?;
 let mut zk=rustzk::ZK::new(&ip,port as u16);zk.set_password(key as u32);zk.set_timeout(std::time::Duration::from_secs(15));
 zk.connect(rustzk::ZKProtocol::UDP)?;
 let result=(||->Result<()>{
  println!("Device time: {}",zk.get_time()?);
  let users=zk.get_users()?;println!("Users: {}",users.len());
  let logs=zk.get_attendance()?;
  let matched:Vec<_>=logs.iter().filter(|l|mapped.iter().any(|id|id==l.user_id())).collect();
  println!("Records: {}; active employee punches: {}; unmapped: {}",logs.len(),matched.len(),logs.len()-matched.len());
  let mut days=std::collections::BTreeMap::new();
  for l in matched {if l.timestamp().date().to_string()>="2026-10-06".to_owned() && l.timestamp().date().to_string()<="2026-10-10".to_owned(){*days.entry(l.timestamp().date()).or_insert(0)+=1;}}
  println!("Recent mapped punches: {days:?}");Ok(())
 })();let _=zk.disconnect();result
}
