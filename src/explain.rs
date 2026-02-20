#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PlanNode {
    pub node_type: String,
    pub estimated_cost: Option<(f64, f64)>,
    pub actual_time: Option<(f64, f64)>,
    pub estimated_rows: Option<u64>,
    pub actual_rows: Option<u64>,
    pub loops: Option<u64>,
    pub details: Vec<String>,
    pub children: Vec<PlanNode>,
    pub depth: usize,
}

#[derive(Debug, Clone)]
pub struct QueryPlan {
    pub root: PlanNode,
    pub total_time: Option<f64>,
    pub planning_time: Option<f64>,
    pub execution_time: Option<f64>,
}

pub fn is_explain_query(query: &str) -> bool {
    let trimmed = query.trim().to_uppercase();
    trimmed.starts_with("EXPLAIN")
}

pub fn parse_explain_output(text: &str) -> Option<QueryPlan> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return None;
    }

    let mut planning_time = None;
    let mut execution_time = None;

    // Extract timing info from the end
    for line in lines.iter().rev() {
        let trimmed = line.trim();
        if let Some(time_str) = trimmed.strip_prefix("Planning Time:") {
            planning_time = parse_time_ms(time_str);
        } else if let Some(time_str) = trimmed.strip_prefix("Execution Time:") {
            execution_time = parse_time_ms(time_str);
        } else if let Some(time_str) = trimmed.strip_prefix("Planning time:") {
            planning_time = parse_time_ms(time_str);
        } else if let Some(time_str) = trimmed.strip_prefix("Execution time:") {
            execution_time = parse_time_ms(time_str);
        }
    }

    // Parse the plan tree
    let plan_lines: Vec<&str> = lines
        .iter()
        .filter(|l| {
            let t = l.trim();
            !t.starts_with("Planning Time:")
                && !t.starts_with("Planning time:")
                && !t.starts_with("Execution Time:")
                && !t.starts_with("Execution time:")
                && !t.starts_with("QUERY PLAN")
                && !t.starts_with("---")
                && !t.is_empty()
        })
        .copied()
        .collect();

    if plan_lines.is_empty() {
        return None;
    }

    let root = parse_node(&plan_lines, 0).0?;

    let total_time = root.actual_time.map(|(_, end)| end);

    Some(QueryPlan {
        root,
        total_time,
        planning_time,
        execution_time,
    })
}

fn parse_time_ms(s: &str) -> Option<f64> {
    let s = s.trim().trim_end_matches("ms").trim();
    s.parse::<f64>().ok()
}

fn parse_node(lines: &[&str], start: usize) -> (Option<PlanNode>, usize) {
    if start >= lines.len() {
        return (None, start);
    }

    let first_line = lines[start];
    let node_indent = get_indent(first_line);

    // Parse the node type and cost/timing info from the first line
    let content = first_line.trim().trim_start_matches("-> ");

    let (node_type, estimated_cost, actual_time, estimated_rows, actual_rows, loops) =
        parse_node_header(content);

    let mut details = Vec::new();
    let mut children = Vec::new();
    let mut idx = start + 1;

    while idx < lines.len() {
        let line = lines[idx];
        let indent = get_indent(line);
        let trimmed = line.trim();

        if indent <= node_indent && !trimmed.starts_with("-> ") && idx > start + 1 {
            // Back at same or lower indent level — this line belongs to parent
            break;
        }

        if trimmed.starts_with("-> ") && indent > node_indent {
            // Child node
            let (child, next_idx) = parse_node(lines, idx);
            if let Some(child) = child {
                children.push(child);
            }
            idx = next_idx;
        } else if indent > node_indent {
            // Detail line for this node
            details.push(trimmed.to_string());
            idx += 1;
        } else {
            break;
        }
    }

    let node = PlanNode {
        node_type,
        estimated_cost,
        actual_time,
        estimated_rows,
        actual_rows,
        loops,
        details,
        children,
        depth: 0,
    };

    (Some(node), idx)
}

