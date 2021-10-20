use std::path::PathBuf;
use std::str::FromStr;

use clap::{crate_version, App, Arg};

#[derive(Debug)]
pub struct Options {
    /// Path to the configuration file to use
    pub config_path: Option<PathBuf>,

    /// The max level of logging
    pub max_log_level: Option<log::LevelFilter>,

    /// Log file path
    pub log_file: Option<String>,

    /// Dump parsed config to stderr
    pub dump_config: bool,
}

impl Options {
    pub fn load() -> Options {
        let matches = App::new("stuffimport")
            .version(crate_version!())
            .author("Karsten Borgwaldt <kb@spambri.de>")
            .about("Event importer for postgres databases")
            .arg(
                Arg::with_name("dump_config")
                    .short("d")
                    .long("dump-config")
                    .help("Dump config file after loading it to stderr")
                    .takes_value(false),
            )
            .arg(
                Arg::with_name("config_file")
                    .short("c")
                    .long("config")
                    .value_name("FILE")
                    .help("Sets a custom config file")
                    .takes_value(true),
            )
            .arg(
                Arg::with_name("log_level")
                    .short("l")
                    .long("log-level")
                    .help("Set log level")
                    .possible_values(&["trace", "debug", "info", "warn", "error", "off"])
                    .case_insensitive(true)
                    .value_name("LEVEL")
                    .takes_value(true),
            )
            .arg(
                Arg::with_name("log_file")
                    .short("L")
                    .long("log-file")
                    .help("Write logs to given file")
                    .value_name("FILE")
                    .takes_value(true),
            )
            .get_matches();

        let map_path = |path| PathBuf::from(path);
        Options {
            config_path: matches.value_of("config_file").map(map_path),
            max_log_level: matches
                .value_of("log_level")
                .map(|level| log::LevelFilter::from_str(level).unwrap()),
            log_file: matches.value_of("log_file").map(|file| file.to_string()),
            dump_config: matches.is_present("dump_config"),
        }
    }
}
