use bb8_postgres::tokio_postgres;
use bb8_postgres::tokio_postgres::types::ToSql;
use futures::lock::Mutex;
use futures::stream;
use futures::{StreamExt, TryStreamExt};
use logstuff::serde::de::rfc3339;
use serde_derive::{Deserialize, Serialize};
use serde_json::Value;
use std::iter::Iterator;
use std::sync::Arc;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use logstuff_query::ExpressionParser;

use crate::app::DBPool;
use crate::app::Error;
use crate::app::MalformedQuery;
use crate::interval::CountsInterval;

type Param = (dyn ToSql + Sync);

#[derive(Serialize, Deserialize, Debug)]
pub struct EventsRequest {
    #[serde(deserialize_with = "rfc3339")]
    start: OffsetDateTime,
    #[serde(deserialize_with = "rfc3339")]
    end: OffsetDateTime,
    query: Option<String>,
    limit_events: Option<i64>,
}

#[derive(Clone)]
pub struct EventsResponse {
    parser: Arc<Mutex<ExpressionParser>>,
    table: String,
    db: DBPool,
}

fn fetch_doc(
    rows: tokio_postgres::RowStream,
) -> impl stream::Stream<Item = Result<String, tokio_postgres::Error>> {
    rows.map_ok(|row| {
        let value: Option<Value> = row.get("doc");
        value.unwrap_or(Value::Null).to_string()
    })
}

fn events_query(
    table: &str,
    expr: &str,
    start_id: usize,
    end_id: usize,
    limit_id: usize,
) -> String {
    format!(
        r#"
            select jsonb_agg(doc) as doc from (
                select jsonb_build_object('timestamp', tstamp, 'id', id, 'source', doc) as doc
                from {}
                where {}
                and tstamp between ${} and ${}
                order by tstamp desc
                limit ${}
            ) e
        "#,
        table, expr, start_id, end_id, limit_id,
    )
}

