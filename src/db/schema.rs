use anyhow::Result;
use tokio_postgres::Client;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DatabaseInfo {
    pub name: String,
    pub owner: String,
    pub encoding: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SchemaInfo {
    pub name: String,
    pub owner: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TableInfo {
    pub name: String,
    pub schema: String,
    pub table_type: TableType,
    pub row_estimate: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TableType {
    Table,
    View,
    MaterializedView,
    ForeignTable,
}

#[allow(dead_code)]
impl TableType {
    pub fn icon(&self) -> &'static str {
        match self {
            TableType::Table => "󰓫",
            TableType::View => "󰈈",
            TableType::MaterializedView => "󰈈",
            TableType::ForeignTable => "󰒍",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            TableType::Table => "TABLE",
            TableType::View => "VIEW",
            TableType::MaterializedView => "MVIEW",
            TableType::ForeignTable => "FOREIGN",
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ColumnDetails {
    pub name: String,
    pub data_type: String,
    pub is_nullable: bool,
    pub is_primary_key: bool,
    pub default_value: Option<String>,
    pub ordinal_position: i32,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct IndexInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub is_unique: bool,
    pub is_primary: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_type_icon() {
        assert_eq!(TableType::Table.icon(), "󰓫");
        assert_eq!(TableType::View.icon(), "󰈈");
        assert_eq!(TableType::MaterializedView.icon(), "󰈈");
        assert_eq!(TableType::ForeignTable.icon(), "󰒍");
    }

    #[test]
    fn test_table_type_label() {
        assert_eq!(TableType::Table.label(), "TABLE");
        assert_eq!(TableType::View.label(), "VIEW");
        assert_eq!(TableType::MaterializedView.label(), "MVIEW");
        assert_eq!(TableType::ForeignTable.label(), "FOREIGN");
    }

    #[test]
    fn test_table_type_equality() {
        assert_eq!(TableType::Table, TableType::Table);
        assert_ne!(TableType::Table, TableType::View);
    }

    #[test]
    fn test_database_info_clone() {
        let db = DatabaseInfo {
            name: "testdb".into(),
            owner: "postgres".into(),
            encoding: "UTF8".into(),
        };
        let cloned = db.clone();
        assert_eq!(cloned.name, "testdb");
    }

    #[test]
    fn test_schema_info_clone() {
        let schema = SchemaInfo {
            name: "public".into(),
            owner: "postgres".into(),
        };
        let cloned = schema.clone();
        assert_eq!(cloned.name, "public");
    }

    #[test]
    fn test_table_info_clone() {
        let table = TableInfo {
            name: "users".into(),
            schema: "public".into(),
            table_type: TableType::Table,
            row_estimate: 1000,
        };
        let cloned = table.clone();
        assert_eq!(cloned.name, "users");
        assert_eq!(cloned.row_estimate, 1000);
    }

    #[test]
    fn test_column_details() {
        let col = ColumnDetails {
            name: "id".into(),
            data_type: "integer".into(),
            is_nullable: false,
            is_primary_key: true,
            default_value: Some("nextval('users_id_seq')".into()),
            ordinal_position: 1,
        };
        assert_eq!(col.name, "id");
        assert!(col.is_primary_key);
        assert!(!col.is_nullable);
    }

    #[test]
    fn test_index_info() {
        let idx = IndexInfo {
            name: "users_pkey".into(),
            columns: vec!["id".into()],
            is_unique: true,
            is_primary: true,
        };
        assert_eq!(idx.name, "users_pkey");
        assert!(idx.is_primary);
    }
}

pub async fn get_databases(client: &Client) -> Result<Vec<DatabaseInfo>> {
    let rows = client
        .query(
            r#"
            SELECT
                datname as name,
                pg_catalog.pg_get_userbyid(datdba) as owner,
                pg_catalog.pg_encoding_to_char(encoding) as encoding
            FROM pg_catalog.pg_database
            WHERE datistemplate = false
            ORDER BY datname
            "#,
            &[],
        )
        .await?;

    let databases = rows
        .iter()
        .map(|row| DatabaseInfo {
            name: row.get("name"),
            owner: row.get("owner"),
            encoding: row.get("encoding"),
        })
        .collect();

    Ok(databases)
}

pub async fn get_schemas(client: &Client) -> Result<Vec<SchemaInfo>> {
    let rows = client
        .query(
            r#"
            SELECT
                schema_name as name,
                schema_owner as owner
            FROM information_schema.schemata
            WHERE schema_name NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
            ORDER BY schema_name
            "#,
            &[],
        )
        .await?;

    let schemas = rows
        .iter()
        .map(|row| SchemaInfo {
            name: row.get("name"),
            owner: row.get("owner"),
        })
        .collect();

    Ok(schemas)
}

pub async fn get_tables(client: &Client, schema: &str) -> Result<Vec<TableInfo>> {
    let rows = client
        .query(
            r#"
            SELECT
                c.relname as name,
                n.nspname as schema,
                CASE c.relkind
                    WHEN 'r' THEN 'table'
                    WHEN 'v' THEN 'view'
                    WHEN 'm' THEN 'materialized_view'
                    WHEN 'f' THEN 'foreign_table'
                    ELSE 'other'
                END as table_type,
                COALESCE(c.reltuples::bigint, 0) as row_estimate
            FROM pg_catalog.pg_class c
            JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = $1
              AND c.relkind IN ('r', 'v', 'm', 'f')
            ORDER BY c.relname
            "#,
            &[&schema],
        )
        .await?;

    let tables = rows
        .iter()
        .map(|row| {
            let type_str: String = row.get("table_type");
            let table_type = match type_str.as_str() {
                "table" => TableType::Table,
                "view" => TableType::View,
                "materialized_view" => TableType::MaterializedView,
                "foreign_table" => TableType::ForeignTable,
                _ => TableType::Table,
            };

            TableInfo {
                name: row.get("name"),
                schema: row.get("schema"),
                table_type,
                row_estimate: row.get("row_estimate"),
            }
        })
        .collect();

    Ok(tables)
}

#[allow(dead_code)]
pub async fn get_columns(client: &Client, schema: &str, table: &str) -> Result<Vec<ColumnDetails>> {
    let rows = client
        .query(
            r#"
            SELECT
                c.column_name as name,
                c.data_type,
                c.is_nullable = 'YES' as is_nullable,
                COALESCE(tc.constraint_type = 'PRIMARY KEY', false) as is_primary_key,
                c.column_default as default_value,
                c.ordinal_position
            FROM information_schema.columns c
            LEFT JOIN information_schema.key_column_usage kcu
                ON c.table_schema = kcu.table_schema
                AND c.table_name = kcu.table_name
                AND c.column_name = kcu.column_name
            LEFT JOIN information_schema.table_constraints tc
                ON kcu.constraint_name = tc.constraint_name
                AND kcu.table_schema = tc.table_schema
                AND tc.constraint_type = 'PRIMARY KEY'
            WHERE c.table_schema = $1 AND c.table_name = $2
            ORDER BY c.ordinal_position
            "#,
            &[&schema, &table],
        )
        .await?;

    let columns = rows
        .iter()
        .map(|row| ColumnDetails {
            name: row.get("name"),
            data_type: row.get("data_type"),
            is_nullable: row.get("is_nullable"),
            is_primary_key: row.get("is_primary_key"),
            default_value: row.get("default_value"),
            ordinal_position: row.get("ordinal_position"),
        })
        .collect();

    Ok(columns)
}

#[allow(dead_code)]
pub async fn get_indexes(client: &Client, schema: &str, table: &str) -> Result<Vec<IndexInfo>> {
    let rows = client
        .query(
            r#"
            SELECT
                i.relname as index_name,
                array_agg(a.attname ORDER BY array_position(ix.indkey, a.attnum)) as columns,
                ix.indisunique as is_unique,
                ix.indisprimary as is_primary
            FROM pg_catalog.pg_index ix
            JOIN pg_catalog.pg_class t ON t.oid = ix.indrelid
            JOIN pg_catalog.pg_class i ON i.oid = ix.indexrelid
            JOIN pg_catalog.pg_namespace n ON n.oid = t.relnamespace
            JOIN pg_catalog.pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(ix.indkey)
            WHERE n.nspname = $1 AND t.relname = $2
            GROUP BY i.relname, ix.indisunique, ix.indisprimary
            ORDER BY i.relname
            "#,
            &[&schema, &table],
        )
        .await?;

    let indexes = rows
        .iter()
        .map(|row| {
            let columns: Vec<String> = row.get("columns");
            IndexInfo {
                name: row.get("index_name"),
                columns,
                is_unique: row.get("is_unique"),
                is_primary: row.get("is_primary"),
            }
        })
        .collect();

    Ok(indexes)
}

#[allow(dead_code)]
pub async fn get_table_ddl(client: &Client, schema: &str, table: &str) -> Result<String> {
    // Get columns
    let columns = get_columns(client, schema, table).await?;

    let mut ddl = format!("CREATE TABLE {}.{} (\n", schema, table);

    for (i, col) in columns.iter().enumerate() {
        let null_str = if col.is_nullable { "" } else { " NOT NULL" };
        let default_str = col
            .default_value
            .as_ref()
            .map(|d| format!(" DEFAULT {}", d))
            .unwrap_or_default();
        let pk_str = if col.is_primary_key {
            " PRIMARY KEY"
        } else {
            ""
        };

        let comma = if i < columns.len() - 1 { "," } else { "" };

        ddl.push_str(&format!(
            "    {} {}{}{}{}{}\n",
            col.name, col.data_type, null_str, default_str, pk_str, comma
        ));
    }

    ddl.push_str(");\n");

    Ok(ddl)
}
