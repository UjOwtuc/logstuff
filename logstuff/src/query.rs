use pest::iterators::Pair;
use pest::prec_climber::{Assoc, Operator, PrecClimber};
use pest::Parser;
use std::error::Error;

#[derive(Parser)]
#[grammar = "query.pest"]
pub struct QueryParser;

#[derive(Debug)]
enum Value {
    Identifier(String),
    Scalar(String),
    List(Vec<String>),
}

impl<'i> From<Pair<'i, Rule>> for Value {
    fn from(pair: Pair<'i, Rule>) -> Self {
        match pair.as_rule() {
            Rule::field => Value::Identifier(pair.as_str().to_string()),
            Rule::scalar_value | Rule::list_value => {
                let inner = pair.into_inner().next().unwrap();
                match inner.as_rule() {
                    Rule::string_literal => {
                        Value::Scalar(inner.into_inner().next().unwrap().as_str().to_string())
                    }
                    Rule::num_literal => Value::Scalar(inner.as_str().to_string()),
                    Rule::string_list => Value::List(
                        inner
                            .into_inner()
                            .map(|e| e.into_inner().next().unwrap().as_str().to_string())
                            .collect(),
                    ),
                    Rule::num_list => {
                        Value::List(inner.into_inner().map(|e| e.as_str().to_string()).collect())
                    }
                    _ => {
                        println!("converting {:?} to Value", inner);
                        unreachable!()
                    }
                }
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Debug)]
enum Expression {
    Compare(Value, Rule, Value),
    And(Box<Expression>, Box<Expression>),
    Or(Box<Expression>, Box<Expression>),
    Fts(String),
}

pub type QueryParams = Vec<serde_json::Value>;

pub fn parse_query(query: &str) -> Result<(String, QueryParams), Box<dyn Error>> {
    if !query.is_empty() {
        let mut pairs = QueryParser::parse(Rule::bool_expr, query)?;
        let climber = PrecClimber::new(vec![
            Operator::new(Rule::or_op, Assoc::Left),
            Operator::new(Rule::and_op, Assoc::Left),
        ]);
        let ast = consume(pairs.next().unwrap(), &climber);
        walk_tree(ast, 1)
    } else {
        Ok(("1 = 1".to_string(), QueryParams::new()))
    }
}

fn consume(pair: Pair<Rule>, climber: &PrecClimber<Rule>) -> Expression {
    let atom = |pair| consume(pair, climber);
    let infix = |lhs, op: Pair<Rule>, rhs| match op.as_rule() {
        Rule::and_op => Expression::And(Box::new(lhs), Box::new(rhs)),
        Rule::or_op => Expression::Or(Box::new(lhs), Box::new(rhs)),
        _ => unreachable!(),
    };

    match pair.as_rule() {
        Rule::expr => {
            let pairs = pair.into_inner();
            climber.climb(pairs, atom, infix)
        }
        Rule::paren_bool => pair.into_inner().next().map(atom).unwrap(),
        Rule::comp_expr => {
            let mut iter = pair.into_inner();
            let (lhs, op, rhs) = (
                Value::from(iter.next().unwrap()),
                iter.next().unwrap().into_inner().next().unwrap().as_rule(),
                Value::from(iter.next().unwrap()),
            );
            Expression::Compare(lhs, op, rhs)
        }
        Rule::fts => Expression::Fts(
            pair.into_inner()
                .next()
                .unwrap()
                .into_inner()
                .next()
                .unwrap()
                .as_str()
                .to_string(),
        ),
        _ => {
            println!("other rule: {:?}", pair);
            unreachable!()
        }
    }
}

fn format_operand(operand: Value, param_offset: usize, primitive: bool) -> (String, QueryParams) {
    let (expr, param) = match operand {
        Value::Identifier(id) => {
            let expr = if primitive {
                format!("doc ->> (${}::jsonb #>> '{{}}')", param_offset)
            } else {
                format!("doc -> (${}::jsonb #>> '{{}}')", param_offset)
            };
            (expr, id.into())
        }
        Value::Scalar(value) => {
            let expr = if primitive {
                format!("${}::jsonb #>> '{{}}'", param_offset)
            } else {
                format!("${}", param_offset)
            };
            (expr, value.into())
        }
        Value::List(list) => {
            let expr = if primitive {
                format!(
                    "(select jsonb_array_elements(${}::jsonb) #>> '{{}}')",
                    param_offset
                )
            } else {
                format!("${}::jsonb", param_offset)
            };
            (expr, list.into())
        }
    };

    (expr, vec![param])
}

fn walk_tree(
    expr: Expression,
    param_offset: usize,
) -> Result<(String, QueryParams), Box<dyn Error>> {
    match expr {
        Expression::And(lhs, rhs) => {
            let (left_expr, left_params) = walk_tree(*lhs, param_offset)?;
            let (right_expr, right_params) = walk_tree(*rhs, param_offset + left_params.len())?;
            let mut params = left_params;
            params.extend(right_params);
            Ok((format!("({} AND {})", left_expr, right_expr), params))
        }
        Expression::Or(lhs, rhs) => {
            let (left_expr, left_params) = walk_tree(*lhs, param_offset)?;
            let (right_expr, right_params) = walk_tree(*rhs, param_offset + left_params.len())?;
            let mut params = left_params;
            params.extend(right_params);
            Ok((format!("({} OR {})", left_expr, right_expr), params))
        }
        Expression::Compare(lhs, op, rhs) => {
            let mut negate = false;
            let mut primitive_operands = false;
            let op = match op {
                Rule::eq => "@>",
                Rule::neq => {
                    negate = true;
                    "@>"
                }
                Rule::gte => ">=",
                Rule::gt => ">",
                Rule::lte => "<=",
                Rule::lt => "<",
                Rule::op_in => {
                    primitive_operands = true;
                    "IN"
                }
                Rule::op_not_in => {
                    primitive_operands = true;
                    negate = true;
                    "IN"
                }
                Rule::like => {
                    primitive_operands = true;
                    "LIKE"
                }
                Rule::not_like => {
                    primitive_operands = true;
                    negate = true;
                    "LIKE"
                }
                _ => unreachable!(),
            };
            let (left_expr, left_params) = format_operand(lhs, param_offset, primitive_operands);
            let (right_expr, right_params) =
                format_operand(rhs, param_offset + left_params.len(), primitive_operands);
            let mut params = left_params;
            params.extend(right_params);
            let expr = if negate {
                format!("NOT {} {} {}", left_expr, op, right_expr)
            } else {
                format!("{} {} {}", left_expr, op, right_expr)
            };
            Ok((expr, params))
        }
        Expression::Fts(value) => Ok((
            format!(
                "search @@ websearch_to_tsquery(${}::jsonb #>> '{{}}')",
                param_offset
            ),
            vec![value.into()],
        )),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn whole_queries() {
        for (query, expression, param_count) in &[
            ("id = \"value\"", "doc -> ($1::jsonb #>> '{}') @> $2", 2),
            (
                "id != \"value\"",
                "NOT doc -> ($1::jsonb #>> '{}') @> $2",
                2,
            ),
            ("id < 123", "doc -> ($1::jsonb #>> '{}') < $2", 2),
            ("id <= 1.23", "doc -> ($1::jsonb #>> '{}') <= $2", 2),
            ("id > 123", "doc -> ($1::jsonb #>> '{}') > $2", 2),
            ("id >= 123", "doc -> ($1::jsonb #>> '{}') >= $2", 2),
            (
                "id like \"value\"",
                "doc ->> ($1::jsonb #>> '{}') LIKE $2::jsonb #>> '{}'",
                2,
            ),
            (
                "id not like \"value\"",
                "NOT doc ->> ($1::jsonb #>> '{}') LIKE $2::jsonb #>> '{}'",
                2,
            ),
            (
                "id in (\"a\", \"b\")",
                "doc ->> ($1::jsonb #>> '{}') IN (select jsonb_array_elements($2::jsonb) #>> '{}')",
                2,
            ),
            (
                "id in (1, 2, 3)",
                "doc ->> ($1::jsonb #>> '{}') IN (select jsonb_array_elements($2::jsonb) #>> '{}')",
                2,
            ),
            (
                "id not in (1)",
                "NOT doc ->> ($1::jsonb #>> '{}') IN (select jsonb_array_elements($2::jsonb) #>> '{}')",
                2,
            ),
            ("id.with.dots = 1", "doc -> ($1::jsonb #>> '{}') @> $2", 2),
            (
                "id = 1 and id = 1",
                "(doc -> ($1::jsonb #>> '{}') @> $2 AND doc -> ($3::jsonb #>> '{}') @> $4)",
                4,
            ),
            (
                "id = 1 or id = 2",
                "(doc -> ($1::jsonb #>> '{}') @> $2 OR doc -> ($3::jsonb #>> '{}') @> $4)",
                4,
            ),
            (
                "(id = 1 or id = 2) and (id2 = 1 or id2 = 2)",
                "((doc -> ($1::jsonb #>> '{}') @> $2 OR doc -> ($3::jsonb #>> '{}') @> $4) AND (doc -> ($5::jsonb #>> '{}') @> $6 OR doc -> ($7::jsonb #>> '{}') @> $8))",
                8,
            ),
            (
                "id = 1 or id = 2 and id2 = 1 or id2 = 2",
                "((doc -> ($1::jsonb #>> '{}') @> $2 OR (doc -> ($3::jsonb #>> '{}') @> $4 AND doc -> ($5::jsonb #>> '{}') @> $6)) OR doc -> ($7::jsonb #>> '{}') @> $8)",
                8,
            ),
        ] {
            let (expr, params) = parse_query(query).unwrap();
            assert_eq!(expr, *expression);
            assert_eq!(params.len(), *param_count);
        }
    }

    #[test]
    fn operand_formatting() {
        let (expr, params) = format_operand(Value::Identifier("id".into()), 3, false);
        assert_eq!(expr, "doc -> ($3::jsonb #>> '{}')");
        assert_eq!(params.len(), 1);

        let (expr, params) = format_operand(Value::Identifier("id".into()), 2, true);
        assert_eq!(expr, "doc ->> ($2::jsonb #>> '{}')");
        assert_eq!(params.len(), 1);

        let (expr, params) = format_operand(Value::Scalar("scalar".into()), 1, false);
        assert_eq!(expr, "$1");
        assert_eq!(params.len(), 1);

        let (expr, params) = format_operand(Value::Scalar("scalar".into()), 0, true);
        assert_eq!(expr, "$0::jsonb #>> '{}'");
        assert_eq!(params.len(), 1);

        let (expr, params) = format_operand(Value::List(vec!["a".into(), "b".into()]), 33, false);
        assert_eq!(expr, "$33::jsonb");
        assert_eq!(params.len(), 1);

        let (expr, params) = format_operand(
            Value::List(vec!["a".into(), "b".into(), "c".into()]),
            40,
            true,
        );
        assert_eq!(expr, "(select jsonb_array_elements($40::jsonb) #>> '{}')");
        assert_eq!(params.len(), 1);
    }
}
