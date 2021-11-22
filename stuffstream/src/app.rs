use bb8_postgres::tokio_postgres::{self, types::ToSql};
use bb8_postgres::{bb8, PostgresConnectionManager};
use futures::lock::Mutex;
use futures::TryStreamExt;
use rustls::client::ClientConfig;
use serde_derive::{Deserialize, Serialize};
use serde_json::Value;
use std::convert::Infallible;
use std::iter::Iterator;
use std::sync::Arc;
use std::{fmt, io};
use time::{Duration, OffsetDateTime};
use tokio_postgres_rustls::MakeRustlsConnect;
use warp::http::{Response, StatusCode};
use warp::{reject, reply, Filter, Rejection, Reply};

use logstuff::serde::de::rfc3339;
use logstuff::tls;
use logstuff_query::ExpressionParser;

use crate::application::{Application, Stopping};
use crate::cli::Options;
use crate::config::{Config, HttpSettings, TlsClientAuth};

/// Core program logic
///
/// Must implement the `Application` trait.
pub struct App {
    auto_restart: bool,
    db_url: String,
    postgres_tls: tls::ClientConfig,
    http_settings: HttpSettings,
    table_name: String,
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
            db_url: config.db_url,
            postgres_tls: config.postgres_tls.client_config()?,
            http_settings: config.http_settings,
            table_name: config.root_table_name,
        })
    }

    fn run_once(&mut self) -> Result<Stopping, Self::Err> {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(start_server(
                &self.http_settings,
                &self.db_url,
                &self.postgres_tls,
                &self.table_name,
            ))?;

        if self.auto_restart {
            Ok(Stopping::No)
        } else {
            Ok(Stopping::Yes)
        }
    }
}

impl App {}

#[derive(Debug)]
struct MalformedQuery;

impl reject::Reject for MalformedQuery {}

async fn handle_rejection(err: Rejection) -> Result<impl Reply, Infallible> {
    if err.is_not_found() {
        Ok(reply::with_status("NOT_FOUND", StatusCode::NOT_FOUND))
    } else if err.find::<MalformedQuery>().is_some() {
        Ok(reply::with_status("BAD_REQUEST", StatusCode::BAD_REQUEST))
    } else {
        error!("unhandled rejection: {:?}", err);
        Ok(reply::with_status(
            "INTERNAL_SERVER_ERROR",
            StatusCode::INTERNAL_SERVER_ERROR,
        ))
    }
}

async fn start_server(
    http_settings: &HttpSettings,
    db_url: &str,
    postgres_tls: &ClientConfig,
    table_name: &str,
) -> Result<(), Error> {
    let connector = MakeRustlsConnect::new(postgres_tls.clone());
    let manager = PostgresConnectionManager::new_from_stringlike(db_url, connector)?;
    let dbpool = bb8::Pool::builder()
        .max_size(3)
        .build(manager)
        .await
        .unwrap();

    let parser = Arc::new(Mutex::new(ExpressionParser::default()));

    let table_name = table_name.to_owned();
    let events = warp::get()
        .and(warp::path("events"))
        .and(warp::query::<EventsRequest>())
        .and(with_db(dbpool.clone()))
        .and_then(move |params, dbpool| {
            events_handler(parser.clone(), table_name.to_owned(), params, dbpool)
        })
        .recover(handle_rejection);

    let server = warp::serve(events);
    if http_settings.use_tls {
        let server = server
            .tls()
            .cert_path(&http_settings.tls_cert)
            .key_path(&http_settings.tls_key);

        match &http_settings.tls_client_auth {
            None => server,
            Some(TlsClientAuth::Required { trusted_certs }) => {
                server.client_auth_required_path(trusted_certs)
            }
            Some(TlsClientAuth::Optional { trusted_certs }) => {
                server.client_auth_optional_path(trusted_certs)
            }
        }
        .run(http_settings.listen_address)
        .await;
    } else {
        server.run(http_settings.listen_address).await;
    }

    Ok(())
}

async fn events_handler(
    parser: Arc<Mutex<ExpressionParser>>,
    table_name: String,
    params: EventsRequest,
    db: DBPool,
) -> Result<impl warp::Reply, warp::Rejection> {
    let p = parser.lock().await;
    let (query, query_params) = if let Some(query) = &params.query {
        p.to_sql(query).map_err(|_| MalformedQuery)?
    } else {
        ("1 = 1".into(), Vec::new())
    };
    drop(p);
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(warp::hyper::Body::wrap_stream(
            fetch_events(query, query_params, table_name, params, db).await,
        ))
        .unwrap())
}

