use anyhow::Result;

/// A parsed metadata filter expression: field, operator, value
#[derive(Debug, Clone)]
pub struct Filter {
    pub field: String,
    pub operator: String,
    pub value: String,
}

/// Parse a filter string like "date >= 2023" into a Filter
///
/// Accepted operators: `=`, `!=`, `>=`, `<=`, `>`, `<`
pub fn parse_filter(input: &str) -> Result<Filter> {
    let re = regex::Regex::new(r"^(\w[.\w]*)\s*(=|!=|>=|<=|>|<)\s*(.+)$").unwrap();
    if let Some(caps) = re.captures(input) {
        Ok(Filter {
            field: caps[1].to_string(),
            operator: caps[2].to_string(),
            value: caps[3].trim().to_string(),
        })
    } else {
        anyhow::bail!(
            "Invalid filter syntax: '{}'. Expected: field operator value (e.g., 'date >= 2023')",
            input
        )
    }
}

/// Build a SQL WHERE clause from filters.
///
/// Returns `(sql_condition, param_values)`.
/// - The `sql_condition` is a string like `AND (json_extract(...) op ?2 AND ...)`,
///   or an empty string if no filters are provided.
/// - Parameter numbering assumes `?1` is already used (e.g., for `note_id`).
pub fn build_filter_sql(filters: &[Filter]) -> (String, Vec<String>) {
    let mut conditions = Vec::new();
    let mut params = Vec::new();

    for f in filters {
        let idx = params.len() + 2; // ?1 is taken by note_id, params start at ?2
        let json_path = format!("$.\"{}\"", f.field);

        // Try numeric comparison if value parses as a number, else string comparison
        if f.value.parse::<f64>().is_ok() {
            conditions.push(format!(
                "CAST(json_extract(nodes.metadata, '{}') AS REAL) {} CAST(?{} AS REAL)",
                json_path, f.operator, idx
            ));
        } else {
            conditions.push(format!(
                "json_extract(nodes.metadata, '{}') {} ?{}",
                json_path, f.operator, idx
            ));
        }
        params.push(f.value.clone());
    }

    let sql = if conditions.is_empty() {
        String::new()
    } else {
        format!("AND ({})", conditions.join(" AND "))
    };

    (sql, params)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_filter_with_ge() {
        let f = parse_filter("date >= 2023").unwrap();
        assert_eq!(f.field, "date");
        assert_eq!(f.operator, ">=");
        assert_eq!(f.value, "2023");
    }

    #[test]
    fn test_parse_eq_filter() {
        let f = parse_filter("category = tutorial").unwrap();
        assert_eq!(f.field, "category");
        assert_eq!(f.operator, "=");
        assert_eq!(f.value, "tutorial");
    }

    #[test]
    fn test_parse_ne_filter() {
        let f = parse_filter("status != archived").unwrap();
        assert_eq!(f.field, "status");
        assert_eq!(f.operator, "!=");
        assert_eq!(f.value, "archived");
    }

    #[test]
    fn test_parse_lt_filter() {
        let f = parse_filter("priority < 5").unwrap();
        assert_eq!(f.field, "priority");
        assert_eq!(f.operator, "<");
        assert_eq!(f.value, "5");
    }

    #[test]
    fn test_parse_le_filter() {
        let f = parse_filter("score <= 0.9").unwrap();
        assert_eq!(f.field, "score");
        assert_eq!(f.operator, "<=");
        assert_eq!(f.value, "0.9");
    }

    #[test]
    fn test_parse_gt_filter() {
        let f = parse_filter("count > 10").unwrap();
        assert_eq!(f.field, "count");
        assert_eq!(f.operator, ">");
        assert_eq!(f.value, "10");
    }

    #[test]
    fn test_parse_dotted_field() {
        let f = parse_filter("nested.field = value").unwrap();
        assert_eq!(f.field, "nested.field");
        assert_eq!(f.operator, "=");
        assert_eq!(f.value, "value");
    }

    #[test]
    fn test_parse_invalid_syntax() {
        assert!(parse_filter("badformat").is_err());
    }

    #[test]
    fn test_parse_empty() {
        assert!(parse_filter("").is_err());
    }

    #[test]
    fn test_parse_no_operator() {
        assert!(parse_filter("justfield").is_err());
    }

    #[test]
    fn test_build_filter_sql_single() {
        let filters = vec![parse_filter("date >= 2023").unwrap()];
        let (sql, params) = build_filter_sql(&filters);
        assert!(
            sql.contains("json_extract"),
            "sql should reference json_extract"
        );
        assert!(sql.contains(">="), "sql should contain operator");
        assert_eq!(params, vec!["2023"]);
    }

    #[test]
    fn test_build_filter_sql_multiple() {
        let filters = vec![
            parse_filter("category = tutorial").unwrap(),
            parse_filter("date >= 2022").unwrap(),
        ];
        let (sql, params) = build_filter_sql(&filters);
        assert!(sql.contains("AND"), "multiple filters should join with AND");
        assert_eq!(params.len(), 2);
        assert_eq!(params[0], "tutorial");
        assert_eq!(params[1], "2022");
    }

    #[test]
    fn test_build_filter_sql_empty() {
        let (sql, params) = build_filter_sql(&[]);
        assert!(sql.is_empty());
        assert!(params.is_empty());
    }

    #[test]
    fn test_build_filter_sql_string_value() {
        let filters = vec![parse_filter("category = tutorial").unwrap()];
        let (sql, _params) = build_filter_sql(&filters);
        // String values should NOT wrap in CAST
        assert!(!sql.contains("CAST"), "string values should not use CAST");
    }

    #[test]
    fn test_build_filter_sql_numeric_value() {
        let filters = vec![parse_filter("count > 5").unwrap()];
        let (sql, _params) = build_filter_sql(&filters);
        // Numeric values should use CAST for proper comparison
        assert!(sql.contains("CAST"), "numeric values should use CAST");
    }

    #[test]
    fn test_filter_clone_and_debug() {
        let f = parse_filter("a = b").unwrap();
        let f2 = f.clone();
        assert_eq!(format!("{:?}", f2), format!("{:?}", f));
    }
}
