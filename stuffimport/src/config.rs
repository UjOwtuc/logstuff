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
        Config {
            log_level: log::LevelFilter::Info,
            log_file: "/dev/stderr".into(),
            db_url: "user=stuffimport password=stuffimport-password host=127.0.0.1 port=5432 dbname=log target_session_attrs=read-write".into(),
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
