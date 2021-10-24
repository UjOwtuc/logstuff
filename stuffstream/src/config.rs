use serde_derive::{Deserialize, Serialize};
use std::fs::File;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub log_level: log::LevelFilter,
    pub log_file: String,
    pub db_url: String,
    pub listen_address: SocketAddr,
    pub auto_restart: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            log_level: log::LevelFilter::Info,
            log_file: "/dev/stderr".into(),
            db_url:
                "user=stuffstream password=stuffstream-password host=127.0.0.1 port=5432 dbname=log"
                    .into(),
            listen_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
            auto_restart: false,
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
