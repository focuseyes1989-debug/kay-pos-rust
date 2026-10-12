//! Backend-neutral execution. PostgreSQL statements and typed arguments pass through unchanged.
//! SQLite has a separate, parsed dialect conversion and serialized write transactions.
pub use sqlx::Error;
use sqlx::{Row as _, ValueRef as _};
use std::{
    future::Future,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    pin::Pin,
};
pub use storage_derive::FromRow;
pub type PgPool = Pool;
pub type PgConnection = Connection;
pub type Postgres = Backend;
pub struct Backend;
#[derive(Clone)]
pub enum Pool {
    Postgres(sqlx::PgPool),
    Sqlite(sqlx::SqlitePool),
}
impl From<sqlx::PgPool> for Pool {
    fn from(p: sqlx::PgPool) -> Self {
        Self::Postgres(p)
    }
}
impl Pool {
    pub fn is_sqlite(&self) -> bool {
        matches!(self, Self::Sqlite(_))
    }
    pub fn is_closed(&self) -> bool {
        match self {
            Self::Postgres(p) => p.is_closed(),
            Self::Sqlite(p) => p.is_closed(),
        }
    }
    pub async fn close(&self) {
        match self {
            Self::Postgres(p) => p.close().await,
            Self::Sqlite(p) => p.close().await,
        }
    }
    pub async fn acquire(&self) -> Result<Box<Connection>, Error> {
        Ok(Box::new(match self {
            Self::Postgres(p) => Connection::Postgres(PgSession::Lease(p.acquire().await?)),
            Self::Sqlite(p) => Connection::Sqlite(SqliteSession::Lease(p.acquire().await?, false)),
        }))
    }
    pub async fn begin(&self) -> Result<Transaction<'static, Backend>, Error> {
        Ok(Transaction {
            conn: match self {
                Self::Postgres(p) => Connection::Postgres(PgSession::Transaction(p.begin().await?)),
                Self::Sqlite(p) => Connection::Sqlite(SqliteSession::Transaction(
                    p.begin_with("BEGIN IMMEDIATE").await?,
                    false,
                )),
            },
            marker: PhantomData,
        })
    }
}
pub enum PgSession {
    Lease(sqlx::pool::PoolConnection<sqlx::Postgres>),
    Transaction(sqlx::Transaction<'static, sqlx::Postgres>),
}
pub enum SqliteSession {
    Lease(sqlx::pool::PoolConnection<sqlx::Sqlite>, bool),
    Transaction(sqlx::Transaction<'static, sqlx::Sqlite>, bool),
}
pub enum Connection {
    Postgres(PgSession),
    Sqlite(SqliteSession),
}
impl Connection {
    pub(crate) fn postgres_connection(&mut self) -> anyhow::Result<&mut ::sqlx::PgConnection> {
        match self { Self::Postgres(session) => Ok(session.connection()), _ => anyhow::bail!("PostgreSQL source required") }
    }
    pub fn is_sqlite(&self) -> bool {
        matches!(self, Self::Sqlite(_))
    }
}
impl PgSession {
    fn connection(&mut self) -> &mut sqlx::PgConnection {
        match self {
            Self::Lease(c) => c,
            Self::Transaction(c) => c,
        }
    }
}
impl SqliteSession {
    fn connection(&mut self) -> &mut sqlx::SqliteConnection {
        match self {
            Self::Lease(c, _) => c,
            Self::Transaction(c, _) => c,
        }
    }
    fn read_only(&self) -> bool {
        match self {
            Self::Lease(_, v) | Self::Transaction(_, v) => *v,
        }
    }
    fn set_read_only(&mut self) {
        match self {
            Self::Lease(_, v) | Self::Transaction(_, v) => *v = true,
        }
    }
    fn validate(&mut self, sql: &str) -> Result<(), Error> {
        if sql.trim().to_uppercase().starts_with("SET TRANSACTION ")
            && sql.to_uppercase().contains("READ ONLY")
        {
            self.set_read_only();
            return Ok(());
        }
        if self.read_only() && !crate::sqlite_dialect::is_read_only(sql) {
            return Err(Error::Protocol(
                "Cannot write in a read-only standalone transaction".into(),
            ));
        }
        Ok(())
    }
}
pub struct Transaction<'a, DB> {
    conn: Connection,
    marker: PhantomData<&'a DB>,
}
impl<DB> Deref for Transaction<'_, DB> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        &self.conn
    }
}
impl<DB> DerefMut for Transaction<'_, DB> {
    fn deref_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}
