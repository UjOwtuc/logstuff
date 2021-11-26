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

#[derive(Serialize, Deserialize, Debug)]
pub struct Request {
    #[serde(deserialize_with = "rfc3339")]
    start: OffsetDateTime,
    #[serde(deserialize_with = "rfc3339")]
    end: OffsetDateTime,
    query: Option<String>,
    split_by: Option<String>,
    max_buckets: Option<i64>,
}

impl Request {
    pub fn has_split_by(&self) -> bool {
        if let Some(name) = &self.split_by {
            !name.is_empty()
        } else {
            false
        }
    }
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
    split_by: &str,
    expr: &str,
    start_id: usize,
    end_id: usize,
    interval: &CountsInterval,
    max_buckets_id: usize,
) -> String {
    format!(
        r#"
            select jsonb_object_agg(tstamp, points) as doc from (
                select tstamp, jsonb_object_agg(id, count) as points from (
                    select date_trunc('{}', gen_time) as tstamp, series.id as id, sum(coalesce(subcount, 0)) as count
                    from (select gen_time, id from 
                            generate_series(${}, ${}, '{}'::interval) gen_time,
                            (select distinct {} as id, count(*) as count
                            from {}
                            where {}
                            and tstamp between ${} and ${}
                            group by 1
                            order by count desc
                            limit ${}) split
                        ) series
                    left join (select date_trunc('{}', tstamp) as log_time, {} as id, count(*) as subcount
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
        start_id,
        end_id,
        &interval.interval,
        split_by,
        table,
        expr,
        start_id,
        end_id,
        max_buckets_id,
        &interval.truncate,
        split_by,
        table,
        expr,
        start_id,
        end_id,
        &interval.interval
    )
}

fn counts_query(
    table: &str,
    expr: &str,
    start_id: usize,
    end_id: usize,
    interval: &CountsInterval,
) -> String {
    format!(
        r#"
            select jsonb_object_agg(tstamp, count) as doc from (
                select date_trunc('{}', gen_time) as tstamp, sum(coalesce(subcount, 0)) as count
                from generate_series(${}, ${}, '{}'::interval) gen_time
                left join (select date_trunc('{}', tstamp) as log_time, count(*) as subcount
                    from {}
                    where {}
                    and tstamp between ${} and ${}
                    group by log_time
                ) l
                on log_time between gen_time - '{}'::interval and gen_time
                group by tstamp
                order by tstamp
            ) c
        "#,
        &interval.truncate,
        start_id,
        end_id,
        &interval.interval,
        &interval.truncate,
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

    pub async fn streams(
        self,
        params: Request,
    ) -> impl futures::Stream<Item = Result<impl Into<warp::hyper::body::Bytes>, Error>> {
        let param_offset = match params.has_split_by() {
            true => 2,
            false => 1,
        };

        let (expr, mut query_params) = self.parse_query(&params.query, param_offset).await.unwrap();
        let param_offset = param_offset + query_params.len();

        let db = self.db.get().await.unwrap();
        let interval = CountsInterval::from(params.end - params.start);
        let counts = if params.has_split_by() {
            let (getter, mut getter_params) = self
                .parse_identifier(&params.split_by.unwrap(), 1)
                .await
                .unwrap();
            getter_params.extend(query_params);
            query_params = getter_params;

            let query = split_counts_query(
                &self.table,
                &getter,
                &expr,
                param_offset,
                param_offset + 1,
                &interval,
                param_offset + 2,
            );
            db.query_raw(
                query.as_str(),
                query_params
                    .iter()
                    .map(|e| e as &Param)
                    .chain(std::iter::once::<&Param>(&params.start.to_owned()))
                    .chain(std::iter::once::<&Param>(&params.end.to_owned()))
                    .chain(std::iter::once::<&Param>(&params.max_buckets.to_owned()))
                    .collect::<Vec<&Param>>(),
            )
            .await
        } else {
            let query = counts_query(
                &self.table,
                &expr,
                param_offset,
                param_offset + 1,
                &interval,
            );
            db.query_raw(
                query.as_str(),
                query_params
                    .iter()
                    .map(|e| e as &Param)
                    .chain(std::iter::once::<&Param>(&params.start.to_owned()))
                    .chain(std::iter::once::<&Param>(&params.end.to_owned()))
                    .collect::<Vec<&Param>>(),
            )
            .await
        };

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
