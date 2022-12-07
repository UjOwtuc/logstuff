use logstuff::tls::TlsSettings;
use std::fs::File;

use crate::partition::{self, Partitioner};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub db_url: String,
    pub partitions: Vec<Box<dyn Partitioner>>,
    pub tls: TlsSettings,
    pub use_vars_msg: bool,
    pub statement_cache_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            db_url: "user=stuffimport password=stuffimport-password host=127.0.0.1 port=5432 dbname=log target_session_attrs=read-write".into(),
            partitions: vec![
                Box::new(partition::Root::default()),
                Box::new(partition::Timerange::default()),
            ],
            tls: TlsSettings::default(),
            use_vars_msg: true,
            statement_cache_size: 3,
        }
    }
}

impl Config {
    /// Load config using path specified in options
    pub fn load(opts: &crate::Args) -> Result<Config, Box<dyn ::std::error::Error>> {
        if let Some(path) = &opts.config_path {
            let reader = File::open(path)?;
            Ok(serde_yaml::from_reader(reader)?)
        } else {
            Ok(Config::default())
        }
    }
}