impl<DB> Transaction<'_, DB> {
    pub async fn commit(self) -> Result<(), Error> {
        match self.conn {
            Connection::Postgres(PgSession::Transaction(t)) => t.commit().await,
            Connection::Sqlite(SqliteSession::Transaction(t, _)) => t.commit().await,
            _ => unreachable!(),
        }
    }
    pub async fn rollback(self) -> Result<(), Error> {
        match self.conn {
            Connection::Postgres(PgSession::Transaction(t)) => t.rollback().await,
            Connection::Sqlite(SqliteSession::Transaction(t, _)) => t.rollback().await,
            _ => unreachable!(),
        }
    }
}
#[derive(Clone)]
pub enum Row {
    Postgres(std::sync::Arc<sqlx::postgres::PgRow>),
    Sqlite(std::sync::Arc<sqlx::sqlite::SqliteRow>),
}
impl Row {
    pub fn decode<T: DecodeValue>(&self, index: usize) -> Result<T, Error> {
        T::decode(self, index)
    }
    pub fn decode_named<T: DecodeValue>(&self, name: &str) -> Result<T, Error> {
        let index = match self {
            Self::Postgres(r) => r
                .columns()
                .iter()
                .position(|c| sqlx::Column::name(c) == name),
            Self::Sqlite(r) => r
                .columns()
                .iter()
                .position(|c| sqlx::Column::name(c) == name),
        }
        .ok_or_else(|| Error::ColumnNotFound(name.into()))?;
        self.decode(index)
    }
    fn is_null(&self, index: usize) -> Result<bool, Error> {
        Ok(match self {
            Self::Postgres(r) => r.try_get_raw(index)?.is_null(),
            Self::Sqlite(r) => r.try_get_raw(index)?.is_null(),
        })
    }
}
pub trait DecodeValue: Sized {
    fn decode(row: &Row, index: usize) -> Result<Self, Error>;
}
macro_rules! decode {($($t:ty),*)=>{$(impl DecodeValue for $t {fn decode(row:&Row,index:usize)->Result<Self,Error>{match row {Row::Postgres(r)=>r.try_get(index),Row::Sqlite(r)=>r.try_get(index)}}})*}}
decode!(
    i32,
    i64,
    f64,
    bool,
    String,
    Vec<u8>,
    chrono::NaiveDate,
    chrono::NaiveDateTime,
    chrono::DateTime<chrono::Utc>
);
impl<T: DecodeValue> DecodeValue for Option<T> {
    fn decode(row: &Row, i: usize) -> Result<Self, Error> {
        if row.is_null(i)? {
            Ok(None)
        } else {
            T::decode(row, i).map(Some)
        }
    }
}
impl DecodeValue for rust_decimal::Decimal {
    fn decode(row: &Row, i: usize) -> Result<Self, Error> {
        match row {
            Row::Postgres(r) => r.try_get(i),
            Row::Sqlite(r) => {
                let raw: String = r.try_get_unchecked(i)?;
                raw.parse().map_err(|e| Error::Decode(Box::new(e)))
            }
        }
    }
}
impl DecodeValue for serde_json::Value {
    fn decode(row: &Row, i: usize) -> Result<Self, Error> {
        match row {
            Row::Postgres(r) => r.try_get(i),
            Row::Sqlite(r) => {
                let raw: String = r.try_get(i)?;
                serde_json::from_str(&raw).map_err(|e| Error::Decode(Box::new(e)))
            }
        }
    }
}
pub trait FromRow: Sized {
    fn from_row(row: &Row) -> Result<Self, Error>;
}
impl FromRow for Row {
    fn from_row(row: &Row) -> Result<Self, Error> {
        Ok(row.clone())
    }
}
impl<T0: DecodeValue> FromRow for (T0,) {
    fn from_row(row: &Row) -> Result<Self, Error> {
        Ok((row.decode::<T0>(0)?,))
    }
}
impl<T0: DecodeValue, T1: DecodeValue> FromRow for (T0, T1) {
    fn from_row(row: &Row) -> Result<Self, Error> {
        Ok((row.decode::<T0>(0)?, row.decode::<T1>(1)?))
    }
}
impl<T0: DecodeValue, T1: DecodeValue, T2: DecodeValue> FromRow for (T0, T1, T2) {
    fn from_row(row: &Row) -> Result<Self, Error> {
        Ok((
            row.decode::<T0>(0)?,
            row.decode::<T1>(1)?,
            row.decode::<T2>(2)?,
        ))
    }
}
impl<T0: DecodeValue, T1: DecodeValue, T2: DecodeValue, T3: DecodeValue> FromRow
    for (T0, T1, T2, T3)
{
    fn from_row(row: &Row) -> Result<Self, Error> {
        Ok((
            row.decode::<T0>(0)?,
            row.decode::<T1>(1)?,
            row.decode::<T2>(2)?,
            row.decode::<T3>(3)?,
        ))
    }
}
impl<T0: DecodeValue, T1: DecodeValue, T2: DecodeValue, T3: DecodeValue, T4: DecodeValue> FromRow
    for (T0, T1, T2, T3, T4)
{
    fn from_row(row: &Row) -> Result<Self, Error> {
        Ok((
            row.decode::<T0>(0)?,
            row.decode::<T1>(1)?,
            row.decode::<T2>(2)?,
            row.decode::<T3>(3)?,
            row.decode::<T4>(4)?,
        ))
    }
}
impl<
        T0: DecodeValue,
        T1: DecodeValue,
        T2: DecodeValue,
        T3: DecodeValue,
        T4: DecodeValue,
        T5: DecodeValue,
    > FromRow for (T0, T1, T2, T3, T4, T5)
{
    fn from_row(row: &Row) -> Result<Self, Error> {
        Ok((
            row.decode::<T0>(0)?,
            row.decode::<T1>(1)?,
            row.decode::<T2>(2)?,
            row.decode::<T3>(3)?,
            row.decode::<T4>(4)?,
            row.decode::<T5>(5)?,
        ))
    }
}
impl<
        T0: DecodeValue,
        T1: DecodeValue,
        T2: DecodeValue,
        T3: DecodeValue,
        T4: DecodeValue,
        T5: DecodeValue,
        T6: DecodeValue,
    > FromRow for (T0, T1, T2, T3, T4, T5, T6)
{
    fn from_row(row: &Row) -> Result<Self, Error> {
        Ok((
            row.decode::<T0>(0)?,
            row.decode::<T1>(1)?,
            row.decode::<T2>(2)?,
            row.decode::<T3>(3)?,
            row.decode::<T4>(4)?,
            row.decode::<T5>(5)?,
            row.decode::<T6>(6)?,
        ))
    }
}
impl<
        T0: DecodeValue,
        T1: DecodeValue,
        T2: DecodeValue,
        T3: DecodeValue,
        T4: DecodeValue,
        T5: DecodeValue,
        T6: DecodeValue,
        T7: DecodeValue,
    > FromRow for (T0, T1, T2, T3, T4, T5, T6, T7)
{
    fn from_row(row: &Row) -> Result<Self, Error> {
        Ok((
            row.decode::<T0>(0)?,
            row.decode::<T1>(1)?,
            row.decode::<T2>(2)?,
            row.decode::<T3>(3)?,
            row.decode::<T4>(4)?,
            row.decode::<T5>(5)?,
            row.decode::<T6>(6)?,
            row.decode::<T7>(7)?,
        ))
    }
}
impl<
        T0: DecodeValue,
        T1: DecodeValue,
        T2: DecodeValue,
        T3: DecodeValue,
        T4: DecodeValue,
        T5: DecodeValue,
        T6: DecodeValue,
        T7: DecodeValue,
        T8: DecodeValue,
    > FromRow for (T0, T1, T2, T3, T4, T5, T6, T7, T8)
{
    fn from_row(row: &Row) -> Result<Self, Error> {
        Ok((
            row.decode::<T0>(0)?,
            row.decode::<T1>(1)?,
            row.decode::<T2>(2)?,
            row.decode::<T3>(3)?,
            row.decode::<T4>(4)?,
            row.decode::<T5>(5)?,
            row.decode::<T6>(6)?,
            row.decode::<T7>(7)?,
            row.decode::<T8>(8)?,
        ))
    }
}
impl<
        T0: DecodeValue,
        T1: DecodeValue,
        T2: DecodeValue,
        T3: DecodeValue,
        T4: DecodeValue,
        T5: DecodeValue,
        T6: DecodeValue,
        T7: DecodeValue,
        T8: DecodeValue,
        T9: DecodeValue,
    > FromRow for (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9)
{
    fn from_row(row: &Row) -> Result<Self, Error> {
        Ok((
            row.decode::<T0>(0)?,
            row.decode::<T1>(1)?,
            row.decode::<T2>(2)?,
            row.decode::<T3>(3)?,
            row.decode::<T4>(4)?,
            row.decode::<T5>(5)?,
            row.decode::<T6>(6)?,
            row.decode::<T7>(7)?,
            row.decode::<T8>(8)?,
            row.decode::<T9>(9)?,
        ))
    }
}
impl<
        T0: DecodeValue,
        T1: DecodeValue,
        T2: DecodeValue,
        T3: DecodeValue,
        T4: DecodeValue,
        T5: DecodeValue,
        T6: DecodeValue,
        T7: DecodeValue,
        T8: DecodeValue,
        T9: DecodeValue,
        T10: DecodeValue,
    > FromRow for (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10)
{
    fn from_row(row: &Row) -> Result<Self, Error> {
        Ok((
            row.decode::<T0>(0)?,
            row.decode::<T1>(1)?,
            row.decode::<T2>(2)?,
            row.decode::<T3>(3)?,
            row.decode::<T4>(4)?,
            row.decode::<T5>(5)?,
            row.decode::<T6>(6)?,
            row.decode::<T7>(7)?,
            row.decode::<T8>(8)?,
            row.decode::<T9>(9)?,
            row.decode::<T10>(10)?,
        ))
    }
}
impl<
        T0: DecodeValue,
        T1: DecodeValue,
        T2: DecodeValue,
        T3: DecodeValue,
        T4: DecodeValue,
        T5: DecodeValue,
        T6: DecodeValue,
        T7: DecodeValue,
        T8: DecodeValue,
        T9: DecodeValue,
        T10: DecodeValue,
        T11: DecodeValue,
    > FromRow for (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11)
{
    fn from_row(row: &Row) -> Result<Self, Error> {
        Ok((
            row.decode::<T0>(0)?,
            row.decode::<T1>(1)?,
            row.decode::<T2>(2)?,
            row.decode::<T3>(3)?,
            row.decode::<T4>(4)?,
            row.decode::<T5>(5)?,
            row.decode::<T6>(6)?,
            row.decode::<T7>(7)?,
            row.decode::<T8>(8)?,
            row.decode::<T9>(9)?,
            row.decode::<T10>(10)?,
            row.decode::<T11>(11)?,
        ))
    }
}
impl<
        T0: DecodeValue,
        T1: DecodeValue,
        T2: DecodeValue,
        T3: DecodeValue,
        T4: DecodeValue,
        T5: DecodeValue,
        T6: DecodeValue,
        T7: DecodeValue,
        T8: DecodeValue,
        T9: DecodeValue,
        T10: DecodeValue,
        T11: DecodeValue,
        T12: DecodeValue,
    > FromRow for (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12)
{
    fn from_row(row: &Row) -> Result<Self, Error> {
        Ok((
            row.decode::<T0>(0)?,
            row.decode::<T1>(1)?,
            row.decode::<T2>(2)?,
            row.decode::<T3>(3)?,
            row.decode::<T4>(4)?,
            row.decode::<T5>(5)?,
            row.decode::<T6>(6)?,
            row.decode::<T7>(7)?,
            row.decode::<T8>(8)?,
            row.decode::<T9>(9)?,
            row.decode::<T10>(10)?,
            row.decode::<T11>(11)?,
            row.decode::<T12>(12)?,
        ))
    }
}
impl<
        T0: DecodeValue,
        T1: DecodeValue,
        T2: DecodeValue,
        T3: DecodeValue,
        T4: DecodeValue,
        T5: DecodeValue,
        T6: DecodeValue,
        T7: DecodeValue,
        T8: DecodeValue,
        T9: DecodeValue,
        T10: DecodeValue,
        T11: DecodeValue,
        T12: DecodeValue,
        T13: DecodeValue,
    > FromRow for (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13)
{
    fn from_row(row: &Row) -> Result<Self, Error> {
        Ok((
            row.decode::<T0>(0)?,
            row.decode::<T1>(1)?,
            row.decode::<T2>(2)?,
            row.decode::<T3>(3)?,
            row.decode::<T4>(4)?,
            row.decode::<T5>(5)?,
            row.decode::<T6>(6)?,
            row.decode::<T7>(7)?,
            row.decode::<T8>(8)?,
            row.decode::<T9>(9)?,
            row.decode::<T10>(10)?,
            row.decode::<T11>(11)?,
            row.decode::<T12>(12)?,
            row.decode::<T13>(13)?,
        ))
    }
}
impl<
        T0: DecodeValue,
        T1: DecodeValue,
        T2: DecodeValue,
        T3: DecodeValue,
        T4: DecodeValue,
        T5: DecodeValue,
        T6: DecodeValue,
        T7: DecodeValue,
        T8: DecodeValue,
        T9: DecodeValue,
        T10: DecodeValue,
        T11: DecodeValue,
        T12: DecodeValue,
        T13: DecodeValue,
        T14: DecodeValue,
    > FromRow
    for (
        T0,
        T1,
        T2,
        T3,
        T4,
        T5,
        T6,
        T7,
        T8,
        T9,
        T10,
        T11,
        T12,
        T13,
        T14,
    )
{
    fn from_row(row: &Row) -> Result<Self, Error> {
        Ok((
            row.decode::<T0>(0)?,
            row.decode::<T1>(1)?,
            row.decode::<T2>(2)?,
            row.decode::<T3>(3)?,
            row.decode::<T4>(4)?,
            row.decode::<T5>(5)?,
            row.decode::<T6>(6)?,
            row.decode::<T7>(7)?,
            row.decode::<T8>(8)?,
            row.decode::<T9>(9)?,
            row.decode::<T10>(10)?,
            row.decode::<T11>(11)?,
            row.decode::<T12>(12)?,
            row.decode::<T13>(13)?,
            row.decode::<T14>(14)?,
        ))
    }
}
impl<
        T0: DecodeValue,
        T1: DecodeValue,
        T2: DecodeValue,
        T3: DecodeValue,
        T4: DecodeValue,
        T5: DecodeValue,
        T6: DecodeValue,
        T7: DecodeValue,
        T8: DecodeValue,
        T9: DecodeValue,
        T10: DecodeValue,
        T11: DecodeValue,
        T12: DecodeValue,
        T13: DecodeValue,
        T14: DecodeValue,
        T15: DecodeValue,
    > FromRow
    for (
        T0,
        T1,
        T2,
        T3,
        T4,
        T5,
        T6,
        T7,
        T8,
        T9,
        T10,
        T11,
        T12,
        T13,
        T14,
        T15,
    )
{
    fn from_row(row: &Row) -> Result<Self, Error> {
        Ok((
            row.decode::<T0>(0)?,
            row.decode::<T1>(1)?,
            row.decode::<T2>(2)?,
            row.decode::<T3>(3)?,
            row.decode::<T4>(4)?,
            row.decode::<T5>(5)?,
            row.decode::<T6>(6)?,
            row.decode::<T7>(7)?,
            row.decode::<T8>(8)?,
            row.decode::<T9>(9)?,
            row.decode::<T10>(10)?,
            row.decode::<T11>(11)?,
            row.decode::<T12>(12)?,
            row.decode::<T13>(13)?,
            row.decode::<T14>(14)?,
            row.decode::<T15>(15)?,
        ))
    }
}

