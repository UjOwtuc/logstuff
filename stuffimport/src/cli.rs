use std::path::PathBuf;

use clap::{crate_version, App, Arg};

#[derive(Debug)]
pub struct Options {
    /// Path to the configuration file to use
    pub config_path: Option<PathBuf>,

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
            .get_matches();

        let map_path = |path| PathBuf::from(path);
        Options {
            config_path: matches.value_of("config_file").map(map_path),
            dump_config: matches.is_present("dump_config"),
        }
    }
}
