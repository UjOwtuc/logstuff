use postgres_native_tls::MakeTlsConnector;
use std::{fmt, io};

use logstuff::event::{Event, RsyslogdEvent};
use logstuff::tls;

use crate::application::{Application, Stopping};
use crate::cli::Options;
use crate::config::Config;
use crate::partition::{self, Partitioner};

/// Core program logic
///
/// Must implement the `Application` trait.
pub struct App {
    client: postgres::Client,
    partitions: Vec<Box<dyn partition::Partitioner>>,
}

/// Error type for the core program logic
#[derive(Debug)]
pub enum Error {
    Db(postgres::Error),
    Io(io::Error),
    Json(serde_json::Error),
    Partition(partition::Error),
    Tls(tls::Error),
}

impl Application for App {
    type Err = Error;

    fn new(_opts: Options, config: Config) -> Result<Self, Self::Err> {
        env_logger::init();
        let connector = MakeTlsConnector::new(config.tls.connector()?);
        let client = postgres::Client::connect(&config.db_url, connector)?;

        // tell rsyslogd that we are ready
        println!("OK");

        Ok(App {
            client,
            partitions: config.partitions,
        })
    }

    fn run_once(&mut self) -> Result<Stopping, Self::Err> {
        let mut line = String::new();
        let bytes = io::stdin().read_line(&mut line)?;
        let line: &str = line.trim();

        if !line.is_empty() {
            self.handle_event(line)?;
        }

        if bytes == 0 {
            info!("input at EOF");
            Ok(Stopping::Yes)
        } else {
            Ok(Stopping::No)
        }
    }
}

impl App {
    fn insert_single_shot(&mut self, event: &Event, search: &str) -> Result<(), Error> {
        let root_table = self.partitions[0].table_name(event)?;
        self.client.execute(
            format!(
                "insert into {} (tstamp, doc, search) values ($1, $2, to_tsvector($3))",
                root_table
            )
            .as_str(),
            &[&event.timestamp, &event.doc, &search],
        )?;
        Ok(())
    }

    fn insert_event(&mut self, event: &Event) -> Result<(), Error> {
        let search = event.search_string();
        if self.insert_single_shot(event, &search).is_err() {
            info!("Event insertion failed, trying to create missing partitions");
            crate::partition::create_tables(
                &mut self.client,
                event,
                &self
                    .partitions
                    .iter()
                    .map(|boxed| (*boxed).as_ref() as &dyn Partitioner)
                    .collect::<Vec<&dyn Partitioner>>(),
            )?;
            debug!("Partitions created, retrying event insertion");
            self.insert_single_shot(event, &search)
                .expect("event insertion still failed after creating partitions");
        }

        Ok(())
    }

    fn handle_event(&mut self, line: &str) -> Result<(), Error> {
        match serde_json::from_str::<RsyslogdEvent>(line) {
            Ok(rsyslog_event) => {
                let stuff_event: Event = rsyslog_event.into();
                self.insert_event(&stuff_event)?;
                println!("OK");
            }
            Err(error) => error!("could not parse event: '{}': {}", line, error),
        }
        Ok(())
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<postgres::Error> for Error {
    fn from(error: postgres::Error) -> Self {
        Self::Db(error)
    }
}

impl From<partition::Error> for Error {
    fn from(error: partition::Error) -> Self {
        Self::Partition(error)
    }
}

impl From<tls::Error> for Error {
    fn from(error: tls::Error) -> Self {
        Self::Tls(error)
    }
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::Error::*;
        match self {
            Db(e) => write!(f, "Database connection error: {}", e),
            Io(e) => write!(f, "I/O Error: {}", e),
            Json(e) => write!(f, "json de-/serialization failed: {}", e),
            Partition(e) => write!(f, "Could not create partitions: {}", e),
            Tls(e) => write!(f, "TLS Error: {}", e),
        }
    }
}
