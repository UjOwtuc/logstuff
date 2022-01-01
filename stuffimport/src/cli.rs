use std::path::PathBuf;

use clap::{crate_name, crate_version, App, Arg};

#[derive(Debug)]
pub struct Options {
    /// Path to the configuration file to use
    pub config_path: Option<PathBuf>,

    /// Dump parsed config to stderr
    pub dump_config: bool,
}

impl Options {
    pub fn load() -> Options {
        let matches = App::new(crate_name!())
            .version(crate_version!())
            .author("Karsten Borgwaldt <kb@spambri.de>")
            .about("Event importer for postgres databases")
            .arg(
                Arg::new("dump_config")
                    .short('d')
                    .long("dump-config")
                    .help("Dump config file after loading it to stderr")
                    .takes_value(false),
            )
            .arg(
                Arg::new("config_file")
                    .short('c')
                    .long("config")
                    .value_name("FILE")
                    .help("Sets a custom config file")
                    .takes_value(true),
            )
            .get_matches();

        Options {
            config_path: matches.value_of("config_file").map(PathBuf::from),
            dump_config: matches.is_present("dump_config"),
        }
    }
}
