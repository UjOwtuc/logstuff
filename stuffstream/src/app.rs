use bb8_postgres::tokio_postgres::{self, types::ToSql, IsolationLevel};
use bb8_postgres::{bb8, PostgresConnectionManager};
use futures::TryStreamExt;
use rustls::client::ClientConfig;
use serde_derive::{Deserialize, Serialize};
use serde_json::Value;
use std::convert::Infallible;
use std::iter::Iterator;
use std::net::SocketAddr;
use std::{fmt, io};
use time::{macros::format_description, Duration, OffsetDateTime, UtcOffset};
use tokio_postgres_rustls::MakeRustlsConnect;
use warp::http::{Response, StatusCode};
use warp::Filter;

use logstuff::query::parse_query;
use logstuff::serde::de::rfc3339;
use logstuff::tls;

use crate::application::{Application, Stopping};
use crate::cli::Options;
use crate::config::Config;

/// Core program logic
///
/// Must implement the `Application` trait.
pub struct App {
    auto_restart: bool,
    listen_address: SocketAddr,
    db_url: String,
    tls_config: tls::ClientConfig,
}

/// Error type for the core program logic
#[derive(Debug)]
pub enum Error {
    Logger(log::SetLoggerError),
    Io(io::Error),
    Db(tokio_postgres::Error),
    Tls(tls::Error),
}

#[derive(Serialize, Deserialize, Debug)]
struct EventsRequest {
    #[serde(deserialize_with = "rfc3339")]
    start: OffsetDateTime,
    #[serde(deserialize_with = "rfc3339")]
    end: OffsetDateTime,
    query: Option<String>,
    limit_events: Option<i64>,
}

impl Application for App {
    type Err = Error;

    fn new(_opts: Options, config: Config) -> Result<Self, Self::Err> {
        env_logger::try_init()?;
        Ok(App {
            auto_restart: config.auto_restart,
            listen_address: config.listen_address,
            db_url: config.db_url,
            tls_config: config.tls.client_config()?,
        })
    }

    fn run_once(&mut self) -> Result<Stopping, Self::Err> {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(start_server(
                &self.listen_address,
                &self.db_url,
                &self.tls_config,
            ))?;

        if self.auto_restart {
            Ok(Stopping::No)
        } else {
            Ok(Stopping::Yes)
        }
    }
}

impl App {}

async fn start_server(
    listen_address: &SocketAddr,
    db_url: &str,
    tls_config: &ClientConfig,
) -> Result<(), Error> {
    let connector = MakeRustlsConnect::new(tls_config.clone());
    let manager = PostgresConnectionManager::new_from_stringlike(db_url, connector)?;
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

    warp::serve(events).run(*listen_address).await;
    Ok(())
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

    let rfc3339_format = format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]Z");
    transaction.execute(
        format!(
            "create temporary view tail as select id, tstamp, doc, search from logs where tstamp between '{}' and '{}'",
            params.start.to_offset(UtcOffset::UTC).format(&rfc3339_format).unwrap(),
            params.end.to_offset(UtcOffset::UTC).format(&rfc3339_format).unwrap()
        ).as_str(), &[]
    ).await.unwrap();

    let (expr, query_params) = match params.query {
        Some(query) => parse_query(&query).unwrap(),
        None => ("1 = 1".to_string(), Vec::new()),
    };
    let table = "tail";
    let event_limit_param_id = query_params.len() + 1;
    let start_tstamp_param_id = query_params.len() + 2;
    let end_tstamp_param_id = query_params.len() + 3;

    let fields_query = format!(
        r#"
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
        "#,
        table, expr, start_tstamp_param_id, end_tstamp_param_id
    );

    let events_query = format!(
        r#"
            select jsonb_build_object('timestamp', tstamp, 'id', id, 'source', doc) as doc
            from {}
            where {}
            order by tstamp desc
            limit ${}
        "#,
        table, expr, event_limit_param_id,
    );

    let duration = Duration::new(
        params.end.unix_timestamp() - params.start.unix_timestamp(),
        0,
    );
    let (trunc, interval) = if duration <= Duration::hours(1) {
        ("second", 1)
    } else if duration <= Duration::days(1) {
        ("minute", 60)
    } else if duration <= Duration::days(30) {
        ("hour", 3600)
    } else {
        ("day", 3600 * 24)
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
        start_tstamp_param_id,
        end_tstamp_param_id,
        trunc,
        trunc,
        table,
        expr,
        start_tstamp_param_id,
        end_tstamp_param_id,
        trunc
    );

    let metadata_query = format!(
        r#"
            select 'event_count' as key, count_estimate('select * from {}') as value
            union
            select 'counts_interval_sec' as key, {} as value
        "#,
        table, interval
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

type DBPool = bb8::Pool<PostgresConnectionManager<MakeRustlsConnect>>;
fn with_db(db_pool: DBPool) -> impl Filter<Extract = (DBPool,), Error = Infallible> + Clone {
    warp::any().map(move || db_pool.clone())
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<log::SetLoggerError> for Error {
    fn from(error: log::SetLoggerError) -> Self {
        Self::Logger(error)
    }
}

impl From<tokio_postgres::Error> for Error {
    fn from(error: tokio_postgres::Error) -> Self {
        Self::Db(error)
    }
}

impl From<tls::Error> for Error {
    fn from(error: tls::Error) -> Self {
        Self::Tls(error)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::Error::*;
        match self {
            Logger(e) => write!(f, "Could not set logger: {}", e),
            Io(e) => write!(f, "I/O Error: {}", e),
            Db(e) => write!(f, "Database connection error: {}", e),
            Tls(e) => write!(f, "TLS setup error: {}", e),
        }
    }
}
