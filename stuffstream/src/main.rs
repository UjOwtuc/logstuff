use bb8_postgres::tokio_postgres::{types::ToSql, IsolationLevel, NoTls};
use bb8_postgres::{bb8, PostgresConnectionManager};
use chrono::{DateTime, Duration, FixedOffset};
use futures::TryStreamExt;
use serde_derive::{Deserialize, Serialize};
use serde_json::Value;
use std::convert::Infallible;
use std::iter::Iterator;
use warp::http::{Response, StatusCode};
use warp::Filter;

use logstuff::query::parse_query;

#[derive(Serialize, Deserialize, Debug)]
struct EventsRequest {
    start: DateTime<FixedOffset>,
    end: DateTime<FixedOffset>,
    query: Option<String>,
    limit_events: Option<i64>,
}

#[tokio::main]
async fn main() {
    let manager = PostgresConnectionManager::new_from_stringlike(
        "host=/var/run/postgresql/ user=karsten dbname=log",
        NoTls,
    )
    .unwrap();
    let dbpool = bb8::Pool::builder()
        .max_size(3)
        .build(manager)
        .await
        .unwrap();

    let events = warp::get()
        .and(warp::path("events"))
        .and(warp::query::<EventsRequest>())
        .and(with_db(dbpool.clone()))
        .and_then(events_handler);

    warp::serve(events).run(([127, 0, 0, 1], 8080)).await;
}

async fn events_handler(
    params: EventsRequest,
    db: DBPool,
) -> Result<impl warp::Reply, warp::Rejection> {
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(warp::hyper::Body::wrap_stream(
            fetch_events(params, db).await,
        ))
        .unwrap())
}

async fn fetch_events(
    params: EventsRequest,
    db: DBPool,
) -> impl futures::Stream<Item = Result<String, impl std::error::Error>> {
    println!("{:?}", params);
    let mut conn = db.get().await.unwrap();
    let transaction = conn
        .build_transaction()
        .isolation_level(IsolationLevel::RepeatableRead)
        .start()
        .await
        .unwrap();
    transaction
        .execute("drop view if exists tail", &[])
        .await
        .unwrap();
    transaction.execute(format!(
        "create temporary view tail as select id, tstamp, doc, search from logs where tstamp between '{}' and '{}'", params.start.to_rfc3339(), params.end.to_rfc3339()).as_str(), &[]
    ).await.unwrap();

    let (expr, query_params) = match params.query {
        Some(query) => parse_query(&query).unwrap(),
        None => ("1 = 1".to_string(), Vec::new()),
    };
    let table = "tail";

    let fields_query = format!(
        r#"
    select key::varchar, jsonb_object_agg(coalesce(value::text, ''), count::integer) as values from (
        select row_number() over (
        partition by key
        order by count desc) as row_number,
        count, key, value
        from (
        select count(*), key, value
        from (
            select doc
            from {}
            where {}
            order by tstamp desc
            limit 500
        ) limited_logs, jsonb_each_text(doc)
        group by key, value
        order by key, count desc
        ) counted
    ) ranked
    where row_number <= 5
    group by key
    "#,
        table, expr
    );

    let mut next_param_id = query_params.len() + 1;
    let events_query = format!(
        r#"
    select jsonb_build_object('timestamp', tstamp, 'id', id, 'source', doc) as doc
    from {}
    where {}
    order by tstamp desc
    limit ${}
        "#,
        table, expr, next_param_id,
    );
    next_param_id += 1;

    let duration = params.end.signed_duration_since(params.start);
    let trunc = if duration <= Duration::hours(1) {
        "second"
    } else if duration <= Duration::days(1) {
        "minute"
    } else if duration <= Duration::days(30) {
        "hour"
    } else {
        "day"
    };
    println!("counts scale: {}", trunc);

    let counts_query = format!(
        r#"
            select dt, coalesce(l.count, 0) as count
            from generate_series(${}, ${}, '1 {}'::interval) dt
            left join (select date_trunc('{}', tstamp) as ld, count(tstamp) as count
                from {}
                where {}
                and tstamp between ${} and ${}
                group by 1) l
            on date_trunc('{}', dt) = l.ld
            order by dt
        "#,
        next_param_id,
        next_param_id + 1,
        trunc,
        trunc,
        table,
        expr,
        next_param_id,
        next_param_id + 1,
        trunc
    );

    let metadata_query = format!(
        r#"
            select 'event_count' as key, count_estimate('select * from {}') as value
        "#,
        table
    );

    let full_query = format!(
        r#"
                select fields.doc || events.doc || counts.doc || metadata.doc as doc
                from
                (select jsonb_build_object('fields', jsonb_object_agg(key, values)) as doc from ({}) f) fields,
                (select jsonb_build_object('events', jsonb_agg(doc)) as doc from ({}) e) events,
                (select jsonb_build_object('counts', jsonb_object_agg(dt, count)) as doc from ({}) c) counts,
                (select jsonb_build_object('metadata', jsonb_object_agg(key, value)) as doc from ({}) m) metadata
            "#,
        fields_query, events_query, counts_query, metadata_query
    );

    type Param = (dyn ToSql + Sync);
    let query = transaction
        .query_raw(
            full_query.as_str(),
            query_params
                .iter()
                .map(|e| e as &Param)
                .chain(std::iter::once::<&Param>(&params.limit_events))
                .chain(std::iter::once::<&Param>(&params.start))
                .chain(std::iter::once::<&Param>(&params.end))
                .collect::<Vec<&Param>>(),
        )
        .await
        .unwrap();

    query.map_ok(|row| format!("{}\n", row.get::<&str, Value>("doc").to_string()))
}

type DBPool = bb8::Pool<PostgresConnectionManager<NoTls>>;
fn with_db(db_pool: DBPool) -> impl Filter<Extract = (DBPool,), Error = Infallible> + Clone {
    warp::any().map(move || db_pool.clone())
}