#[derive(Clone)]
pub enum Value {
    Int(Option<i32>),
    BigInt(Option<i64>),
    Float(Option<f64>),
    Bool(Option<bool>),
    Text(Option<String>),
    Bytes(Option<Vec<u8>>),
    Decimal(Option<rust_decimal::Decimal>),
    Json(Option<serde_json::Value>),
    Date(Option<chrono::NaiveDate>),
    Timestamp(Option<chrono::NaiveDateTime>),
    Utc(Option<chrono::DateTime<chrono::Utc>>),
    Ints(Option<Vec<i32>>),
    Texts(Option<Vec<String>>),
}
pub trait ToValue {
    fn to_value(&self) -> Value;
    fn null_value() -> Value;
}
macro_rules! value {($($t:ty=>$v:ident),*)=>{$(impl ToValue for $t {fn to_value(&self)->Value{Value::$v(Some(self.clone()))}fn null_value()->Value{Value::$v(None)}})*}}
value!(i32=>Int,i64=>BigInt,f64=>Float,bool=>Bool,String=>Text,Vec<u8> =>Bytes,rust_decimal::Decimal=>Decimal,serde_json::Value=>Json,chrono::NaiveDate=>Date,chrono::NaiveDateTime=>Timestamp,chrono::DateTime<chrono::Utc> =>Utc,Vec<i32> =>Ints,Vec<String> =>Texts);
impl ToValue for str {
    fn to_value(&self) -> Value {
        Value::Text(Some(self.into()))
    }
    fn null_value() -> Value {
        Value::Text(None)
    }
}
impl<T: ToValue + ?Sized> ToValue for &T {
    fn to_value(&self) -> Value {
        (**self).to_value()
    }
    fn null_value() -> Value {
        T::null_value()
    }
}
impl<T: ToValue> ToValue for Option<T> {
    fn to_value(&self) -> Value {
        self.as_ref()
            .map(ToValue::to_value)
            .unwrap_or_else(T::null_value)
    }
    fn null_value() -> Value {
        T::null_value()
    }
}
impl ToValue for [i32] {
    fn to_value(&self) -> Value {
        Value::Ints(Some(self.to_vec()))
    }
    fn null_value() -> Value {
        Value::Ints(None)
    }
}
impl ToValue for [u8] {
    fn to_value(&self) -> Value {
        Value::Bytes(Some(self.to_vec()))
    }
    fn null_value() -> Value {
        Value::Bytes(None)
    }
}
fn pg_query<'q>(
    sql: &'q str,
    args: Vec<Value>,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    let mut q = sqlx::query(sql);
    for v in args {
        q = match v {
            Value::Int(v) => q.bind(v),
            Value::BigInt(v) => q.bind(v),
            Value::Float(v) => q.bind(v),
            Value::Bool(v) => q.bind(v),
            Value::Text(v) => q.bind(v),
            Value::Bytes(v) => q.bind(v),
            Value::Decimal(v) => q.bind(v),
            Value::Json(v) => q.bind(v),
            Value::Date(v) => q.bind(v),
            Value::Timestamp(v) => q.bind(v),
            Value::Utc(v) => q.bind(v),
            Value::Ints(v) => q.bind(v),
            Value::Texts(v) => q.bind(v),
        }
    }
    q
}
fn sqlite_query<'q>(
    sql: &'q str,
    args: Vec<Value>,
) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>> {
    let mut q = sqlx::query(sql);
    for v in args {
        q = match v {
            Value::Int(v) => q.bind(v),
            Value::BigInt(v) => q.bind(v),
            Value::Float(v) => q.bind(v),
            Value::Bool(v) => q.bind(v),
            Value::Text(v) => q.bind(v),
            Value::Bytes(v) => q.bind(v),
            Value::Decimal(v) => q.bind(v.map(|v| v.to_string())),
            Value::Json(v) => q.bind(v.map(|v| v.to_string())),
            Value::Date(v) => q.bind(v),
            Value::Timestamp(v) => q.bind(v),
            Value::Utc(v) => q.bind(v),
            Value::Ints(v) => q.bind(v.map(|v| serde_json::to_string(&v).unwrap())),
            Value::Texts(v) => q.bind(v.map(|v| serde_json::to_string(&v).unwrap())),
        }
    }
    q
}
pub struct Outcome {
    pub rows: Vec<Row>,
    affected: u64,
}
pub struct QueryResult {
    affected: u64,
}
impl QueryResult {
    pub fn rows_affected(&self) -> u64 {
        self.affected
    }
}
type FutureResult<'a> = Pin<Box<dyn Future<Output = Result<Outcome, Error>> + Send + 'a>>;
pub trait Executor<'a> {
    type Database;
    fn run(self, sql: String, args: Vec<Value>, rows: bool) -> FutureResult<'a>;
}
impl<'a> Executor<'a> for &'a Pool {
    type Database = Backend;
    fn run(self, sql: String, args: Vec<Value>, rows: bool) -> FutureResult<'a> {
        Box::pin(async move {
            match self {
                Pool::Postgres(p) => {
                    if rows {
                        Ok(Outcome {
                            rows: pg_query(&sql, args)
                                .fetch_all(p)
                                .await?
                                .into_iter()
                                .map(|r| Row::Postgres(std::sync::Arc::new(r)))
                                .collect(),
                            affected: 0,
                        })
                    } else {
                        Ok(Outcome {
                            rows: vec![],
                            affected: pg_query(&sql, args).execute(p).await?.rows_affected(),
                        })
                    }
                }
                Pool::Sqlite(p) => {
                    let (sql, args) = crate::sqlite_dialect::compile(&sql, args)
                        .map_err(|e| Error::Protocol(e.to_string()))?;
                    if rows {
                        Ok(Outcome {
                            rows: sqlite_query(&sql, args)
                                .fetch_all(p)
                                .await?
                                .into_iter()
                                .map(|r| Row::Sqlite(std::sync::Arc::new(r)))
                                .collect(),
                            affected: 0,
                        })
                    } else {
                        Ok(Outcome {
                            rows: vec![],
                            affected: sqlite_query(&sql, args).execute(p).await?.rows_affected(),
                        })
                    }
                }
            }
        })
    }
}
impl<'a> Executor<'a> for &'a mut Connection {
    type Database = Backend;
    fn run(self, sql: String, args: Vec<Value>, rows: bool) -> FutureResult<'a> {
        Box::pin(async move {
            match self {
                Connection::Postgres(c) => {
                    if rows {
                        Ok(Outcome {
                            rows: pg_query(&sql, args)
                                .fetch_all(c.connection())
                                .await?
                                .into_iter()
                                .map(|r| Row::Postgres(std::sync::Arc::new(r)))
                                .collect(),
                            affected: 0,
                        })
                    } else {
                        Ok(Outcome {
                            rows: vec![],
                            affected: pg_query(&sql, args)
                                .execute(c.connection())
                                .await?
                                .rows_affected(),
                        })
                    }
                }
                Connection::Sqlite(c) => {
                    c.validate(&sql)?;
                    let (sql, args) = crate::sqlite_dialect::compile(&sql, args)
                        .map_err(|e| Error::Protocol(e.to_string()))?;
                    if c.read_only() && !crate::sqlite_dialect::is_compiled_read_only(&sql) {
                        return Err(Error::Protocol(
                            "Cannot write in a read-only standalone transaction".into(),
                        ));
                    }
                    if rows {
                        Ok(Outcome {
                            rows: sqlite_query(&sql, args)
                                .fetch_all(c.connection())
                                .await?
                                .into_iter()
                                .map(|r| Row::Sqlite(std::sync::Arc::new(r)))
                                .collect(),
                            affected: 0,
                        })
                    } else {
                        Ok(Outcome {
                            rows: vec![],
                            affected: sqlite_query(&sql, args)
                                .execute(c.connection())
                                .await?
                                .rows_affected(),
                        })
                    }
                }
            }
        })
    }
}
impl<'a> Executor<'a> for &'a mut Box<Connection> {
    type Database = Backend;
    fn run(self, s: String, a: Vec<Value>, r: bool) -> FutureResult<'a> {
        (&mut **self).run(s, a, r)
    }
}
pub struct Query<DB, T> {
    sql: String,
    args: Vec<Value>,
    marker: PhantomData<(DB, T)>,
}
pub fn query(sql: &str) -> Query<Backend, Row> {
    Query {
        sql: sql.into(),
        args: vec![],
        marker: PhantomData,
    }
}
pub fn query_as<DB, T>(sql: &str) -> Query<DB, T> {
    Query {
        sql: sql.into(),
        args: vec![],
        marker: PhantomData,
    }
}
pub struct Scalar<T>(T);
impl<T: DecodeValue> FromRow for Scalar<T> {
    fn from_row(r: &Row) -> Result<Self, Error> {
        Ok(Self(r.decode(0)?))
    }
}
pub struct ScalarQuery<DB, T>(Query<DB, Scalar<T>>);
pub fn query_scalar<DB, T>(sql: &str) -> ScalarQuery<DB, T> {
    ScalarQuery(Query {
        sql: sql.into(),
        args: vec![],
        marker: PhantomData,
    })
}
impl<DB, T: DecodeValue> ScalarQuery<DB, T> {
    pub fn bind<V: ToValue>(self, v: V) -> Self {
        Self(self.0.bind(v))
    }
    pub async fn execute<'a, E: Executor<'a, Database = DB>>(
        self,
        e: E,
    ) -> Result<QueryResult, Error> {
        self.0.execute(e).await
    }
    pub async fn fetch_all<'a, E: Executor<'a, Database = DB>>(
        self,
        e: E,
    ) -> Result<Vec<T>, Error> {
        Ok(self
            .0
            .fetch_all(e)
            .await?
            .into_iter()
            .map(|v| v.0)
            .collect())
    }
    pub async fn fetch_optional<'a, E: Executor<'a, Database = DB>>(
        self,
        e: E,
    ) -> Result<Option<T>, Error> {
        Ok(self.0.fetch_optional(e).await?.map(|v| v.0))
    }
    pub async fn fetch_one<'a, E: Executor<'a, Database = DB>>(self, e: E) -> Result<T, Error> {
        Ok(self.0.fetch_one(e).await?.0)
    }
}
impl<DB, T> Query<DB, T> {
    pub fn bind<V: ToValue>(mut self, v: V) -> Self {
        self.args.push(v.to_value());
        self
    }
    pub async fn execute<'a, E: Executor<'a, Database = DB>>(
        self,
        e: E,
    ) -> Result<QueryResult, Error> {
        let result = e.run(self.sql, self.args, false).await?;
        Ok(QueryResult {
            affected: result.affected,
        })
    }
}
impl<DB, T: FromRow> Query<DB, T> {
    pub async fn fetch_all<'a, E: Executor<'a, Database = DB>>(
        self,
        e: E,
    ) -> Result<Vec<T>, Error> {
        e.run(self.sql, self.args, true)
            .await?
            .rows
            .iter()
            .map(T::from_row)
            .collect()
    }
    pub async fn fetch_optional<'a, E: Executor<'a, Database = DB>>(
        self,
        e: E,
    ) -> Result<Option<T>, Error> {
        let rows = e.run(self.sql, self.args, true).await?.rows;
        rows.first().map(T::from_row).transpose()
    }
    pub async fn fetch_one<'a, E: Executor<'a, Database = DB>>(self, e: E) -> Result<T, Error> {
        self.fetch_optional(e).await?.ok_or(Error::RowNotFound)
    }
}

