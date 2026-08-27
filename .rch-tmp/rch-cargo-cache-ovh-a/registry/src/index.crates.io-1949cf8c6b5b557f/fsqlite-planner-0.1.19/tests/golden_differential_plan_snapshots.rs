use fsqlite_ast::{SelectStatement, Statement};
use fsqlite_parser::parse_first_statement_with_tail;
use fsqlite_planner::differential::explain_differential_view_plan;

fn parse_select(sql: &str) -> Result<SelectStatement, String> {
    let Some((stmt, consumed)) =
        parse_first_statement_with_tail(sql).map_err(|error| error.to_string())?
    else {
        return Err("expected a parsed SELECT statement".to_owned());
    };
    if consumed != sql.len() {
        return Err(format!(
            "parser consumed {consumed} bytes from a {} byte statement",
            sql.len()
        ));
    }

    match stmt {
        Statement::Select(select) => Ok(select),
        other => Err(format!("expected SELECT statement, got {other:?}")),
    }
}

fn render_differential_plan_case(name: &str, sql: &str) -> Result<String, String> {
    let select = parse_select(sql)?;
    let explain = explain_differential_view_plan(&select).map_err(|error| error.to_string())?;

    Ok(format!("case: {name}\nsql: {sql}\nstatus: ok\n\n{explain}"))
}

#[test]
fn golden_planner_differential_plan_family() -> Result<(), String> {
    let rowset = render_differential_plan_case(
        "differential_rowset_literal_filters",
        "SELECT id, name \
             FROM users \
             WHERE status = 'paid' AND tenant_id = 7",
    )?;
    insta::assert_snapshot!("differential_rowset_literal_filters", rowset);

    let grouped_join = render_differential_plan_case(
        "differential_grouped_aggregate_join",
        "SELECT u.id AS user_id, COUNT(*) AS n_orders, SUM(o.total) AS gross_total \
             FROM users AS u \
             INNER JOIN orders AS o ON u.id = o.user_id AND u.tenant_id = o.tenant_id \
             WHERE o.status = 'paid' \
             GROUP BY u.id",
    )?;
    insta::assert_snapshot!("differential_grouped_aggregate_join", grouped_join);

    Ok(())
}
