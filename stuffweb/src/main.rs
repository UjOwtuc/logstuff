use chrono::{DateTime, Duration, FixedOffset, Utc};
use logstuff::query::parse_query;
use postgres::types::ToSql;
use rouille::{Request, Response};
use serde_derive::Serialize;
use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};

type TopValues = HashMap<String, i32>;

#[derive(Serialize)]
struct EventsReply {
    fields: HashMap<String, TopValues>,
    events: Vec<serde_json::Value>,
    counts: HashMap<DateTime<Utc>, i64>,
}

fn top_fields(
    conn: &mut postgres::Client,
    expr: &str,
    params: &[&(dyn ToSql + Sync)],
    table: &str,
) -> Result<HashMap<String, TopValues>, Box<dyn Error>> {
    let top_fields = conn.query(
        format!(
            r#"
        select count::integer, key::varchar, coalesce(value::text, '') as value from (
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
        where row_number <= 5"#,
            table, expr
        )
        .as_str(),
        params,
    )?;

    let mut top_fields_map: HashMap<String, TopValues> = HashMap::new();
    for row in top_fields {
        let key: String = row.get("key");
        let value = row.get::<&str, String>("value").to_string();
        let count = row.get("count");
        match top_fields_map.get_mut(&key) {
            Some(values) => {
                values.insert(value, count);
            }
            None => {
                let mut data = TopValues::new();
                data.insert(value, count);
                top_fields_map.insert(key, data);
            }
        };
    }
    Ok(top_fields_map)
}

fn events(
    conn: &mut postgres::Client,
    expr: &str,
    params: &[&(dyn ToSql + Sync)],
    table: &str,
) -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
    let events = conn.query(format!("select id, jsonb_build_object('timestamp', tstamp, 'id', id, 'source', doc) as doc from {} where {} order by tstamp desc", table, expr).as_str(), params)?;
    Ok(events.iter().map(|row| row.get("doc")).collect())
}

fn counts(
    conn: &mut postgres::Client,
    start: &DateTime<FixedOffset>,
    end: &DateTime<FixedOffset>,
    expr: &str,
    query_params: &[&(dyn ToSql + Sync)],
    table: &str,
) -> Result<HashMap<DateTime<Utc>, i64>, Box<dyn Error>> {
    let next_param_id = query_params.len() + 1;
    let mut our_params = Vec::from(query_params);
    our_params.push(&start);
    our_params.push(&end);

    let duration = end.signed_duration_since(*start);
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

    let counts = conn.query(format!("select date_trunc('{}', dd) as t, count(l) as count from generate_series(${}, ${}, '1 {}'::interval) dd left join {} l on date_trunc('{}', dd) = date_trunc('{}', l.tstamp) where {} group by dd order by dd",
    trunc, next_param_id, next_param_id +1, trunc, table, trunc, trunc, expr).as_str(), &our_params)?;
    Ok(counts
        .iter()
        .map(|row| (row.get("t"), row.get("count")))
        .collect())
}

fn prepare_table(
    conn: &mut postgres::Client,
    start: &DateTime<FixedOffset>,
    end: &DateTime<FixedOffset>,
) -> Result<(), Box<dyn Error>> {
    conn.execute("drop view if exists tail", &[])?;
    conn.execute(format!(
        "create temporary view tail as select id, tstamp, doc from logs where tstamp between '{}' and '{}'", start.to_rfc3339(), end.to_rfc3339()).as_str(), &[]
    )?;
    Ok(())
}

struct EventsRequest {
    start: DateTime<FixedOffset>,
    end: DateTime<FixedOffset>,
    query: Option<String>,
}

struct ErrorReply {
    status: u16,
    text: Option<String>,
}

impl ErrorReply {
    fn new(status: u16, text: impl Into<String>) -> Self {
        ErrorReply {
            status,
            text: Some(text.into()),
        }
    }
}

impl From<chrono::ParseError> for ErrorReply {
    fn from(err: chrono::ParseError) -> Self {
        Self {
            status: 400,
            text: Some(format!("parse error: {:?}", err)),
        }
    }
}

impl From<Box<dyn Error>> for ErrorReply {
    fn from(err: Box<dyn Error>) -> Self {
        Self {
            status: 500,
            text: Some(format!("server error: {:?}", err)),
        }
    }
}

fn parse_request(request: &Request) -> Result<EventsRequest, ErrorReply> {
    let query = if request
        .get_param("query")
        .unwrap_or_else(|| "".into())
        .is_empty()
    {
        None
    } else {
        request.get_param("query")
    };
    Ok(EventsRequest {
        start: DateTime::parse_from_rfc3339(&request.get_param("start").ok_or_else(|| {
            ErrorReply {
                status: 400,
                text: Some("missing parameter \"start\"".to_string()),
            }
        })?)?,
        end: DateTime::parse_from_rfc3339(&request.get_param("end").ok_or_else(|| {
            ErrorReply {
                status: 400,
                text: Some("missing parameter \"start\"".to_string()),
            }
        })?)?,
        query,
    })
}

fn handle_request(
    request: &Request,
    db: Arc<Mutex<postgres::Client>>,
) -> Result<EventsReply, ErrorReply> {
    let params = parse_request(request)?;
    let mut conn = db
        .lock()
        .map_err(|e| ErrorReply::new(500, format!("Could not get database connection: {:?}", e)))?;
    prepare_table(&mut conn, &params.start, &params.end)?;

    let (expr, query_params) = if let Some(query) = params.query {
        parse_query(&query)?
    } else {
        ("1 = 1".to_string(), Vec::new())
    };

    let ref_params = query_params
        .iter()
        .map(|e| e.as_ref())
        .collect::<Vec<&(dyn ToSql + Sync)>>();
    Ok(EventsReply {
        fields: top_fields(&mut conn, &expr, &ref_params, "tail")?,
        events: events(&mut conn, &expr, &ref_params, "tail")?,
        counts: counts(
            &mut conn,
            &params.start,
            &params.end,
            &expr,
            &ref_params,
            "tail",
        )?,
    })
}

fn main() {
    let client = Arc::new(Mutex::new(
        postgres::Client::connect(
            "host=/var/run/postgresql/ user=karsten dbname=log",
            postgres::NoTls,
        )
        .unwrap(),
    ));
    rouille::start_server("127.0.0.1:8000", move |request| {
        match handle_request(request, client.clone()) {
            Ok(reply) => Response::json(&reply),
            Err(err) => {
                let mut response = match err.text {
                    Some(text) => Response::text(text),
                    None => Response::text(""),
                };
                response.status_code = err.status;
                response
            }
        }
    });
}