fn counts_query(
    table: &str,
    expr: &str,
    start_id: usize,
    end_id: usize,
    start: &OffsetDateTime,
    end: &OffsetDateTime,
) -> String {
    let interval = CountsInterval::from(*end - *start);
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

fn fields_query(table: &str, expr: &str, start_id: usize, end_id: usize) -> String {
    format!(
        r#"
            select jsonb_object_agg(key, values) as doc from (
                select key::varchar, jsonb_object_agg(coalesce(value::text, ''), count::integer) as values from (
                    select row_number() over (
                            partition by key
                            order by count desc
                        ) as row_number, count, key, value
                    from (
                        select count(*), key, jsonb_array_elements(
                            case
                                when jsonb_typeof(value) = 'array' then value
                                else jsonb_build_array(value)
                            end) #>> '{{}}' as value
                        from (
                            select doc
                            from {}
                            where {}
                            and tstamp between ${} and ${}
                            order by tstamp desc
                            limit 500
                        ) limited_logs, jsonb_each(doc)
                        group by key, value
                        order by key, count desc
                    ) counted
                ) ranked
                where row_number <= 5
                group by key
            ) f
        "#,
        table, expr, start_id, end_id
    )
}

fn metadata_query(table: &str, start: &OffsetDateTime, end: &OffsetDateTime) -> String {
    let interval = CountsInterval::from(*end - *start);
    format!(
        r#"
            select jsonb_object_agg(key, value) as doc from (
                select 'event_count' as key, count_estimate('select * from {} where tstamp between ''{}'' and ''{}''') as value
                union
                select 'counts_interval_sec' as key, {} as value
            ) m
        "#,
        table,
        start.format(&Rfc3339).unwrap(),
        end.format(&Rfc3339).unwrap(),
        &interval.seconds
    )
}

async fn metadata(
    db: DBPool,
    table: Arc<String>,
    start: &OffsetDateTime,
    end: &OffsetDateTime,
) -> impl stream::Stream<Item = Result<String, Error>> {
    let db = db.get().await.unwrap();
    let empty_params: Vec<&str> = Vec::new();
    fetch_doc(
        db.query_raw(
            metadata_query(table.as_ref(), start, end).as_str(),
            empty_params,
        )
        .await
        .unwrap(),
    )
    .map_err(|err| {
        error!("fetch metadata: {:?}", err);
        Error::from(err)
    })
}

async fn counts(
    db: DBPool,
    table: Arc<String>,
    expr: Arc<String>,
    params: Arc<Vec<Value>>,
    start: &OffsetDateTime,
    end: &OffsetDateTime,
) -> impl stream::Stream<Item = Result<String, Error>> {
    let db = db.get().await.unwrap();
    fetch_doc(
        db.query_raw(
            counts_query(
                table.as_ref(),
                expr.as_ref(),
                params.len() + 1,
                params.len() + 2,
                start,
                end,
            )
            .as_str(),
            params
                .iter()
                .map(|e| e as &Param)
                .chain(std::iter::once::<&Param>(&start.to_owned()))
                .chain(std::iter::once::<&Param>(&end.to_owned()))
                .collect::<Vec<&Param>>(),
        )
        .await
        .unwrap(),
    )
    .map_err(|err| {
        error!("fetch counts: {:?}", err);
        Error::from(err)
    })
}

async fn fields(
    db: DBPool,
    table: Arc<String>,
    expr: Arc<String>,
    params: Arc<Vec<Value>>,
    start: &OffsetDateTime,
    end: &OffsetDateTime,
) -> impl stream::Stream<Item = Result<String, Error>> {
    let db = db.get().await.unwrap();
    fetch_doc(
        db.query_raw(
            fields_query(
                table.as_ref(),
                expr.as_ref(),
                params.len() + 1,
                params.len() + 2,
            )
            .as_str(),
            params
                .iter()
                .map(|e| e as &Param)
                .chain(std::iter::once::<&Param>(&start.to_owned()))
                .chain(std::iter::once::<&Param>(&end.to_owned()))
                .collect::<Vec<&Param>>(),
        )
        .await
        .unwrap(),
    )
    .map_err(|err| {
        error!("fetch fields: {:?}", err);
        Error::from(err)
    })
}

async fn events(
    db: DBPool,
    table: Arc<String>,
    expr: Arc<String>,
    params: Arc<Vec<Value>>,
    start: &OffsetDateTime,
    end: &OffsetDateTime,
    limit: &Option<i64>,
) -> impl stream::Stream<Item = Result<String, Error>> {
    let db = db.get().await.unwrap();
    fetch_doc(
        db.query_raw(
            events_query(
                table.as_ref(),
                expr.as_ref(),
                params.len() + 1,
                params.len() + 2,
                params.len() + 3,
            )
            .as_str(),
            params
                .iter()
                .map(|e| e as &Param)
                .chain(std::iter::once::<&Param>(&start.to_owned()))
                .chain(std::iter::once::<&Param>(&end.to_owned()))
                .chain(std::iter::once::<&Param>(&limit.to_owned()))
                .collect::<Vec<&Param>>(),
        )
        .await
        .unwrap(),
    )
    .map_err(|err| {
        error!("fetch events: {:?}", err);
        Error::from(err)
    })
}

impl EventsResponse {
    pub fn new(parser: Arc<Mutex<ExpressionParser>>, table: &str, db: DBPool) -> Self {
        Self {
            parser,
            table: table.to_owned(),
            db,
        }
    }

    async fn parse_query(
        &self,
        query: &Option<String>,
    ) -> Result<(String, Vec<Value>), MalformedQuery> {
        let p = self.parser.lock().await;
        let (query, query_params) = if let Some(query) = query {
            p.to_sql(query).map_err(|_| MalformedQuery)?
        } else {
            ("1 = 1".into(), Vec::new())
        };
        drop(p);
        Ok((query, query_params))
    }

    pub async fn streams(
        self,
        params: EventsRequest,
    ) -> impl futures::Stream<Item = Result<impl Into<warp::hyper::body::Bytes>, Error>> {
        let (expr, query_params) = self.parse_query(&params.query).await.unwrap();
        let expr = Arc::new(expr);
        let query_params = Arc::new(query_params);
        let table = Arc::new(self.table.to_owned());

        let (e, f, c, m) = futures::join!(
            events(
                self.db.clone(),
                table.clone(),
                expr.clone(),
                query_params.clone(),
                &params.start,
                &params.end,
                &params.limit_events,
            ),
            fields(
                self.db.clone(),
                table.clone(),
                expr.clone(),
                query_params.clone(),
                &params.start,
                &params.end,
            ),
            counts(
                self.db.clone(),
                table.clone(),
                expr.clone(),
                query_params.clone(),
                &params.start,
                &params.end,
            ),
            metadata(self.db, table, &params.start, &params.end),
        );

        stream::once(async { Ok(r#"{"events":"#.to_string()) })
            .chain(e)
            .chain(stream::once(async { Ok(r#", "fields":"#.to_string()) }))
            .chain(f)
            .chain(stream::once(async { Ok(r#", "counts":"#.to_string()) }))
            .chain(c)
            .chain(stream::once(async { Ok(r#", "metadata":"#.to_string()) }))
            .chain(m)
            .chain(stream::once(async { Ok("}".to_string()) }))
    }
}
