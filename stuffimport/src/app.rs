use chrono::Utc;
use simplelog::{ConfigBuilder, WriteLogger};
use std::fs::OpenOptions;
use std::{fmt, io};

use logstuff::event::{Event, RsyslogdEvent};

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
    Logger(log::SetLoggerError),
    Db(postgres::Error),
    Io(io::Error),
    Json(serde_json::Error),
    Partition(partition::Error),
}

impl Application for App {
    type Err = Error;

    fn new(opts: Options, config: Config) -> Result<Self, Self::Err> {
        let log_level = opts.max_log_level.unwrap_or(config.log_level);
        let log_file = opts.log_file.unwrap_or(config.log_file);
        WriteLogger::init(
            log_level,
            ConfigBuilder::new()
                .set_max_level(log_level)
                .set_time_format_str("%F %T")
                .set_time_to_local(true)
                .build(),
            reopen::Reopen::new(Box::new(move || {
                OpenOptions::new()
                    .create(true)
                    .write(true)
                    .append(true)
                    .open(log_file.to_string())
            }))?,
        )?;

        let client = postgres::Client::connect(&config.db_url, postgres::NoTls)?;

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
            &[&event.timestamp.with_timezone(&Utc), &event.doc, &search],
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

impl From<log::SetLoggerError> for Error {
    fn from(error: log::SetLoggerError) -> Self {
        Self::Logger(error)
    }
}

impl From<partition::Error> for Error {
    fn from(error: partition::Error) -> Self {
        Self::Partition(error)
    }
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::Error::*;
        match self {
            Logger(e) => write!(f, "Could not set logger: {}", e),
            Db(e) => write!(f, "Database connection error: {}", e),
            Io(e) => write!(f, "I/O Error: {}", e),
            Json(e) => write!(f, "json de-/serialization failed: {}", e),
            Partition(e) => write!(f, "Could not create partitions: {}", e),
        }
    }
}
