use pest::iterators::Pair;
use pest::prec_climber::{Assoc, Operator, PrecClimber};
use pest::Parser;
use postgres::types::ToSql;
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
}

pub type QueryParams = Vec<Box<dyn ToSql + Sync>>;

pub fn parse_query(query: &str) -> Result<(String, QueryParams), Box<dyn Error>> {
    // log::debug!("parse {:?}:", query);
    let mut pairs = QueryParser::parse(Rule::bool_expr, query)?;
    let climber = PrecClimber::new(vec![
        Operator::new(Rule::and_op, Assoc::Left) | Operator::new(Rule::or_op, Assoc::Left),
    ]);
    let ast = consume(pairs.next().unwrap(), &climber);
    walk_tree(ast, 1)
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
                format!("doc ->>${}", param_offset)
            };
            (expr, vec![Box::new(id)])
        }
        Value::Scalar(value) => {
            let expr = if numeric {
                format!("try_to_int(${})", param_offset)
            } else {
                format!("${}", param_offset)
            };
            (expr, vec![Box::new(value)])
        }
        Value::List(list) => {
            let mut param_num = param_offset;
            let mut expr = Vec::new();
            let mut params: QueryParams = Vec::new();
            list.iter().for_each(|e| {
                expr.push(format!("${}", param_num));
                param_num += 1;
                params.push(Box::new(e.to_owned()));
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
    }
}
