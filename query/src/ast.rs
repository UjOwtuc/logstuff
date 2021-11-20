use serde_json::json;

#[derive(Debug, PartialEq)]
pub enum Scalar {
    Int(i64),
    Float(f64),
    Text(String),
}

impl From<i64> for Scalar {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl From<f64> for Scalar {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

impl From<&str> for Scalar {
    fn from(value: &str) -> Self {
        Self::Text(value.into())
    }
}

impl From<String> for Scalar {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl Scalar {
    fn as_json(&self) -> serde_json::Value {
        match self {
            Scalar::Int(i) => serde_json::Value::from(*i),
            Scalar::Float(f) => serde_json::Value::from(*f),
            Scalar::Text(s) => serde_json::Value::from(s.to_owned()),
        }
    }
}

type List = Vec<Scalar>;

#[derive(Debug, PartialEq)]
pub enum Value {
    Scalar(Scalar),
    List(List),
}

impl Value {
    pub fn to_sql_primitive_param(&self, param_offset: usize) -> (String, QueryParams) {
        match self {
            Value::Scalar(value) => (
                format!("${}::jsonb #>> '{{}}'", param_offset),
                vec![value.as_json()],
            ),
            Value::List(list) => (
                format!(
                    "(select jsonb_array_elements(${}::jsonb) #>> '{{}}')",
                    param_offset
                ),
                vec![json!(list
                    .iter()
                    .map(|e| e.as_json())
                    .collect::<Vec<serde_json::Value>>())],
            ),
        }
    }

    pub fn to_sql_json_param(&self, param_offset: usize) -> (String, QueryParams) {
        match self {
            Value::Scalar(value) => (format!("${}", param_offset), vec![value.as_json()]),
            Value::List(list) => (
                format!("${}::jsonb", param_offset),
                vec![json!(list
                    .iter()
                    .map(|e| e.as_json())
                    .collect::<Vec<serde_json::Value>>())],
            ),
        }
    }
}

impl<T> From<T> for Value
where
    T: Into<Scalar>,
{
    fn from(scalar: T) -> Self {
        Self::Scalar(scalar.into())
    }
}

impl From<List> for Value {
    fn from(list: List) -> Self {
        Self::List(list)
    }
}

#[derive(Debug, PartialEq)]
pub enum Operator {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
    Like,
    In,
}

impl Operator {
    pub fn sql_symbol(&self) -> &'static str {
        match self {
            Operator::Eq => "@>",
            Operator::Gt => ">",
            Operator::Ge => ">=",
            Operator::Lt => "<",
            Operator::Le => "<=",
            Operator::Like => "LIKE",
            Operator::In => "IN",
        }
    }

    pub fn primitive_operands(&self) -> bool {
        matches!(self, Operator::Like | Operator::In)
    }
}

#[derive(Debug, PartialEq)]
pub struct Comparison {
    pub(crate) identifier: String,
    pub(crate) operator: Operator,
    pub(crate) value: Value,
}

#[derive(Debug, PartialEq)]
pub enum Expression {
    Compare(String, Operator, Value),
    And(Box<Expression>, Box<Expression>),
    Or(Box<Expression>, Box<Expression>),
    Not(Box<Expression>),
    FullTextSearch(String),
}

pub type QueryParams = Vec<serde_json::Value>;

impl Expression {
    pub fn to_sql_query(&self, param_offset: usize) -> (String, QueryParams) {
        match self {
            Expression::And(lhs, rhs) => {
                let (left_expr, left_params) = lhs.to_sql_query(param_offset);
                let (right_expr, right_params) = rhs.to_sql_query(param_offset + left_params.len());
                let mut params = left_params;
                params.extend(right_params);
                (format!("({} AND {})", left_expr, right_expr), params)
            }
            Expression::Or(lhs, rhs) => {
                let (left_expr, left_params) = lhs.to_sql_query(param_offset);
                let (right_expr, right_params) = rhs.to_sql_query(param_offset + left_params.len());
                let mut params = left_params;
                params.extend(right_params);
                (format!("({} OR {})", left_expr, right_expr), params)
            }
            Expression::Not(expr) => {
                let (expr, params) = expr.to_sql_query(param_offset);
                (format!("(NOT {})", expr), params)
            }
            Expression::FullTextSearch(s) => (
                format!(
                    "search @@ websearch_to_tsquery(${}::jsonb #>> '{{}}')",
                    param_offset
                ),
                vec![serde_json::Value::from(s.to_owned())],
            ),
            Expression::Compare(id, op, value) => {
                let id_expr = if op.primitive_operands() {
                    format!("doc ->> (${}::jsonb #>> '{{}}')", param_offset)
                } else {
                    format!("doc -> (${}::jsonb #>> '{{}}')", param_offset)
                };
                let (value_expr, value_params) = if op.primitive_operands() {
                    value.to_sql_primitive_param(param_offset + 1)
                } else {
                    value.to_sql_json_param(param_offset + 1)
                };
                let mut params: QueryParams = vec![serde_json::Value::from(id.to_owned())];
                params.extend(value_params);
                (
                    format!("{} {} {}", id_expr, op.sql_symbol(), value_expr),
                    params,
                )
            }
        }
    }
}
