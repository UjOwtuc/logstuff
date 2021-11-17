use serde_derive::{Deserialize, Serialize};
use std::fs::File;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use logstuff::tls::TlsSettings;

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum TlsClientAuth {
    Required { trusted_certs: String },
    Optional { trusted_certs: String },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct HttpSettings {
    pub listen_address: SocketAddr,
    pub use_tls: bool,
    pub tls_cert: String,
    pub tls_key: String,
    pub tls_client_auth: Option<TlsClientAuth>,
}

impl Default for HttpSettings {
    fn default() -> Self {
        Self {
            listen_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
            use_tls: false,
            tls_cert: String::new(),
            tls_key: String::new(),
            tls_client_auth: None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub db_url: String,
    pub auto_restart: bool,
    pub postgres_tls: TlsSettings,
    pub http_settings: HttpSettings,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            db_url:
                "user=stuffstream password=stuffstream-password host=127.0.0.1 port=5432 dbname=log"
                    .into(),
            auto_restart: false,
            postgres_tls: TlsSettings::default(),
            http_settings: HttpSettings::default(),
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
