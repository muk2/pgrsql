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
    "BETWEEN",
    "EXISTS",
    "CASE",
    "WHEN",
    "THEN",
    "ELSE",
    "END",
    "JOIN",
    "INNER",
    "LEFT",
    "RIGHT",
    "FULL",
    "OUTER",
    "CROSS",
    "ON",
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
    "INSERT",
    "INTO",
    "VALUES",
    "UPDATE",
    "SET",
    "DELETE",
    "CREATE",
    "ALTER",
    "DROP",
    "TRUNCATE",
    "TABLE",
    "INDEX",
    "VIEW",
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
    "GRANT",
    "REVOKE",
    "ALL",
    "PRIVILEGES",
    "TO",
    "PUBLIC",
    "BEGIN",
    "COMMIT",
    "ROLLBACK",
    "TRANSACTION",
    "SAVEPOINT",
    "WITH",
    "AS",
    "RECURSIVE",
    "UNION",
    "INTERSECT",
    "EXCEPT",
    "DISTINCT",
    "COUNT",
    "SUM",
    "AVG",
    "MIN",
    "MAX",
    "COALESCE",
    "NULLIF",
    "CAST",
    "EXTRACT",
    "DATE",
    "TIME",
    "TIMESTAMP",
    "INTERVAL",
    "TRUE",
    "FALSE",
    "RETURNING",
    "OVER",
    "PARTITION",
    "WINDOW",
    "ROW_NUMBER",
    "RANK",
    "DENSE_RANK",
    "LAG",
    "LEAD",
    "FIRST_VALUE",
    "LAST_VALUE",
    "SCHEMA",
    "DATABASE",
    "IF",
    "EXPLAIN",
    "ANALYZE",
    "VERBOSE",
];

pub const SQL_TYPES: &[&str] = &[
    "INTEGER",
    "INT",
    "SMALLINT",
    "BIGINT",
    "SERIAL",
    "BIGSERIAL",
    "REAL",
    "DOUBLE",
    "PRECISION",
    "NUMERIC",
    "DECIMAL",
    "FLOAT",
    "VARCHAR",
    "CHAR",
    "TEXT",
    "CHARACTER",
    "VARYING",
    "BOOLEAN",
    "BOOL",
    "DATE",
    "TIME",
    "TIMESTAMP",
    "TIMESTAMPTZ",
    "INTERVAL",
    "UUID",
    "JSON",
    "JSONB",
    "BYTEA",
    "ARRAY",
    "POINT",
    "LINE",
    "LSEG",
    "BOX",
    "PATH",
    "POLYGON",
    "CIRCLE",
    "CIDR",
    "INET",
    "MACADDR",
    "BIT",
    "VARBIT",
    "XML",
    "MONEY",
];

pub fn is_sql_keyword(word: &str) -> bool {
    SQL_KEYWORDS.contains(&word.to_uppercase().as_str())
}

pub fn is_sql_type(word: &str) -> bool {
    SQL_TYPES.contains(&word.to_uppercase().as_str())
}