pub struct RawQuery(String);
pub fn raw_sql(sql: &str) -> RawQuery {
    RawQuery(sql.into())
}
pub trait RawExecutor<'a> {
    fn raw(self, sql: String) -> FutureResult<'a>;
}
impl RawQuery {
    pub async fn execute<'a, E: RawExecutor<'a>>(self, e: E) -> Result<QueryResult, Error> {
        let r = e.raw(self.0).await?;
        Ok(QueryResult {
            affected: r.affected,
        })
    }
}
impl<'a> RawExecutor<'a> for &'a Pool {
    fn raw(self, sql: String) -> FutureResult<'a> {
        Box::pin(async move {
            let affected = match self {
                Pool::Postgres(p) => ::sqlx::Executor::execute(p, sql.as_str())
                    .await?
                    .rows_affected(),
                Pool::Sqlite(p) => ::sqlx::Executor::execute(p, sql.as_str())
                    .await?
                    .rows_affected(),
            };
            Ok(Outcome {
                rows: vec![],
                affected,
            })
        })
    }
}
impl<'a> RawExecutor<'a> for &'a mut Connection {
    fn raw(self, sql: String) -> FutureResult<'a> {
        Box::pin(async move {
            let affected = match self {
                Connection::Postgres(c) => ::sqlx::Executor::execute(c.connection(), sql.as_str())
                    .await?
                    .rows_affected(),
                Connection::Sqlite(c) => {
                    c.validate(&sql)?;
                    ::sqlx::Executor::execute(c.connection(), sql.as_str())
                        .await?
                        .rows_affected()
                }
            };
            Ok(Outcome {
                rows: vec![],
                affected,
            })
        })
    }
}
impl<'a> RawExecutor<'a> for &'a mut Box<Connection> {
    fn raw(self, sql: String) -> FutureResult<'a> {
        (&mut **self).raw(sql)
    }
}

