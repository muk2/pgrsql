use ratatui::style::{Color, Modifier, Style};

#[allow(dead_code)]
pub struct Theme {
    // Background colors
    pub bg_primary: Color,
    pub bg_secondary: Color,
    pub bg_tertiary: Color,
    pub bg_selected: Color,
    pub bg_highlight: Color,

    // Text colors
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub text_accent: Color,

    // Status colors
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,

    // Syntax highlighting
    pub syntax_keyword: Color,
    pub syntax_string: Color,
    pub syntax_number: Color,
    pub syntax_comment: Color,
    pub syntax_function: Color,
    pub syntax_operator: Color,
    pub syntax_type: Color,

    // UI elements
    pub border: Color,
    pub border_focused: Color,
    pub cursor: Color,
    pub selection: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

#[allow(dead_code)]
impl Theme {
    pub fn dark() -> Self {
        Self {
            // Background colors - dark blue-gray palette
            bg_primary: Color::Rgb(24, 26, 33),
            bg_secondary: Color::Rgb(30, 33, 43),
            bg_tertiary: Color::Rgb(40, 44, 57),
            bg_selected: Color::Rgb(50, 56, 74),
            bg_highlight: Color::Rgb(60, 67, 87),

            // Text colors
            text_primary: Color::Rgb(230, 233, 240),
            text_secondary: Color::Rgb(180, 185, 200),
            text_muted: Color::Rgb(120, 125, 145),
            text_accent: Color::Rgb(100, 180, 255),

            // Status colors
            success: Color::Rgb(80, 200, 120),
            warning: Color::Rgb(255, 190, 80),
            error: Color::Rgb(255, 100, 100),
            info: Color::Rgb(100, 180, 255),

            // Syntax highlighting
            syntax_keyword: Color::Rgb(198, 120, 221), // Purple
            syntax_string: Color::Rgb(152, 195, 121),  // Green
            syntax_number: Color::Rgb(209, 154, 102),  // Orange
            syntax_comment: Color::Rgb(92, 99, 112),   // Gray
            syntax_function: Color::Rgb(97, 175, 239), // Blue
            syntax_operator: Color::Rgb(86, 182, 194), // Cyan
            syntax_type: Color::Rgb(229, 192, 123),    // Yellow

            // UI elements
            border: Color::Rgb(60, 65, 80),
            border_focused: Color::Rgb(100, 180, 255),
            cursor: Color::Rgb(255, 255, 255),
            selection: Color::Rgb(60, 80, 120),
        }
    }

    pub fn light() -> Self {
        Self {
            bg_primary: Color::Rgb(250, 250, 252),
            bg_secondary: Color::Rgb(240, 240, 245),
            bg_tertiary: Color::Rgb(230, 230, 238),
            bg_selected: Color::Rgb(210, 220, 240),
            bg_highlight: Color::Rgb(200, 210, 230),

            text_primary: Color::Rgb(30, 35, 45),
            text_secondary: Color::Rgb(70, 75, 90),
            text_muted: Color::Rgb(130, 135, 150),
            text_accent: Color::Rgb(0, 100, 200),

            success: Color::Rgb(40, 160, 80),
            warning: Color::Rgb(200, 140, 0),
            error: Color::Rgb(200, 60, 60),
            info: Color::Rgb(0, 120, 200),

            syntax_keyword: Color::Rgb(150, 70, 180),
            syntax_string: Color::Rgb(80, 140, 60),
            syntax_number: Color::Rgb(180, 100, 40),
            syntax_comment: Color::Rgb(140, 145, 160),
            syntax_function: Color::Rgb(40, 120, 200),
            syntax_operator: Color::Rgb(30, 140, 150),
            syntax_type: Color::Rgb(180, 140, 40),

            border: Color::Rgb(200, 205, 215),
            border_focused: Color::Rgb(0, 120, 200),
            cursor: Color::Rgb(0, 0, 0),
            selection: Color::Rgb(180, 200, 240),
        }
    }

    // Style helpers
    pub fn normal(&self) -> Style {
        Style::default().fg(self.text_primary).bg(self.bg_primary)
    }