fn get_indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

type NodeHeader = (
    String,
    Option<(f64, f64)>,
    Option<(f64, f64)>,
    Option<u64>,
    Option<u64>,
    Option<u64>,
);

fn parse_node_header(s: &str) -> NodeHeader {
    let mut node_type = s.to_string();
    let mut estimated_cost = None;
    let mut actual_time = None;
    let mut estimated_rows = None;
    let mut actual_rows = None;
    let mut loops = None;

    // Extract (cost=X..Y rows=N width=W)
    if let Some(cost_start) = s.find("(cost=") {
        node_type = s[..cost_start].trim().to_string();

        let rest = &s[cost_start..];
        // Parse cost
        if let Some(cost_str) = extract_between(rest, "(cost=", " ") {
            let parts: Vec<&str> = cost_str.split("..").collect();
            if parts.len() == 2 {
                if let (Ok(a), Ok(b)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                    estimated_cost = Some((a, b));
                }
            }
        }
        // Parse estimated rows
        if let Some(rows_str) = extract_between(rest, "rows=", " ") {
            estimated_rows = rows_str.parse::<u64>().ok();
        }

        // Parse actual time
        if let Some(actual_str) = extract_between(rest, "(actual time=", " ") {
            let parts: Vec<&str> = actual_str.split("..").collect();
            if parts.len() == 2 {
                if let (Ok(a), Ok(b)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                    actual_time = Some((a, b));
                }
            }
        }
        // Parse actual rows
        if let Some(arows_str) = extract_between(rest, "rows=", " loops") {
            // This might match the estimated rows= too, so look for the one after "actual"
            if rest.contains("actual") {
                // Find the second "rows=" after "actual"
                if let Some(actual_pos) = rest.find("actual") {
                    let after_actual = &rest[actual_pos..];
                    if let Some(rows_str) = extract_between(after_actual, "rows=", " ") {
                        actual_rows = rows_str.parse::<u64>().ok();
                    }
                }
            } else {
                actual_rows = arows_str.parse::<u64>().ok();
            }
        }
        // Parse loops
        if let Some(loops_str) = extract_between(rest, "loops=", ")") {
            loops = loops_str.parse::<u64>().ok();
        }
    }

    (
        node_type,
        estimated_cost,
        actual_time,
        estimated_rows,
        actual_rows,
        loops,
    )
}

fn extract_between<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let start_pos = s.find(start)? + start.len();
    let remaining = &s[start_pos..];
    let end_pos = remaining.find(end)?;
    Some(&remaining[..end_pos])
}

pub fn node_color_class(node: &PlanNode, total_time: Option<f64>) -> NodeColorClass {
    if let (Some((_, end)), Some(total)) = (node.actual_time, total_time) {
        if total <= 0.0 {
            return NodeColorClass::Fast;
        }
        let ratio = end / total;
        if ratio > 0.3 {
            NodeColorClass::Slow
        } else if ratio > 0.1 {
            NodeColorClass::Moderate
        } else {
            NodeColorClass::Fast
        }
    } else {
        NodeColorClass::Fast
    }
}

