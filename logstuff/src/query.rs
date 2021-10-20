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
            Rule::value => {
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

pub type QueryParams = Vec<String>;

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

fn format_operand(operand: Value, param_offset: usize, numeric: bool) -> (String, QueryParams) {
    match operand {
        Value::Identifier(id) => {
            let expr = if numeric {
                format!("try_to_int(doc ->> ${})", param_offset)
            } else {
                format!("doc ->> ${}", param_offset)
            };
            (expr, vec![id])
        }
        Value::Scalar(value) => {
            let expr = if numeric {
                format!("try_to_int(${})", param_offset)
            } else {
                format!("${}", param_offset)
            };
            (expr, vec![value])
        }
        Value::List(list) => {
            let mut param_num = param_offset;
            let mut expr = Vec::new();
            let mut params: QueryParams = Vec::new();
            list.iter().for_each(|e| {
                expr.push(format!("${}", param_num));
                param_num += 1;
                params.push(e.to_owned());
            });
            (format!("({})", expr.join(", ")), params)
        }
    }
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
            let mut numeric_expr = false;
            let op = match op {
                Rule::eq => "=",
                Rule::neq => "!=",
                Rule::op_in => "IN",
                Rule::op_not_in => {
                    negate = true;
                    "IN"
                }
                Rule::gte => {
                    numeric_expr = true;
                    ">="
                }
                Rule::gt => {
                    numeric_expr = true;
                    ">"
                }
                Rule::lte => {
                    numeric_expr = true;
                    "<="
                }
                Rule::lt => {
                    numeric_expr = true;
                    "<"
                }
                Rule::like => "LIKE",
                Rule::not_like => {
                    negate = true;
                    "LIKE"
                }
                _ => unreachable!(),
            };
            let (left_expr, left_params) = format_operand(lhs, param_offset, numeric_expr);
            let (right_expr, right_params) =
                format_operand(rhs, param_offset + left_params.len(), numeric_expr);
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
            format!("search @@ websearch_to_tsquery(${})", param_offset),
            vec![value],
        )),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn whole_queries() {
        for (query, expression, param_count) in &[
            ("id = \"value\"", "doc ->> $1 = $2", 2),
            ("id != \"value\"", "doc ->> $1 != $2", 2),
            ("id < 123", "try_to_int(doc ->> $1) < try_to_int($2)", 2),
            ("id <= 1.23", "try_to_int(doc ->> $1) <= try_to_int($2)", 2),
            ("id > 123", "try_to_int(doc ->> $1) > try_to_int($2)", 2),
            ("id >= 123", "try_to_int(doc ->> $1) >= try_to_int($2)", 2),
            ("id like \"value\"", "doc ->> $1 LIKE $2", 2),
            ("id not like \"value\"", "NOT doc ->> $1 LIKE $2", 2),
            ("id in (\"a\", \"b\")", "doc ->> $1 IN ($2, $3)", 3),
            ("id in (1, 2, 3)", "doc ->> $1 IN ($2, $3, $4)", 4),
            ("id not in (1)", "NOT doc ->> $1 IN ($2)", 2),
            ("id.with.dots = 1", "doc ->> $1 = $2", 2),
            (
                "id = 1 and id = 1",
                "(doc ->> $1 = $2 AND doc ->> $3 = $4)",
                4,
            ),
            (
                "id = 1 or id = 2",
                "(doc ->> $1 = $2 OR doc ->> $3 = $4)",
                4,
            ),
            (
                "(id = 1 or id = 2) and (id2 = 1 or id2 = 2)",
                "((doc ->> $1 = $2 OR doc ->> $3 = $4) AND (doc ->> $5 = $6 OR doc ->> $7 = $8))",
                8,
            ),
            (
                "id = 1 or id = 2 and id2 = 1 or id2 = 2",
                "((doc ->> $1 = $2 OR (doc ->> $3 = $4 AND doc ->> $5 = $6)) OR doc ->> $7 = $8)",
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
        assert_eq!(expr, "doc ->> $3");
        assert_eq!(params.len(), 1);

        let (expr, params) = format_operand(Value::Identifier("id".into()), 2, true);
        assert_eq!(expr, "try_to_int(doc ->> $2)");
        assert_eq!(params.len(), 1);

        let (expr, params) = format_operand(Value::Scalar("scalar".into()), 1, false);
        assert_eq!(expr, "$1");
        assert_eq!(params.len(), 1);

        let (expr, params) = format_operand(Value::Scalar("scalar".into()), 0, true);
        assert_eq!(expr, "try_to_int($0)");
        assert_eq!(params.len(), 1);

        let (expr, params) = format_operand(Value::List(vec!["a".into(), "b".into()]), 33, false);
        assert_eq!(expr, "($33, $34)");
        assert_eq!(params.len(), 2);

        let (expr, params) = format_operand(
            Value::List(vec!["a".into(), "b".into(), "c".into()]),
            40,
            true,
        );
        assert_eq!(expr, "($40, $41, $42)");
        assert_eq!(params.len(), 3);
    }
}
