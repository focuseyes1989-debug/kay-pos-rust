//! Parse PostgreSQL SQL before adapting it. Never rewrite inside quoted text or bound values.
use crate::storage::Value as BoundValue;
use anyhow::{bail, Context, Result};
use sqlparser::{ast::*, dialect::PostgreSqlDialect, parser::Parser};
use std::{collections::HashMap, ops::ControlFlow};

fn expression(sql: &str) -> Result<Expr> {
    let mut statements = Parser::parse_sql(&PostgreSqlDialect {}, &format!("SELECT {sql}"))?;
    if let Statement::Query(q) = statements.remove(0) {
        if let SetExpr::Select(s) = *q.body {
            if let SelectItem::UnnamedExpr(e) = s.projection[0].clone() {
                return Ok(e);
            }
        }
    }
    bail!("Invalid SQLite adapter expression")
}
fn args(f: &Function) -> Vec<String> {
    match &f.args {
        FunctionArguments::List(a) => a.args.iter().map(ToString::to_string).collect(),
        _ => vec![],
    }
}
fn ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}
fn literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}
fn column_map() -> &'static HashMap<String, Vec<String>> {
    static C: std::sync::OnceLock<HashMap<String, Vec<String>>> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        serde_json::from_str(include_str!("../../../migrations/sqlite/columns.json"))
            .expect("compiled schema columns")
    })
}
fn table_columns(t: &TableFactor) -> Option<(String, Vec<String>)> {
    match t {
        TableFactor::Table { name, alias, .. } => {
            let name = name.to_string();
            column_map().get(&name).map(|c| {
                (
                    alias.as_ref().map(|a| a.name.value.clone()).unwrap_or(name),
                    c.clone(),
                )
            })
        }
        TableFactor::Derived {
            subquery,
            alias: Some(a),
            ..
        } => {
            if let SetExpr::Select(s) = &*subquery.body {
                Some((
                    a.name.value.clone(),
                    s.projection
                        .iter()
                        .filter_map(|p| match p {
                            SelectItem::ExprWithAlias { alias, .. } => Some(alias.value.clone()),
                            SelectItem::UnnamedExpr(Expr::Identifier(i)) => Some(i.value.clone()),
                            SelectItem::UnnamedExpr(Expr::CompoundIdentifier(i)) => {
                                i.last().map(|i| i.value.clone())
                            }
                            _ => None,
                        })
                        .collect(),
                ))
            } else {
                None
            }
        }
        _ => None,
    }
}
struct Adapter {
    scopes: Vec<HashMap<String, Vec<String>>>,
}
impl Adapter {
    fn transform(&mut self, e: &mut Expr) -> Result<()> {
        let replacement = match e {
            Expr::Cast {
                kind,
                expr,
                data_type,
                ..
            } => {
                *kind = CastKind::Cast;
                let ty = data_type.to_string().to_lowercase();
                if ty == "date" {
                    Some(format!("date({expr})"))
                } else if ty.starts_with("timestamp") {
                    Some(format!("strftime('%Y-%m-%d %H:%M:%f',{expr})"))
                } else {
                    *data_type = if matches!(
                        ty.as_str(),
                        "float8" | "float4" | "double precision" | "real"
                    ) {
                        DataType::Real
                    } else if ty.starts_with("numeric") || ty.starts_with("decimal") {
                        DataType::Numeric(ExactNumberInfo::None)
                    } else if matches!(
                        ty.as_str(),
                        "int"
                            | "integer"
                            | "int4"
                            | "int8"
                            | "bigint"
                            | "smallint"
                            | "boolean"
                            | "tid"
                    ) {
                        DataType::Integer(None)
                    } else if ty == "bytea" {
                        DataType::Blob(None)
                    } else {
                        DataType::Text
                    };
                    None
                }
            }
            Expr::Identifier(i) if i.value == "ctid" => {
                i.value = "rowid".into();
                None
            }
            Expr::CompoundIdentifier(i) if i.last().is_some_and(|v| v.value == "ctid") => {
                i.last_mut().unwrap().value = "rowid".into();
                None
            }
            Expr::ILike {
                negated,
                expr,
                pattern,
                escape_char,
                ..
            } => Some(format!(
                "lower({expr}) {}LIKE lower({pattern}){}",
                if *negated { "NOT " } else { "" },
                escape_char
                    .as_ref()
                    .map(|v| format!(" ESCAPE {v}"))
                    .unwrap_or_default()
            )),
            Expr::AnyOp {
                left,
                compare_op: BinaryOperator::Eq,
                right,
                ..
            } => Some(format!("{left} IN (SELECT value FROM json_each({right}))")),
            // PostgreSQL financial divisions use NUMERIC/float operands. SQLite NUMERIC
            // affinity may store whole amounts as integers, so preserve fractional results.
            Expr::BinaryOp {
                left,
                op: BinaryOperator::Divide,
                right,
            } => Some(format!("CAST({left} AS REAL) / {right}")),
            Expr::BinaryOp {
                left,
                op: BinaryOperator::PGRegexMatch,
                right,
            } => Some(format!("kay_regexp({right},{left})")),
            Expr::BinaryOp {
                left,
                op: BinaryOperator::PGRegexNotMatch,
                right,
            } => Some(format!("NOT kay_regexp({right},{left})")),
            Expr::BinaryOp {
                left,
                op: BinaryOperator::Minus,
                right,
            } if right.to_string().starts_with("kay_interval_days(") => {
                let Expr::Function(f) = &**right else {
                    unreachable!()
                };
                Some(format!(
                    "datetime({left}, '-' || {} || ' days')",
                    args(f)[0]
                ))
            }
            Expr::BinaryOp {
                left,
                op: BinaryOperator::Minus,
                right,
            } if left.to_string().contains("json_object(") => {
                Some(format!("json_remove({left}, '$.' || {right})"))
            }
            Expr::BinaryOp {
                left,
                op: BinaryOperator::StringConcat,
                right,
            } if right.to_string().starts_with("json_object(") => {
                Some(format!("json_patch({left},{right})"))
            }
            Expr::BinaryOp {
                left,
                op: BinaryOperator::Plus,
                right,
            } if matches!(&**right, Expr::Interval(_)) => {
                if let Expr::Interval(i) = &**right {
                    Some(format!("datetime({left}, {})", i.value))
                } else {
                    None
                }
            }
            Expr::BinaryOp {
                left,
                op: BinaryOperator::Plus,
                right,
            } if left.to_string().starts_with("date(") => {
                Some(format!("date({left}, '+' || {right} || ' days')"))
            }
            Expr::Substring {
                expr,
                substring_from: Some(from),
                substring_for,
                ..
            } => {
                if matches!(&**from,Expr::Value(v) if matches!(v.value,Value::SingleQuotedString(_)))
                {
                    Some(format!("kay_substring_regex({expr},{from})"))
                } else {
                    Some(format!(
                        "substr({expr},{from}{})",
                        substring_for
                            .as_ref()
                            .map(|v| format!(",{v}"))
                            .unwrap_or_default()
                    ))
                }
            }
            Expr::Function(f) => {
                let name = f.name.to_string().to_lowercase();
                let a = args(f);
                match name.as_str() {
                    "make_interval" => {
                        let FunctionArguments::List(list) = &f.args else {
                            bail!("Unsupported interval")
                        };
                        let (name, arg) = match list.args.as_slice() {
                            [FunctionArg::Named { name, arg, .. }] => (name.to_string(), arg),
                            [FunctionArg::ExprNamed { name, arg, .. }] => (name.to_string(), arg),
                            _ => bail!("Unsupported interval arguments"),
                        };
                        anyhow::ensure!(
                            name.eq_ignore_ascii_case("days"),
                            "Unsupported interval unit"
                        );
                        Some(format!("kay_interval_days({arg})"))
                    }
                    "current_date" => Some("date('now','localtime')".into()),
                    "localtimestamp" | "current_timestamp" | "now" | "clock_timestamp" => {
                        Some("kay_clock_timestamp()".into())
                    }
                    "to_regclass" => Some(format!(
                        "(SELECT name FROM sqlite_schema WHERE type='table' AND name={})",
                        a[0]
                    )),
                    "to_char" => Some(format!("kay_to_char({})", a.join(","))),
                    "chr" => Some(format!("char({})", a.join(","))),
                    "btrim" => Some(format!("trim({})", a.join(","))),
                    "octet_length" => Some(format!("length({})", a.join(","))),
                    "left" => Some(format!("substr({},1,{})", a[0], a[1])),
                    "strpos" => Some(format!("instr({},{})", a[0], a[1])),
                    "greatest" | "least" => Some(format!("kay_{name}({})", a.join(","))),
                    "json_build_array" | "jsonb_build_array" => {
                        Some(format!("json_array({})", a.join(",")))
                    }
                    "json_build_object" | "jsonb_build_object" => {
                        Some(format!("json_object({})", a.join(",")))
                    }
                    "to_jsonb" | "row_to_json" => {
                        let alias = a.first().context("Missing row alias")?;
                        let columns = self
                            .scopes
                            .iter()
                            .rev()
                            .find_map(|s| s.get(alias))
                            .with_context(|| format!("Unknown JSON row alias {alias}"))?;
                        Some(format!(
                            "json_object({})",
                            columns
                                .iter()
                                .flat_map(|c| [
                                    literal(c),
                                    if c == "photo_data" {
                                        format!("encode({}.{},'base64')", ident(alias), ident(c))
                                    } else {
                                        format!("{}.{}", ident(alias), ident(c))
                                    }
                                ])
                                .collect::<Vec<_>>()
                                .join(",")
                        ))
                    }
                    "string_agg" => {
                        f.name = ObjectName::from(vec![Ident::new("group_concat")]);
                        None
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some(s) = replacement {
            *e = expression(&s)?;
        }
        Ok(())
    }
}
impl VisitorMut for Adapter {
    type Break = anyhow::Error;
    fn pre_visit_query(&mut self, q: &mut Query) -> ControlFlow<Self::Break> {
        q.locks.clear();
        let mut aliases = HashMap::new();
        if let SetExpr::Select(s) = &mut *q.body {
            for p in &mut s.projection {
                if let SelectItem::ExprWithAlias { alias, .. } = p {
                    alias.quote_style = Some('"');
                }
            }
        }

        if let SetExpr::Select(s) = &*q.body {
            for from in &s.from {
                if let Some((a, c)) = table_columns(&from.relation) {
                    aliases.insert(a, c);
                }
                for j in &from.joins {
                    if let Some((a, c)) = table_columns(&j.relation) {
                        aliases.insert(a, c);
                    }
                }
            }
        }
        self.scopes.push(aliases);
        ControlFlow::Continue(())
    }
    fn post_visit_query(&mut self, _: &mut Query) -> ControlFlow<Self::Break> {
        self.scopes.pop();
        ControlFlow::Continue(())
    }
    fn post_visit_expr(&mut self, e: &mut Expr) -> ControlFlow<Self::Break> {
        match self.transform(e) {
            Ok(()) => ControlFlow::Continue(()),
            Err(e) => ControlFlow::Break(e),
        }
    }
}
pub fn compile(sql: &str, mut arguments: Vec<BoundValue>) -> Result<(String, Vec<BoundValue>)> {
    let trimmed = sql.trim();
    let upper = trimmed.to_uppercase();
    // BEGIN IMMEDIATE already serializes writers, including the scope of PostgreSQL advisory/table locks.
    if upper.starts_with("SET LOCAL ")
        || upper.starts_with("SET TRANSACTION ")
        || upper.starts_with("LOCK TABLE ")
        || upper.starts_with("SELECT PG_ADVISORY_XACT_LOCK(")
    {
        return Ok(("SELECT 1".into(), vec![]));
    }
    if upper.starts_with("SELECT NEXTVAL(") {
        let sequence = trimmed.split('\'').nth(1).context("Invalid sequence")?;
        if matches!(sequence, "service_orders" | "category_groups") {
            return Ok((format!("INSERT INTO rust_sequences(name,value) VALUES({},(SELECT COALESCE(MAX(id),0)+1 FROM {sequence})) ON CONFLICT(name) DO UPDATE SET value=max(rust_sequences.value+1,(SELECT COALESCE(MAX(id),0)+1 FROM {sequence})) RETURNING value",literal(sequence)),vec![]));
        }
        anyhow::ensure!(
            matches!(
                sequence,
                "rust_invoice_number_seq" | "rust_batch_number_seq"
            ),
            "Unknown standalone sequence"
        );
        return Ok((
            format!(
                "UPDATE rust_sequences SET value=value+1 WHERE name={} RETURNING value",
                literal(sequence)
            ),
            vec![],
        ));
    }
    let mut statements = Parser::parse_sql(&PostgreSqlDialect {}, sql)
        .with_context(|| format!("Cannot parse standalone SQL: {sql}"))?;
    anyhow::ensure!(statements.len() == 1, "One statement required per query");
    if let ControlFlow::Break(e) = VisitMut::visit(&mut statements, &mut Adapter { scopes: vec![] })
    {
        return Err(e);
    }
    let output = statements[0].to_string();
    // Token-level parameter conversion leaves quoted literals and comments untouched.
    let tokens = sqlparser::tokenizer::Tokenizer::new(&PostgreSqlDialect {}, &output).tokenize()?;
    let mut count = 0;
    let output = tokens
        .into_iter()
        .map(|t| {
            if let sqlparser::tokenizer::Token::Placeholder(p) = &t {
                if let Some(n) = p.strip_prefix('$').and_then(|n| n.parse::<usize>().ok()) {
                    count = count.max(n);
                    return format!("?{n}");
                }
            }
            t.to_string()
        })
        .collect::<String>();
    anyhow::ensure!(count <= arguments.len(), "Missing query argument");
    arguments.truncate(count);
    Ok((output, arguments))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn all_static_pos_queries_prepare_on_standalone_schema() -> Result<()> {
        let pool = crate::sqlite::connect("sqlite::memory:").await?;
        let crate::storage::Pool::Sqlite(native) = &pool else {
            unreachable!()
        };
        let queries: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../tests/fixtures/sqlite_queries.json"))?;
        let mut failures = vec![];
        for q in queries {
            let source = q["source"].as_str().unwrap();
            let sql = q["sql"].as_str().unwrap();
            let result = async {
                let (sql, args) = compile(sql, vec![BoundValue::Text(None); 32])?;
                let explain = format!("EXPLAIN {sql}");
                let mut query = ::sqlx::query(&explain);
                for _ in args {
                    query = query.bind(Option::<String>::None);
                }
                query.fetch_all(native).await.with_context(|| sql)?;
                Ok::<_, anyhow::Error>(())
            }
            .await;
            if let Err(e) = result {
                failures.push(format!("{source}: {e:#}"));
            }
        }
        pool.close().await;
        anyhow::ensure!(
            failures.is_empty(),
            "{} unsupported statements:\n{}",
            failures.len(),
            failures.join("\n")
        );
        Ok(())
    }
}

pub(crate) fn is_read_only(sql: &str) -> bool {
    let upper = sql.trim().to_uppercase();
    if upper.starts_with("SET LOCAL ") || upper.starts_with("SET TRANSACTION ") {
        return true;
    }
    Parser::parse_sql(&PostgreSqlDialect {}, sql)
        .is_ok_and(|s| s.len() == 1 && matches!(s[0], Statement::Query(_)))
}

/// Check the translated statement too: e.g. PostgreSQL SELECT nextval becomes UPDATE.
pub(crate) fn is_compiled_read_only(sql: &str) -> bool {
    Parser::parse_sql(&sqlparser::dialect::SQLiteDialect {}, sql)
        .is_ok_and(|s| s.len() == 1 && matches!(s[0], Statement::Query(_)))
}