// Native PostgreSQL executors remain available for fixtures and owner tools.
impl<'a> Executor<'a> for &'a ::sqlx::PgPool {
    type Database = Backend;
    fn run(self, sql: String, args: Vec<Value>, rows: bool) -> FutureResult<'a> {
        Box::pin(async move {
            if rows {
                Ok(Outcome {
                    rows: pg_query(&sql, args)
                        .fetch_all(self)
                        .await?
                        .into_iter()
                        .map(|r| Row::Postgres(std::sync::Arc::new(r)))
                        .collect(),
                    affected: 0,
                })
            } else {
                Ok(Outcome {
                    rows: vec![],
                    affected: pg_query(&sql, args).execute(self).await?.rows_affected(),
                })
            }
        })
    }
}
impl<'a> Executor<'a> for &'a mut ::sqlx::PgConnection {
    type Database = Backend;
    fn run(self, sql: String, args: Vec<Value>, rows: bool) -> FutureResult<'a> {
        Box::pin(async move {
            if rows {
                Ok(Outcome {
                    rows: pg_query(&sql, args)
                        .fetch_all(self)
                        .await?
                        .into_iter()
                        .map(|r| Row::Postgres(std::sync::Arc::new(r)))
                        .collect(),
                    affected: 0,
                })
            } else {
                Ok(Outcome {
                    rows: vec![],
                    affected: pg_query(&sql, args).execute(self).await?.rows_affected(),
                })
            }
        })
    }
}
impl<'a> RawExecutor<'a> for &'a ::sqlx::PgPool {
    fn raw(self, sql: String) -> FutureResult<'a> {
        Box::pin(async move {
            Ok(Outcome {
                rows: vec![],
                affected: ::sqlx::Executor::execute(self, sql.as_str())
                    .await?
                    .rows_affected(),
            })
        })
    }
}
pub mod postgres {
    pub use ::sqlx::postgres::{PgConnectOptions, PgPoolOptions};
}
pub trait ColumnIndex {
    fn index(self, row: &Row) -> Result<usize, Error>;
}
impl ColumnIndex for usize {
    fn index(self, _: &Row) -> Result<usize, Error> {
        Ok(self)
    }
}
impl ColumnIndex for &str {
    fn index(self, row: &Row) -> Result<usize, Error> {
        match row {
            Row::Postgres(r) => r
                .columns()
                .iter()
                .position(|c| ::sqlx::Column::name(c) == self),
            Row::Sqlite(r) => r
                .columns()
                .iter()
                .position(|c| ::sqlx::Column::name(c) == self),
        }
        .ok_or_else(|| Error::ColumnNotFound(self.into()))
    }
}
impl Row {
    pub fn get<T: DecodeValue, I: ColumnIndex>(&self, index: I) -> T {
        self.decode(index.index(self).expect("column index"))
            .expect("column decode")
    }
}
