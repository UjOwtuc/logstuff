#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;

extern crate clap;
extern crate serde;
extern crate serde_yaml;

use std::process::exit;

mod app; // app stuff for *this* program
mod application; // general app stuff
mod config;
mod partition;

use app::App;
use application::Application;
use clap::Parser;
use config::Config;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// Sets a custom config file
    #[arg(short, long = "config-file", value_name = "FILE")]
    pub config_path: Option<PathBuf>,

    /// Dump config file after loading it to stderr
    #[arg(short, long)]
    pub dump_config: bool,
}

/// The main function
///
/// Delegates to `run()` to provide error handling.
fn main() {
    if let Err(err) = run::<App>() {
        error!("encountered a fatal error: {}", err);
        println!("encountered a fatal error: {}", err);
        exit(1);
    }
}

fn run<T: Application>() -> Result<(), Box<dyn ::std::error::Error>> {
    // Load command-line options
    let opts = Args::parse();

    // Load configuration
    let config = Config::load(&opts)?;

    if opts.dump_config {
        eprintln!("{}", serde_yaml::to_string(&config)?)
    }

    // Initialize the application.
    application::run::<T>(opts, config)?;
    Ok(())
}
