#[macro_use]
extern crate log;
use clap::Parser;
use std::path::PathBuf;
use std::process::exit;

mod app;
mod application;
mod config;
mod counts;
mod events;
mod interval;

use app::App;
use application::Application;
use config::Config;

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

fn main() {
    if let Err(err) = run::<App>() {
        error!("encountered a fatal error: {}", err);
        eprintln!("encountered a fatal error: {}", err);
        exit(1);
    }
}

fn run<T: Application>() -> Result<(), Box<dyn std::error::Error>> {
    let opts = Args::parse();
    let config = Config::load(&opts)?;
    if opts.dump_config {
        eprintln!("{}", serde_yaml::to_string(&config)?)
    }
    application::run::<T>(opts, config)?;
    Ok(())
}
