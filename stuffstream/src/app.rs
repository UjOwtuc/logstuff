use bb8_postgres::tokio_postgres;
use bb8_postgres::{bb8, PostgresConnectionManager};
use futures::lock::Mutex;
use rustls::client::ClientConfig;
use std::convert::Infallible;
use std::sync::Arc;
use std::{fmt, io};
use tokio_postgres_rustls::MakeRustlsConnect;
use warp::http::StatusCode;
use warp::{reject, reply, Filter, Rejection, Reply};

use logstuff::tls;
use logstuff_query::{ExpressionParser, IdentifierParser};

use crate::application::{Application, Stopping};
use crate::config::{Config, HttpSettings, TlsClientAuth};
use crate::counts;
use crate::events;
use crate::Args;

pub(crate) type DBPool = bb8::Pool<PostgresConnectionManager<MakeRustlsConnect>>;

/// Error type for the core program logic
#[derive(Debug)]
pub enum Error {
    Logger(log::SetLoggerError),
    Io(io::Error),
    Db(tokio_postgres::Error),
    Tls(tls::Error),
}

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

impl Application for App {
    type Err = Error;

    fn new(_opts: Args, config: Config) -> Result<Self, Self::Err> {
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
pub struct MalformedQuery;

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

    let expr_parser = Arc::new(Mutex::new(ExpressionParser::default()));
    let id_parser = Arc::new(Mutex::new(IdentifierParser::default()));

    let p = expr_parser.clone();
    let table = table_name.to_owned();
    let events = warp::get()
        .and(warp::path("events"))
        .and(warp::query::<events::Request>())
        .and(with_db(dbpool.clone()))
        .and_then(move |params, dbpool| {
            events::handler(p.clone(), table.to_owned(), params, dbpool)
        });

    let table = table_name.to_owned();
    let counts = warp::get()
        .and(warp::path("counts"))
        .and(warp::query::<counts::Request>())
        .and(with_db(dbpool.clone()))
        .and_then(move |params, dbpool| {
            counts::handler(
                expr_parser.clone(),
                id_parser.clone(),
                table.to_owned(),
                params,
                dbpool,
            )
        });

    let routes = events.or(counts).recover(handle_rejection);
    let server = warp::serve(routes);
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
