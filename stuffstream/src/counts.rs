use bb8_postgres::tokio_postgres::types::ToSql;
use futures::lock::Mutex;
use futures::stream;
use futures::stream::StreamExt as _;
use futures::stream::TryStreamExt as _;
use serde_derive::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use time::OffsetDateTime;
use warp::http;

use logstuff::serde::de::rfc3339;
use logstuff_query::{ExpressionParser, IdentifierParser};

use crate::app::DBPool;
use crate::app::Error;
use crate::app::MalformedQuery;
use crate::interval::CountsInterval;

// const DEFAULT_SPLIT_BUCKETS: u16 = 5;

pub(crate) async fn handler(
    expr_parser: Arc<Mutex<ExpressionParser>>,
    id_parser: Arc<Mutex<IdentifierParser>>,
    table_name: String,
    params: Request,
    db: DBPool,
) -> Result<impl warp::Reply, warp::Rejection> {
    let response = Response::new(expr_parser, id_parser, &table_name, db.clone());
    Ok(http::Response::builder()
        .status(http::StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(warp::hyper::Body::wrap_stream(
            response.streams(params).await,
        ))
        .unwrap())
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Request {
    #[serde(deserialize_with = "rfc3339")]
    start: OffsetDateTime,
    #[serde(deserialize_with = "rfc3339")]
    end: OffsetDateTime,
    query: Option<String>,
    split_by: Option<String>,
    max_buckets: Option<i64>,
    value: Option<String>,
    aggregate: Option<String>,
    missing_value_is_zero: Option<bool>,
}

type Param = (dyn ToSql + Sync);

pub struct Response {
    expr_parser: Arc<Mutex<ExpressionParser>>,
    id_parser: Arc<Mutex<IdentifierParser>>,
    table: String,
    db: DBPool,
}

fn split_counts_query(
    table: &str,
    split_by: &Option<String>,
    expr: &str,
    start_id: usize,
    end_id: usize,
    interval: &CountsInterval,
    max_buckets_id: usize,
    outer_value_getter: &str,
    inner_value_getter: &str,
) -> String {
    let (getter, split_subquery) = if let Some(split_by) = split_by {
        let getter = format!("coalesce({}, '(null)') as id", split_by);
        let query = format!(
            r#"
                select {}, {}
                from {}
                where {}
                and tstamp between ${} and ${}
                group by 1
                order by subvalue desc
                limit ${}
            "#,
            getter, inner_value_getter, table, expr, start_id, end_id, max_buckets_id
        );
        (getter, query)
    } else {
        let getter = "'value' as id".to_string();
        let query = format!("select {} limit ${}", getter, max_buckets_id);
        (getter, query)
    };
    format!(
        r#"
            select jsonb_object_agg(tstamp, points) as doc from (
                select tstamp, jsonb_object_agg(id, value) as points from (
                    select date_trunc('{}', gen_time) as tstamp, series.id as id, {}
                    from (select gen_time, id from 
                            generate_series(${}, ${}, '{}'::interval) gen_time,
                            ({}) split
                        ) series
                    left join (select date_trunc('{}', tstamp) as log_time, {}, {}
                            from {}
                            where {}
                            and tstamp between ${} and ${}
                            group by log_time, 2
                        ) l
                    on log_time between gen_time - '{}'::interval and gen_time
                    and series.id = l.id
                    group by tstamp, series.id
                    order by tstamp, series.id
                ) p
                group by tstamp
            ) c
        "#,
        &interval.truncate,
        outer_value_getter,
        start_id,
        end_id,
        &interval.interval,
        split_subquery,
        &interval.truncate,
        getter,
        inner_value_getter,
        table,
        expr,
        start_id,
        end_id,
        &interval.interval
    )
}

impl Response {
    pub fn new(
        expr_parser: Arc<Mutex<ExpressionParser>>,
        id_parser: Arc<Mutex<IdentifierParser>>,
        table: &str,
        db: DBPool,
    ) -> Self {
        Self {
            expr_parser,
            id_parser,
            table: table.to_owned(),
            db,
        }
    }

    async fn parse_query(
        &self,
        query: &Option<String>,
        param_offset: usize,
    ) -> Result<(String, Vec<Value>), MalformedQuery> {
        let p = self.expr_parser.lock().await;
        let (query, query_params) = if let Some(query) = query {
            p.to_sql(query, param_offset).map_err(|_| MalformedQuery)?
        } else {
            ("1 = 1".into(), Vec::new())
        };
        drop(p);
        Ok((query, query_params))
    }

    async fn parse_identifier(
        &self,
        id: &str,
        param_offset: usize,
    ) -> Result<(String, Vec<Value>), MalformedQuery> {
        let p = self.id_parser.lock().await;
        let (expr, params) = p.sql_string(id, param_offset).map_err(|_| MalformedQuery)?;
        drop(p);
        Ok((expr, params))
    }

    async fn value_getters(
        &self,
        params: Request,
        param_offset: usize,
    ) -> Result<(String, String, Vec<Value>), MalformedQuery> {
        if let Some(value) = params.value {
            if params.aggregate.is_none() {
                return Err(MalformedQuery {}); // TODO query is not malformed, parameters don't make sense
            }
            let agg = params.aggregate.unwrap();

            let (expr, query_params) = self.parse_identifier(&value, param_offset).await?;

            let coalesce = params.missing_value_is_zero.unwrap_or(false);
            let outer = if coalesce {
                format!("{}(coalesce(subvalue, 0)) as value", agg)
            } else {
                format!("{}(subvalue) as value", agg)
            };
            let inner = format!("{}({}) as subvalue", agg, expr);
            Ok((outer, inner, query_params))
        } else {
            Ok((
                "sum(coalesce(subvalue, 0)) as value".to_string(),
                "count(*) as subvalue".to_string(),
                Vec::new(),
            ))
        }
    }

    pub async fn streams(
        self,
        params: Request,
    ) -> impl futures::Stream<Item = Result<impl Into<warp::hyper::body::Bytes>, Error>> {
        let params_clone = params.clone();

        let (expr, mut query_params) = self.parse_query(&params.query, 1).await.unwrap();
        let getter = if let Some(split_by) = params.split_by {
            let (getter, getter_params) = self
                .parse_identifier(&split_by, query_params.len() + 1)
                .await
                .unwrap();
            query_params.extend(getter_params);
            Some(getter)
        } else {
            None
        };

        let (outer_value_getter, inner_value_getter, value_params) = self
            .value_getters(params_clone, query_params.len() + 1)
            .await
            .unwrap();
        query_params.extend(value_params);
        let param_offset = query_params.len() + 1;

        let db = self.db.get().await.unwrap();
        let interval = CountsInterval::from(params.end - params.start);

        let query = split_counts_query(
            &self.table,
            &getter,
            &expr,
            param_offset,
            param_offset + 1,
            &interval,
            param_offset + 2,
            &outer_value_getter,
            &inner_value_getter,
        );
        let counts = db
            .query_raw(
                query.as_str(),
                query_params
                    .iter()
                    .map(|e| e as &Param)
                    .chain(std::iter::once::<&Param>(&params.start.to_owned()))
                    .chain(std::iter::once::<&Param>(&params.end.to_owned()))
                    .chain(std::iter::once::<&Param>(&params.max_buckets.to_owned()))
                    .collect::<Vec<&Param>>(),
            )
            .await;

        stream::once(async move {
            Ok(format!(
                r#"{{"metadata":{{"counts_interval_sec": {}}},"counts":"#,
                interval.seconds
            ))
        })
        .chain(
            counts
                .unwrap()
                .map_ok(|row| {
                    let value: Option<Value> = row.get("doc");
                    value.unwrap_or(Value::Null).to_string()
                })
                .map_err(Error::from),
        )
        .chain(stream::once(async { Ok(r#"}"#.to_string()) }))
    }
}
