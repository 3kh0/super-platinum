//! Bounded exact logical SQLite inventory, including unknown raw JSON bytes.
use super::*;
use rusqlite::{
    params_from_iter,
    types::{Value, ValueRef},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Object {
    pub kind: String,
    pub name: String,
    pub table: String,
    pub sql: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(super) enum Cell {
    Null,
    Integer(i64),
    Real(u64),
    Text(String),
    Blob(Vec<u8>),
}
impl Cell {
    fn value(&self) -> Value {
        match self {
            Self::Null => Value::Null,
            Self::Integer(v) => Value::Integer(*v),
            Self::Real(v) => Value::Real(f64::from_bits(*v)),
            Self::Text(v) => Value::Text(v.clone()),
            Self::Blob(v) => Value::Blob(v.clone()),
        }
    }
    pub fn text(&self) -> Result<&str, AppError> {
        if let Self::Text(v) = self {
            Ok(v)
        } else {
            Err(invalid("expected exact SQLite text"))
        }
    }
    pub fn integer(&self) -> Result<i64, AppError> {
        if let Self::Integer(v) = self {
            Ok(*v)
        } else {
            Err(invalid("expected exact SQLite integer"))
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Cell>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Image {
    pub schema: Vec<Object>,
    pub tables: BTreeMap<String, Table>,
    pub user_version: i64,
    pub application_id: i64,
}
pub(super) fn quoted(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}
impl Image {
    pub fn shape(&self) -> Result<(), AppError> {
        let mut rows = 0usize;
        let mut bytes = 0usize;
        let mut names = std::collections::BTreeSet::new();
        if self.schema.len() > 128 || self.tables.len() > 128 {
            return Err(invalid("persisted schema bound"));
        }
        let schema_tables = self
            .schema
            .iter()
            .filter(|o| o.kind == "table")
            .map(|o| &o.name)
            .collect::<std::collections::BTreeSet<_>>();
        if schema_tables
            != self
                .tables
                .keys()
                .collect::<std::collections::BTreeSet<_>>()
        {
            return Err(invalid("schema/table inventory mismatch"));
        }
        for object in &self.schema {
            if !names.insert((&object.kind, &object.name)) {
                return Err(invalid("duplicate schema object"));
            }
            bytes = bytes
                .checked_add(
                    object.sql.as_ref().map_or(0, String::len)
                        + object.name.len()
                        + object.table.len(),
                )
                .ok_or_else(|| invalid("schema bound overflow"))?;
        }
        if bytes > 262144 {
            return Err(invalid("persisted schema byte bound"));
        }
        for table in self.tables.values() {
            table.shape()?;
            rows = rows
                .checked_add(table.rows.len())
                .ok_or_else(|| invalid("row count overflow"))?;
            for row in &table.rows {
                for cell in row {
                    bytes = bytes
                        .checked_add(match cell {
                            Cell::Text(v) => v.len(),
                            Cell::Blob(v) => v.len(),
                            _ => 8,
                        })
                        .ok_or_else(|| invalid("inventory size overflow"))?;
                }
            }
        }
        if rows > 16384 || bytes > 32 * 1024 * 1024 {
            return Err(invalid("persisted inventory bound"));
        }
        Ok(())
    }
    pub fn read(conn: &Connection) -> Result<Self, AppError> {
        let (n,size):(i64,i64)=conn.query_row("SELECT COUNT(*),COALESCE(SUM(length(CAST(COALESCE(sql,'') AS BLOB))+length(name)+length(tbl_name)),0) FROM sqlite_master",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
        if !(0..=128).contains(&n) || !(0..=262144).contains(&size) {
            return Err(invalid("upgrade schema bound"));
        }
        let schema = conn
            .prepare("SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY type,name")?
            .query_map([], |r| {
                Ok(Object {
                    kind: r.get(0)?,
                    name: r.get(1)?,
                    table: r.get(2)?,
                    sql: r.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut tables = BTreeMap::new();
        let mut total_rows = 0i64;
        let mut total_bytes = 0i64;
        for object in &schema {
            if object.kind != "table" {
                continue;
            }
            let table = quoted(&object.name);
            let columns = conn
                .prepare(&format!("PRAGMA table_info({table})"))?
                .query_map([], |r| r.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()?;
            if columns.is_empty() || columns.len() > 32 {
                return Err(invalid("upgrade column bound"));
            }
            let bytes = columns
                .iter()
                .map(|c| format!("COALESCE(length(CAST({} AS BLOB)),0)", quoted(c)))
                .collect::<Vec<_>>()
                .join("+");
            let (count, size, max): (i64, i64, i64) = conn.query_row(
                &format!(
                    "SELECT COUNT(*),COALESCE(SUM({bytes}),0),COALESCE(MAX({bytes}),0) FROM {table}"
                ),
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
            total_rows = total_rows
                .checked_add(count)
                .ok_or_else(|| invalid("upgrade count overflow"))?;
            total_bytes = total_bytes
                .checked_add(size)
                .ok_or_else(|| invalid("upgrade byte overflow"))?;
            if count < 0
                || size < 0
                || total_rows > 16384
                || total_bytes > 32 * 1024 * 1024
                || max > 4 * 1024 * 1024
            {
                return Err(invalid("upgrade inventory bound"));
            }
            let select = columns
                .iter()
                .map(|c| quoted(c))
                .collect::<Vec<_>>()
                .join(",");
            let mut q = conn.prepare(&format!("SELECT {select} FROM {table}"))?;
            let mut cursor = q.query([])?;
            let mut rows = Vec::new();
            while let Some(row) = cursor.next()? {
                let mut out = Vec::new();
                for i in 0..columns.len() {
                    out.push(match row.get_ref(i)? {
                        ValueRef::Null => Cell::Null,
                        ValueRef::Integer(v) => Cell::Integer(v),
                        ValueRef::Real(v) if v.is_finite() => Cell::Real(v.to_bits()),
                        ValueRef::Real(_) => return Err(invalid("nonfinite SQLite value")),
                        ValueRef::Text(v) => Cell::Text(
                            std::str::from_utf8(v)
                                .map_err(|_| invalid("non UTF8 SQLite text"))?
                                .to_owned(),
                        ),
                        ValueRef::Blob(v) => Cell::Blob(v.to_vec()),
                    });
                }
                rows.push(out);
            }
            rows.sort();
            tables.insert(object.name.clone(), Table { columns, rows });
        }
        Ok(Self {
            schema,
            tables,
            user_version: conn.query_row("PRAGMA user_version", [], |r| r.get(0))?,
            application_id: conn.query_row("PRAGMA application_id", [], |r| r.get(0))?,
        })
    }
    pub fn table(&self, name: &str) -> Result<&Table, AppError> {
        self.tables
            .get(name)
            .ok_or_else(|| invalid("missing upgrade table"))
    }
    // Caller has matched every object against trusted program-generated SQL.
    // Never run a schema string copied from an unrecognized database.
    pub fn shadow(&self) -> Result<DraftStore, AppError> {
        self.shape()?;
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA synchronous=FULL;")?;
        for object in &self.schema {
            if object.kind == "table" && object.name != "sqlite_sequence" {
                conn.execute_batch(
                    object
                        .sql
                        .as_deref()
                        .ok_or_else(|| invalid("missing table SQL"))?,
                )?;
            }
        }
        for (name, table) in &self.tables {
            // AUTOINCREMENT creates sqlite_sequence; copy its exact historical floor.
            if name == "sqlite_sequence" {
                conn.execute("DELETE FROM sqlite_sequence", [])?;
            }
            let columns = table
                .columns
                .iter()
                .map(|c| quoted(c))
                .collect::<Vec<_>>()
                .join(",");
            let placeholders = (0..table.columns.len())
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "INSERT INTO {}({columns}) VALUES({placeholders})",
                quoted(name)
            );
            for row in &table.rows {
                conn.execute(&sql, params_from_iter(row.iter().map(Cell::value)))?;
            }
        }
        for object in &self.schema {
            if object.kind != "table" {
                if let Some(sql) = &object.sql {
                    conn.execute_batch(sql)?;
                }
            }
        }
        conn.execute_batch(&format!(
            "PRAGMA user_version={}; PRAGMA application_id={};",
            self.user_version, self.application_id
        ))?;
        if Image::read(&conn)? != *self {
            return Err(invalid("shadow reconstruction was not lossless"));
        }
        Ok(DraftStore(conn))
    }
    pub fn preservation(&self, after: &Self) -> Result<(), AppError> {
        for (name, table) in &self.tables {
            if name != "composer_draft_sync"
                && name != "composer_draft_owner"
                && after.tables.get(name) != Some(table)
            {
                return Err(invalid("migration changed retained raw table"));
            }
        }
        for object in &self.schema {
            if !after.schema.contains(object) {
                return Err(invalid("migration changed existing schema"));
            }
        }
        if self.user_version != after.user_version || self.application_id != after.application_id {
            return Err(invalid("migration changed database identity"));
        }
        Ok(())
    }
}
impl Table {
    pub fn shape(&self) -> Result<(), AppError> {
        let columns = self
            .columns
            .iter()
            .collect::<std::collections::BTreeSet<_>>();
        if self.columns.is_empty()
            || self.columns.len() > 32
            || columns.len() != self.columns.len()
            || self.rows.len() > 16384
        {
            return Err(invalid("persisted table shape"));
        }
        for row in &self.rows {
            if row.len() != self.columns.len() {
                return Err(invalid("persisted row width"));
            }
            let mut size = 0usize;
            for cell in row {
                size = size
                    .checked_add(match cell {
                        Cell::Text(v) => v.len(),
                        Cell::Blob(v) => v.len(),
                        Cell::Real(v) if !f64::from_bits(*v).is_finite() => {
                            return Err(invalid("persisted nonfinite real"));
                        }
                        _ => 8,
                    })
                    .ok_or_else(|| invalid("row byte overflow"))?;
            }
            if size > 4 * 1024 * 1024 {
                return Err(invalid("persisted row byte bound"));
            }
        }
        Ok(())
    }
}