async fn fetch_events(
    expr: String,
    query_params: Vec<Value>,
    table: String,
    params: EventsRequest,
    db: DBPool,
) -> impl futures::Stream<Item = Result<String, impl std::error::Error>> {
    println!("{:?}", params);
    let conn = db.get().await.unwrap();
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
            and tstamp between ${} and ${}
            order by tstamp desc
            limit ${}
        "#,
        table, expr, start_tstamp_param_id, end_tstamp_param_id, event_limit_param_id,
    );

    let interval = CountsInterval::from(params.end - params.start);
    println!("interval: {:?}", interval);
    let counts_query = format!(
        r#"
            select date_trunc('{}', gen_time) as tstamp, sum(subcount) as count
            from generate_series(${}, ${}, '{}') gen_time
            left join (select date_trunc('{}', tstamp) as log_time, count(*) as subcount
                    from {}
                    where {}
                    and tstamp between ${} and ${}
                    group by log_time
            ) l
            on log_time between gen_time - '{}'::interval and gen_time
            group by tstamp
            order by tstamp
        "#,
        &interval.truncate,
        start_tstamp_param_id,
        end_tstamp_param_id,
        &interval.interval,
        &interval.truncate,
        table,
        expr,
        start_tstamp_param_id,
        end_tstamp_param_id,
        &interval.interval
    );

    let metadata_query = format!(
        r#"
            select 'event_count' as key, count_estimate('select * from {}') as value
            union
            select 'counts_interval_sec' as key, {} as value
        "#,
        table, &interval.seconds
    );

    let full_query = format!(
        r#"
            select fields.doc || events.doc || counts.doc || metadata.doc as doc
            from
            (select jsonb_build_object('fields', jsonb_object_agg(key, values)) as doc from ({}) f) fields,
            (select jsonb_build_object('events', jsonb_agg(doc)) as doc from ({}) e) events,
            (select jsonb_build_object('counts', jsonb_object_agg(tstamp, count)) as doc from ({}) c) counts,
            (select jsonb_build_object('metadata', jsonb_object_agg(key, value)) as doc from ({}) m) metadata
        "#,
        fields_query, events_query, counts_query, metadata_query
    );

    type Param = (dyn ToSql + Sync);
    let query = conn
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

const INTERVALS: &[(u64, &str, &str)] = &[
    (1, "1 seconds", "second"),
    (2, "2 seconds", "second"),
    (5, "5 seconds", "second"),
    (10, "10 seconds", "second"),
    (30, "30 seconds", "second"),
    (60, "1 minute", "minute"),
    (2 * 60, "2 minutes", "minute"),
    (5 * 60, "5 minutes", "minute"),
    (10 * 60, "10 minutes", "minute"),
    (30 * 60, "30 minutes", "minute"),
    (3600, "1 hour", "hour"),
    (2 * 3600, "2 hours", "hour"),
    (5 * 3600, "5 hours", "hour"),
    (10 * 3600, "10 hours", "hour"),
    (24 * 3600, "1 day", "day"),
    (2 * 24 * 3600, "2 days", "day"),
    (7 * 24 * 3600, "1 week", "week"),
    (2 * 7 * 24 * 3600, "2 week", "week"),
    (30 * 24 * 3600, "1 month", "month"),
    (2 * 30 * 24 * 3600, "2 months", "month"),
    (3 * 30 * 24 * 3600, "3 months", "month"),
    (4 * 30 * 24 * 3600, "4 months", "month"),
    (6 * 30 * 24 * 3600, "6 months", "month"),
    (365 * 24 * 3600, "1 year", "year"),
    (2 * 365 * 24 * 3600, "2 years", "year"),
    (5 * 365 * 24 * 3600, "5 years", "year"),
    (10 * 365 * 24 * 3600, "10 years", "year"),
    (20 * 365 * 24 * 3600, "20 years", "year"),
    (50 * 365 * 24 * 3600, "50 years", "year"),
];

#[derive(Debug)]
struct CountsInterval {
    pub seconds: u64,
    pub truncate: String,
    pub interval: String,
}

impl From<Duration> for CountsInterval {
    fn from(duration: Duration) -> Self {
        let duration: u64 = duration.whole_seconds().unsigned_abs();
        for (seconds, interval, trunc) in INTERVALS {
            if duration / seconds < 100 {
                return Self {
                    seconds: *seconds,
                    truncate: trunc.to_string(),
                    interval: interval.to_string(),
                };
            }
        }

        Self {
            seconds: 100 * 365 * 24 * 3600,
            truncate: "year".to_string(),
            interval: "100 years".to_string(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn intervals() {
        let i = CountsInterval::from(Duration::seconds(50));
        assert_eq!(i.interval, "1 seconds");

        let i = CountsInterval::from(Duration::hours(4));
        assert_eq!(i.interval, "5 minutes");
    }
}