pub fn rows_mismatch(node: &PlanNode) -> bool {
    if let (Some(est), Some(actual)) = (node.estimated_rows, node.actual_rows) {
        if est == 0 || actual == 0 {
            return est != actual;
        }
        let ratio = actual as f64 / est as f64;
        !(0.1..=10.0).contains(&ratio)
    } else {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodeColorClass {
    Fast,
    Moderate,
    Slow,
}

pub fn format_duration_ms(ms: f64) -> String {
    if ms >= 1000.0 {
        format!("{:.2}s", ms / 1000.0)
    } else {
        format!("{:.2}ms", ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_explain_query() {
        assert!(is_explain_query("EXPLAIN SELECT 1"));
        assert!(is_explain_query("explain analyze select * from t"));
        assert!(is_explain_query("  EXPLAIN (ANALYZE, BUFFERS) SELECT 1"));
        assert!(!is_explain_query("SELECT 1"));
        assert!(!is_explain_query("-- EXPLAIN SELECT 1"));
    }

    #[test]
    fn test_parse_simple_explain() {
        let output = "Seq Scan on users  (cost=0.00..35.50 rows=2550 width=36)";
        let plan = parse_explain_output(output).unwrap();
        assert_eq!(plan.root.node_type, "Seq Scan on users");
        assert_eq!(plan.root.estimated_cost, Some((0.0, 35.5)));
        assert_eq!(plan.root.estimated_rows, Some(2550));
    }

    #[test]
    fn test_parse_explain_analyze() {
        let output = "\
Seq Scan on users  (cost=0.00..35.50 rows=100 width=36) (actual time=0.010..0.100 rows=100 loops=1)
Planning Time: 0.100 ms
Execution Time: 0.200 ms";
        let plan = parse_explain_output(output).unwrap();
        assert_eq!(plan.root.actual_time, Some((0.01, 0.1)));
        assert_eq!(plan.root.actual_rows, Some(100));
        assert_eq!(plan.root.loops, Some(1));
        assert_eq!(plan.planning_time, Some(0.1));
        assert_eq!(plan.execution_time, Some(0.2));
    }

    #[test]
    fn test_parse_nested_plan() {
        let output = "\
Sort  (cost=100.00..100.25 rows=100 width=40)
  Sort Key: name
  ->  Seq Scan on users  (cost=0.00..35.50 rows=100 width=40)
        Filter: (age > 18)";
        let plan = parse_explain_output(output).unwrap();
        assert_eq!(plan.root.node_type, "Sort");
        assert_eq!(plan.root.children.len(), 1);
        assert_eq!(plan.root.children[0].node_type, "Seq Scan on users");
        assert!(plan.root.children[0]
            .details
            .iter()
            .any(|d| d.contains("Filter")));
    }

    #[test]
    fn test_node_color_class() {
        let fast_node = PlanNode {
            node_type: "Scan".to_string(),
            estimated_cost: None,
            actual_time: Some((0.0, 1.0)),
            estimated_rows: None,
            actual_rows: None,
            loops: None,
            details: vec![],
            children: vec![],
            depth: 0,
        };
        assert_eq!(
            node_color_class(&fast_node, Some(100.0)),
            NodeColorClass::Fast
        );

        let slow_node = PlanNode {
            actual_time: Some((0.0, 50.0)),
            ..fast_node.clone()
        };
        assert_eq!(
            node_color_class(&slow_node, Some(100.0)),
            NodeColorClass::Slow
        );
    }

    #[test]
    fn test_rows_mismatch() {
        let node = PlanNode {
            node_type: "Scan".to_string(),
            estimated_cost: None,
            actual_time: None,
            estimated_rows: Some(10),
            actual_rows: Some(10000),
            loops: None,
            details: vec![],
            children: vec![],
            depth: 0,
        };
        assert!(rows_mismatch(&node));

        let good_node = PlanNode {
            estimated_rows: Some(100),
            actual_rows: Some(95),
            ..node.clone()
        };
        assert!(!rows_mismatch(&good_node));
    }

    #[test]
    fn test_format_duration_ms() {
        assert_eq!(format_duration_ms(0.5), "0.50ms");
        assert_eq!(format_duration_ms(100.0), "100.00ms");
        assert_eq!(format_duration_ms(1500.0), "1.50s");
    }

    #[test]
    fn test_extract_between() {
        assert_eq!(
            extract_between("cost=1.00..2.00 rows=10", "cost=", " "),
            Some("1.00..2.00")
        );
        assert_eq!(
            extract_between("rows=100 width=40", "rows=", " "),
            Some("100")
        );
        assert_eq!(extract_between("no match", "foo=", " "), None);
    }

    #[test]
    fn test_is_not_explain() {
        assert!(!is_explain_query("SELECT * FROM explain_table"));
    }
}
