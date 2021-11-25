use lalrpop_util::lalrpop_mod;
use std::error::Error;
use std::fmt;

pub mod ast;
pub mod c_interface;

pub use ast::QueryParams;

lalrpop_mod!(
    #[allow(clippy::all)]
    pub query
);

pub struct IdentifierParser {
    parser: query::IdentifierParser,
}

impl Default for IdentifierParser {
    fn default() -> Self {
        Self {
            parser: query::IdentifierParser::new(),
        }
    }
}

impl IdentifierParser {
    pub fn sql_primitive(
        &self,
        text: &str,
        param_offset: usize,
    ) -> Result<(String, QueryParams), ParseError> {
        let id = self.parser.parse(text)?;
        Ok(id.primitive_getter(param_offset))
    }

    pub fn sql_json(
        &self,
        text: &str,
        param_offset: usize,
    ) -> Result<(String, QueryParams), ParseError> {
        let id = self.parser.parse(text)?;
        Ok(id.json_getter(param_offset))
    }
}

pub struct ExpressionParser {
    parser: query::ExpressionParser,
}

impl Default for ExpressionParser {
    fn default() -> Self {
        Self {
            parser: query::ExpressionParser::new(),
        }
    }
}

impl ExpressionParser {
    pub fn to_sql(
        &self,
        text: &str,
        param_offset: usize,
    ) -> Result<(String, QueryParams), ParseError> {
        if text.is_empty() {
            Ok(("1 = 1".into(), QueryParams::new()))
        } else {
            let tree = self.parser.parse(&text.to_owned())?;
            Ok(tree.to_sql_query(param_offset))
        }
    }
}

#[derive(Debug)]
pub struct ParseError {
    location: usize,
    expected: Vec<String>,
}

impl Error for ParseError {}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "parse error")
    }
}

impl<T, E> From<lalrpop_util::ParseError<usize, T, E>> for ParseError {
    fn from(err: lalrpop_util::ParseError<usize, T, E>) -> Self {
        match err {
            lalrpop_util::ParseError::InvalidToken { location } => Self {
                location,
                expected: Vec::new(),
            },
            lalrpop_util::ParseError::UnrecognizedEOF { location, expected } => Self {
                location,
                expected: expected.to_vec(),
            },
            lalrpop_util::ParseError::UnrecognizedToken { token, expected } => Self {
                location: token.0,
                expected: expected.to_vec(),
            },
            lalrpop_util::ParseError::ExtraToken { token } => Self {
                location: token.0,
                expected: Vec::new(),
            },
            _ => Self {
                location: 0,
                expected: Vec::new(),
            },
        }
    }
}

#[cfg(test)]
mod test {
    use super::query;
    use crate::ast::{Expression, Operator, Scalar, Value};
    use serde_json::json;

