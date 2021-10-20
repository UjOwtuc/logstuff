use std::fs::File;

use crate::partition::{self, Partitioner};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub log_level: log::LevelFilter,
    pub log_file: String,
    pub db_url: String,
    pub partitions: Vec<Box<dyn Partitioner>>,
}

impl Default for Config {
    fn default() -> Self {
        let username = std::env::var("USER").unwrap_or_else(|_| "stufftail".into());
        Config {
            log_level: log::LevelFilter::Info,
            log_file: "/dev/stderr".into(),
            db_url: format!("host=/var/run/postgresql/ user={} dbname=log", username),
            partitions: vec![
                Box::new(partition::Root::default()),
                Box::new(partition::Timerange::default()),
            ],
        }
    }
}

impl Config {
    /// Load config using path specified in options
    pub fn load(opts: &crate::cli::Options) -> Result<Config, Box<dyn ::std::error::Error>> {
        if let Some(path) = &opts.config_path {
            let reader = File::open(path)?;
            Ok(serde_yaml::from_reader(reader)?)
        } else {
            Ok(Config::default())
        }
    }
}