    pub fn header(&self) -> Style {
        Style::default()
            .fg(self.text_primary)
            .bg(self.bg_secondary)
            .add_modifier(Modifier::BOLD)
    }

    pub fn selected(&self) -> Style {
        Style::default().fg(self.text_primary).bg(self.bg_selected)
    }

    pub fn muted(&self) -> Style {
        Style::default().fg(self.text_muted)
    }

    pub fn border_style(&self, focused: bool) -> Style {
        if focused {
            Style::default().fg(self.border_focused)
        } else {
            Style::default().fg(self.border)
        }
    }

    pub fn status_success(&self) -> Style {
        Style::default().fg(self.success)
    }

    pub fn status_error(&self) -> Style {
        Style::default().fg(self.error)
    }

    pub fn status_warning(&self) -> Style {
        Style::default().fg(self.warning)
    }
}

// SQL Keywords for syntax highlighting
pub const SQL_KEYWORDS: &[&str] = &[
    // DML
    "SELECT",
    "FROM",
    "WHERE",
    "AND",
    "OR",
    "NOT",
    "IN",
    "IS",
    "NULL",
    "LIKE",
    "ILIKE",
    "SIMILAR",
    "BETWEEN",
    "EXISTS",
    "ANY",
    "SOME",
    // CASE expressions
    "CASE",
    "WHEN",
    "THEN",
    "ELSE",
    "END",
    // Joins
    "JOIN",
    "INNER",
    "LEFT",
    "RIGHT",
    "FULL",
    "OUTER",
    "CROSS",
    "NATURAL",
    "LATERAL",
    "ON",
    "USING",
    // Ordering and grouping
    "GROUP",
    "BY",
    "HAVING",
    "ORDER",
    "ASC",
    "DESC",
    "NULLS",
    "FIRST",
    "LAST",
    "LIMIT",
    "OFFSET",
    "FETCH",
    "NEXT",
    "ROWS",
    "ONLY",
    "PERCENT",
    "TIES",
    // Modification
    "INSERT",
    "INTO",
    "VALUES",
    "UPDATE",
    "SET",
    "DELETE",
    "MERGE",
    "UPSERT",
    "RETURNING",
    "ON CONFLICT",
    "DO",
    "NOTHING",
    // DDL
    "CREATE",
    "ALTER",
    "DROP",
    "TRUNCATE",
    "RENAME",
    "REPLACE",
    "TABLE",
    "INDEX",
    "VIEW",
    "MATERIALIZED",
    "TEMPORARY",
    "TEMP",
    "UNLOGGED",
    "CONCURRENTLY",
    // Constraints
    "PRIMARY",
    "KEY",
    "FOREIGN",
    "REFERENCES",
    "UNIQUE",
    "CHECK",
    "DEFAULT",
    "CONSTRAINT",
    "CASCADE",
    "RESTRICT",
    "NO",
    "ACTION",
    "DEFERRABLE",
    "INITIALLY",
    "DEFERRED",
    "IMMEDIATE",
    // Privileges
    "GRANT",
    "REVOKE",
    "ALL",
    "PRIVILEGES",
    "TO",
    "PUBLIC",
    "OWNER",
    // Transactions
    "BEGIN",
    "COMMIT",
    "ROLLBACK",
    "TRANSACTION",
    "SAVEPOINT",
    "RELEASE",
    "ISOLATION",
    "LEVEL",
    "SERIALIZABLE",
    "REPEATABLE",
    "READ",
    "COMMITTED",
    "UNCOMMITTED",
    // CTEs & set operations
    "WITH",
    "AS",
    "RECURSIVE",
    "UNION",
    "INTERSECT",
    "EXCEPT",
    "DISTINCT",
    // Grouping sets (OLAP)
    "GROUPING",
    "SETS",
    "CUBE",
    "ROLLUP",
    // Window functions
    "OVER",
    "PARTITION",
    "WINDOW",
    "RANGE",
    "UNBOUNDED",
    "PRECEDING",
    "FOLLOWING",
    "CURRENT",
    "ROW",
    "EXCLUDE",
    // Expressions
    "CAST",
    "EXTRACT",
    "COALESCE",
    "NULLIF",
    "GREATEST",
    "LEAST",
    // Literals
    "TRUE",
    "FALSE",
    // Schema objects
    "SCHEMA",
    "DATABASE",
    "SEQUENCE",
    "TRIGGER",
    "FUNCTION",
    "PROCEDURE",
    "TYPE",
    "DOMAIN",
    "EXTENSION",
    "RULE",
    "POLICY",
    "ROLE",
    "USER",
    "TABLESPACE",
    "COMMENT",
    // Control flow
    "IF",
    "ELSIF",
    "LOOP",
    "WHILE",
    "FOR",
    "FOREACH",
    "EXIT",
    "CONTINUE",
    "RETURN",
    "RAISE",
    "EXCEPTION",
    "PERFORM",
    "EXECUTE",
    "DECLARE",
    "LANGUAGE",
    "RETURNS",
    "SETOF",
    "VOLATILE",
    "STABLE",
    "IMMUTABLE",
    "SECURITY",
    "DEFINER",
    "INVOKER",
    // Explain & maintenance
    "EXPLAIN",
    "ANALYZE",
    "VERBOSE",
    "VACUUM",
    "REINDEX",
    "CLUSTER",
    "REFRESH",
    // Misc
    "COPY",
    "LISTEN",
    "NOTIFY",
    "UNLISTEN",
    "LOCK",
    "SHARE",
    "EXCLUSIVE",
    "ACCESS",
    "NOWAIT",
    "SKIP",
    "LOCKED",
    "INHERITS",
    "INHERIT",
    "NOINHERIT",
    "OF",
    "ONLY",
    "SHOW",
    "RESET",
    "DISCARD",
    "PREPARE",
    "DEALLOCATE",
];

// SQL built-in functions (highlighted differently from keywords)
pub const SQL_FUNCTIONS: &[&str] = &[
    // Aggregate functions
    "COUNT",
    "SUM",
    "AVG",
    "MIN",
    "MAX",
    "ARRAY_AGG",
    "STRING_AGG",
    "BOOL_AND",
    "BOOL_OR",
    "BIT_AND",
    "BIT_OR",
    "EVERY",
    "JSON_AGG",
    "JSONB_AGG",
    "JSON_OBJECT_AGG",
    "JSONB_OBJECT_AGG",
    "XMLAGG",
    "PERCENTILE_CONT",
    "PERCENTILE_DISC",
    "MODE",
    "CORR",
    "COVAR_POP",
    "COVAR_SAMP",
    "REGR_AVGX",
    "REGR_AVGY",
    "REGR_COUNT",
    "REGR_INTERCEPT",
    "REGR_R2",
    "REGR_SLOPE",
    "REGR_SXX",
    "REGR_SXY",
    "REGR_SYY",
    "STDDEV",
    "STDDEV_POP",
    "STDDEV_SAMP",
    "VARIANCE",
    "VAR_POP",
    "VAR_SAMP",
    // Window functions
    "ROW_NUMBER",
    "RANK",
    "DENSE_RANK",
    "NTILE",
    "LAG",
    "LEAD",
    "FIRST_VALUE",
    "LAST_VALUE",
    "NTH_VALUE",
    "CUME_DIST",
    "PERCENT_RANK",
    // String functions
    "LENGTH",
    "UPPER",
    "LOWER",
    "TRIM",
    "LTRIM",
    "RTRIM",
    "BTRIM",
    "SUBSTRING",
    "SUBSTR",
    "POSITION",
    "STRPOS",
    "REPLACE",
    "TRANSLATE",
    "CONCAT",
    "CONCAT_WS",
    "REPEAT",
    "REVERSE",
    "LEFT",
    "RIGHT",
    "LPAD",
    "RPAD",
    "INITCAP",
    "CHR",
    "ASCII",
    "MD5",
    "ENCODE",
    "DECODE",
    "REGEXP_MATCH",
    "REGEXP_MATCHES",
    "REGEXP_REPLACE",
    "REGEXP_SPLIT_TO_TABLE",
    "REGEXP_SPLIT_TO_ARRAY",
    "SPLIT_PART",
    "FORMAT",
    "QUOTE_IDENT",
    "QUOTE_LITERAL",
    "QUOTE_NULLABLE",
    "TO_HEX",
    "TO_ASCII",
    // Numeric functions
    "ABS",
    "CEIL",
    "CEILING",
    "FLOOR",
    "ROUND",
    "TRUNC",
    "MOD",
    "POWER",
    "SQRT",
    "CBRT",
    "EXP",
    "LN",
    "LOG",
    "LOG10",
    "SIGN",
    "PI",
    "RANDOM",
    "SETSEED",
    "GREATEST",
    "LEAST",
    "WIDTH_BUCKET",
    "SCALE",
    "DEGREES",
    "RADIANS",
    "SIN",
    "COS",
    "TAN",
    "ASIN",
    "ACOS",
    "ATAN",
    "ATAN2",
    // Date/time functions
    "NOW",
    "CURRENT_DATE",
    "CURRENT_TIME",
    "CURRENT_TIMESTAMP",
    "LOCALTIME",
    "LOCALTIMESTAMP",
    "CLOCK_TIMESTAMP",
    "STATEMENT_TIMESTAMP",
    "TRANSACTION_TIMESTAMP",
    "TIMEOFDAY",
    "AGE",
    "DATE_PART",
    "DATE_TRUNC",
    "EXTRACT",
    "ISFINITE",
    "MAKE_DATE",
    "MAKE_TIME",
    "MAKE_TIMESTAMP",
    "MAKE_TIMESTAMPTZ",
    "MAKE_INTERVAL",
    "TO_TIMESTAMP",
    "TO_DATE",
    "TO_CHAR",
    "TO_NUMBER",
    "JUSTIFY_DAYS",
    "JUSTIFY_HOURS",
    "JUSTIFY_INTERVAL",
    "GENERATE_SERIES",
    // JSON/JSONB functions
    "JSON_BUILD_OBJECT",
    "JSONB_BUILD_OBJECT",
    "JSON_BUILD_ARRAY",
    "JSONB_BUILD_ARRAY",
    "JSON_EXTRACT_PATH",
    "JSONB_EXTRACT_PATH",
    "JSON_EXTRACT_PATH_TEXT",
    "JSONB_EXTRACT_PATH_TEXT",
    "JSON_ARRAY_LENGTH",
    "JSONB_ARRAY_LENGTH",
    "JSON_EACH",
    "JSONB_EACH",
    "JSON_EACH_TEXT",
    "JSONB_EACH_TEXT",
    "JSON_OBJECT_KEYS",
    "JSONB_OBJECT_KEYS",
    "JSON_POPULATE_RECORD",
    "JSONB_POPULATE_RECORD",
    "JSON_TO_RECORD",
    "JSONB_TO_RECORD",
    "JSON_STRIP_NULLS",
    "JSONB_STRIP_NULLS",
    "JSONB_SET",
    "JSONB_INSERT",
    "JSONB_PRETTY",
    "JSON_TYPEOF",
    "JSONB_TYPEOF",
    "JSONB_PATH_EXISTS",
    "JSONB_PATH_MATCH",
    "JSONB_PATH_QUERY",
    "JSONB_PATH_QUERY_ARRAY",
    "JSONB_PATH_QUERY_FIRST",
    "ROW_TO_JSON",
    "TO_JSON",
    "TO_JSONB",
    // Array functions
    "ARRAY_APPEND",
    "ARRAY_CAT",
    "ARRAY_DIMS",
    "ARRAY_FILL",
    "ARRAY_LENGTH",
    "ARRAY_LOWER",
    "ARRAY_NDIMS",
    "ARRAY_POSITION",
    "ARRAY_POSITIONS",
    "ARRAY_PREPEND",
    "ARRAY_REMOVE",
    "ARRAY_REPLACE",
    "ARRAY_TO_STRING",
    "ARRAY_UPPER",
    "CARDINALITY",
    "STRING_TO_ARRAY",
    "UNNEST",
    // Conditional expressions
    "COALESCE",
    "NULLIF",
    "GREATEST",
    "LEAST",
    // Type casting
    "CAST",
    // System info functions
    "CURRENT_USER",
    "CURRENT_SCHEMA",
    "CURRENT_DATABASE",
    "CURRENT_CATALOG",
    "SESSION_USER",
    "PG_TYPEOF",
    "VERSION",
    "HAS_TABLE_PRIVILEGE",
    "HAS_SCHEMA_PRIVILEGE",
    "HAS_DATABASE_PRIVILEGE",
    // Full-text search
    "TO_TSVECTOR",
    "TO_TSQUERY",
    "PLAINTO_TSQUERY",
    "PHRASETO_TSQUERY",
    "WEBSEARCH_TO_TSQUERY",
    "TS_RANK",
    "TS_RANK_CD",
    "TS_HEADLINE",
    "TSVECTOR_TO_ARRAY",
    "SETWEIGHT",
    // Sequence functions
    "NEXTVAL",
    "CURRVAL",
    "SETVAL",
    "LASTVAL",
    // Misc
    "GENERATE_SERIES",
    "GENERATE_SUBSCRIPTS",
    "PG_SLEEP",
    "PG_NOTIFY",
    "PG_CANCEL_BACKEND",
    "PG_TERMINATE_BACKEND",
    "PG_TABLE_SIZE",
    "PG_TOTAL_RELATION_SIZE",
    "PG_RELATION_SIZE",
    "PG_SIZE_PRETTY",
    "PG_COLUMN_SIZE",
    "EXISTS",
];

pub const SQL_TYPES: &[&str] = &[
    // Numeric
    "INTEGER",
    "INT",
    "INT2",
    "INT4",
    "INT8",
    "SMALLINT",
    "BIGINT",
    "SERIAL",
    "SMALLSERIAL",
    "BIGSERIAL",
    "REAL",
    "DOUBLE",
    "PRECISION",
    "NUMERIC",
    "DECIMAL",
    "FLOAT",
    "FLOAT4",
    "FLOAT8",
    // Text
    "VARCHAR",
    "CHAR",
    "TEXT",
    "CHARACTER",
    "VARYING",
    "NAME",
    "BPCHAR",
    "CITEXT",
    // Boolean
    "BOOLEAN",
    "BOOL",
    // Date/time
    "DATE",
    "TIME",
    "TIMESTAMP",
    "TIMESTAMPTZ",
    "TIMETZ",
    "INTERVAL",
    // Binary
    "BYTEA",
    // UUID
    "UUID",
    // JSON
    "JSON",
    "JSONB",
    "JSONPATH",
    // XML
    "XML",
    // Array
    "ARRAY",
    // Geometric
    "POINT",
    "LINE",
    "LSEG",
    "BOX",
    "PATH",
    "POLYGON",
    "CIRCLE",
    // Network
    "CIDR",
    "INET",
    "MACADDR",
    "MACADDR8",
    // Bit string
    "BIT",
    "VARBIT",
    // Money
    "MONEY",
    // Full-text search
    "TSVECTOR",
    "TSQUERY",
    // Range types
    "INT4RANGE",
    "INT8RANGE",
    "NUMRANGE",
    "TSRANGE",
    "TSTZRANGE",
    "DATERANGE",
    "INT4MULTIRANGE",
    "INT8MULTIRANGE",
    "NUMMULTIRANGE",
    "TSMULTIRANGE",
    "TSTZMULTIRANGE",
    "DATEMULTIRANGE",
    // OID types
    "OID",
    "REGCLASS",
    "REGTYPE",
    "REGPROC",
    "REGPROCEDURE",
    "REGOPER",
    "REGOPERATOR",
    "REGNAMESPACE",
    "REGROLE",
    "REGCONFIG",
    "REGDICTIONARY",
    // Pseudo-types
    "VOID",
    "RECORD",
    "TRIGGER",
    "EVENT_TRIGGER",
    "ANYELEMENT",
    "ANYARRAY",
    "ANYNONARRAY",
    "ANYENUM",
    "ANYRANGE",
    "ANYMULTIRANGE",
    "ANYCOMPATIBLE",
    "CSTRING",
    "INTERNAL",
];

pub fn is_sql_keyword(word: &str) -> bool {
    SQL_KEYWORDS.contains(&word.to_uppercase().as_str())
}

pub fn is_sql_type(word: &str) -> bool {
    SQL_TYPES.contains(&word.to_uppercase().as_str())
}

pub fn is_sql_function(word: &str) -> bool {
    SQL_FUNCTIONS.contains(&word.to_uppercase().as_str())
}