    #[test]
    fn parse_expression() {
        let p = query::ExpressionParser::new();
        assert_eq!(
            *p.parse(r#"not "fts""#).unwrap(),
            Expression::Not(Box::new(Expression::FullTextSearch("fts".into())))
        );

        assert_eq!(
            *p.parse(r#"not "fts1" and "fts2""#).unwrap(),
            Expression::And(
                Box::new(Expression::Not(Box::new(Expression::FullTextSearch(
                    "fts1".into()
                )))),
                Box::new(Expression::FullTextSearch("fts2".into()))
            )
        );
        assert_eq!(
            *p.parse(r#""fts1" or not "fts2" and "fts3""#).unwrap(),
            Expression::Or(
                Box::new(Expression::FullTextSearch("fts1".into())),
                Box::new(Expression::And(
                    Box::new(Expression::Not(Box::new(Expression::FullTextSearch(
                        "fts2".into()
                    )))),
                    Box::new(Expression::FullTextSearch("fts3".into()))
                ))
            )
        );
        assert_eq!(
            *p.parse(r#"("a" or "b") and "c""#).unwrap(),
            Expression::And(
                Box::new(Expression::Or(
                    Box::new(Expression::FullTextSearch("a".into())),
                    Box::new(Expression::FullTextSearch("b".into()))
                )),
                Box::new(Expression::FullTextSearch("c".into()))
            )
        );
    }

    #[test]
    fn parse_term() {
        let p = query::TermParser::new();
        assert_eq!(
            *p.parse(r#""asdf""#).unwrap(),
            Expression::FullTextSearch("asdf".into())
        );
        assert_eq!(
            *p.parse(r#"ident = "value""#).unwrap(),
            Expression::Compare("ident".into(), Operator::Eq, Value::from("value"))
        );
    }

    #[test]
    fn parse_int() {
        let p = query::ScalarParser::new();
        assert_eq!(p.parse("0").unwrap(), Scalar::from(0));
        assert_eq!(p.parse("5").unwrap(), Scalar::from(5));
        assert_eq!(p.parse("12340").unwrap(), Scalar::from(12340));
        assert!(p.parse("01").is_err());
    }

    #[test]
    fn parse_float() {
        let p = query::ScalarParser::new();
        assert_eq!(p.parse("0.1").unwrap(), Scalar::from(0.1));
        assert_eq!(p.parse("5.0").unwrap(), Scalar::from(5.0));
        assert_eq!(p.parse("12340.321").unwrap(), Scalar::from(12340.321));
        assert!(p.parse("1.").is_err());
        assert!(p.parse("00.1").is_err());
    }

    #[test]
    fn parse_string() {
        let p = query::ScalarParser::new();
        assert_eq!(p.parse(r#""asd""#).unwrap(), Scalar::from("asd"));
        assert_eq!(p.parse(r#""""#).unwrap(), Scalar::from(""));
        assert_eq!(p.parse(r#""a\"b""#).unwrap(), Scalar::from("a\"b"));
        assert_eq!(p.parse(r#""a\\b""#).unwrap(), Scalar::from("a\\b"));
        assert_eq!(p.parse(r#""a\t\n\rb""#).unwrap(), Scalar::from("a\t\n\rb"));
        assert!(p.parse(r#"" unescaped " quote ""#).is_err());
        assert!(p.parse(r#""\ ""#).is_err());
        assert!(p.parse(r#""\x""#).is_err());
    }

    #[test]
    fn parse_list() {
        let p = query::ListParser::new();
        assert_eq!(p.parse("()").unwrap(), Vec::new());
        assert_eq!(p.parse("(1)").unwrap(), vec![Scalar::from(1)]);
        assert_eq!(
            p.parse("(1, 2.2, \"three\")").unwrap(),
            vec![Scalar::from(1), Scalar::from(2.2), Scalar::from("three")]
        );
        assert!(p.parse("(1,)").is_err());
    }

    #[test]
    fn parse_identifier() {
        let p = query::IdentifierParser::new();
        assert_eq!(p.parse("abc_def-ghi.123").unwrap(), "abc_def-ghi.123");
        assert!(p.parse("0asd").is_err());
        assert!(p.parse(".asd").is_err());
        assert!(p.parse("-asd").is_err());
        assert!(p.parse("").is_err());
    }

    #[test]
    fn to_sql() {
        let (query, params) =
            Expression::Compare("id".into(), Operator::Eq, Value::from(123)).to_sql_query(5);
        let expected_query = format!(
            "doc -> ($5::jsonb #>> '{{}}') {} $6",
            Operator::Eq.sql_symbol()
        );
        assert_eq!(query, expected_query);
        assert_eq!(
            params,
            vec![serde_json::Value::from("id"), serde_json::Value::from(123)]
        );

        let (query, params) = Expression::FullTextSearch("asdf".into()).to_sql_query(1);
        assert_eq!(query, "search @@ websearch_to_tsquery($1::jsonb #>> '{}')");
        assert_eq!(params[0], "asdf");

        let (query, params) = Expression::And(
            Box::new(Expression::FullTextSearch("a".into())),
            Box::new(Expression::FullTextSearch("b".into())),
        )
        .to_sql_query(11);
        let expected_query = format!(
            "({} AND {})",
            Expression::FullTextSearch("a".into()).to_sql_query(11).0,
            Expression::FullTextSearch("b".into()).to_sql_query(12).0
        );
        assert_eq!(query, expected_query);
        assert_eq!(params, vec!["a", "b"]);
    }

    #[test]
    fn primitive_sql_value() {
        let (expr, params) = Value::from(123).to_sql_primitive_param(1);
        assert_eq!(expr, "$1::jsonb #>> '{}'");
        assert_eq!(params, vec![123]);

        let (expr, params) = Value::from(vec![Scalar::from(1), Scalar::from(2), Scalar::from(3)])
            .to_sql_primitive_param(32);
        assert_eq!(expr, "(select jsonb_array_elements($32::jsonb) #>> '{}')");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], json!(vec![1, 2, 3]));
    }

    #[test]
    fn json_sql_value() {
        let (expr, params) = Value::from(123).to_sql_json_param(1);
        assert_eq!(expr, "$1");
        assert_eq!(params, vec![123]);

        let (expr, params) = Value::from(vec![Scalar::from(1), Scalar::from(2), Scalar::from(3)])
            .to_sql_json_param(32);
        assert_eq!(expr, "$32::jsonb");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], json!(vec![1, 2, 3]));
    }
}
