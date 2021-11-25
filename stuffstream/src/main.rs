#[macro_use]
extern crate log;
use std::process::exit;

mod app;
mod application;
mod cli;
mod config;
mod counts;
mod events;
mod interval;

use app::App;
use application::Application;
use config::Config;

fn main() {
    if let Err(err) = run::<App>() {
        error!("encountered a fatal error: {}", err);
        eprintln!("encountered a fatal error: {}", err);
        exit(1);
    }
}

fn run<T: Application>() -> Result<(), Box<dyn std::error::Error>> {
    let opts = cli::Options::load();
    let config = Config::load(&opts)?;
    if opts.dump_config {
        eprintln!("{}", serde_yaml::to_string(&config)?)
    }
    application::run::<T>(opts, config)?;
    Ok(())
}
