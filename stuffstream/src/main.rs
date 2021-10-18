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
    limit_events: Option<usize>,
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
        "create temporary view tail as select id, tstamp, doc from logs where tstamp between '{}' and '{}'", params.start.to_rfc3339(), params.end.to_rfc3339()).as_str(), &[]
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

    let events_query = format!(
        r#"
    select jsonb_build_object('timestamp', tstamp, 'id', id, 'source', doc) as doc
    from {}
    where {}
    order by tstamp desc
    limit {}
        "#,
        table,
        expr,
        params.limit_events.unwrap_or(5000)
    );

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

    let our_params = vec![params.start, params.end];
    let next_param_id = query_params.len() + 1;
    let counts_query = format!(
        r#"
        select date_trunc('{}', dd) as dt, count(l) as count
        from generate_series(${}, ${}, '1 {}'::interval) dd
        left join {} l
        on date_trunc('{}', dd) = date_trunc('{}', l.tstamp)
        where {}
        group by dd
        order by dd
        "#,
        trunc,
        next_param_id,
        next_param_id + 1,
        trunc,
        table,
        trunc,
        trunc,
        expr
    );

    let query = transaction
        .query_raw(
            format!(
                r#"
                select fields.doc || events.doc || counts.doc as doc
                from
                (select jsonb_build_object('fields', jsonb_object_agg(key, values)) as doc from ({}) f) fields,
                (select jsonb_build_object('events', jsonb_agg(doc)) as doc from ({}) e) events,
                (select jsonb_build_object('counts', jsonb_object_agg(dt, count)) as doc from ({}) c) counts
            "#,
                fields_query, events_query, counts_query
            )
            .as_str(),
            query_params
                .iter()
                .chain(query_params.iter())
                .map(|e| e as &(dyn ToSql + Sync))
                .chain(our_params.iter().map(|e| e as &(dyn ToSql + Sync)))
                .collect::<Vec<&(dyn ToSql + Sync)>>(),
        )
        .await
        .unwrap();

    query.map_ok(|row| format!("{}\n", row.get::<&str, Value>("doc").to_string()))
}

type DBPool = bb8::Pool<PostgresConnectionManager<NoTls>>;
fn with_db(db_pool: DBPool) -> impl Filter<Extract = (DBPool,), Error = Infallible> + Clone {
    warp::any().map(move || db_pool.clone())
}
