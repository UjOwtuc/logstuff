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
mod cli;
mod config;
mod partition;

use app::App;
use application::Application;
use config::Config;

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
    let opts = cli::Options::load();

    // Load configuration
    let config = Config::load(&opts)?;

    if opts.dump_config {
        eprintln!("{}", serde_yaml::to_string(&config)?)
    }

    // Initialize the application.
    application::run::<T>(opts, config)?;
    Ok(())
}
