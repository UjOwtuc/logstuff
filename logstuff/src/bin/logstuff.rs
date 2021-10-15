use chrono::Utc;
use std::error::Error;
use std::io::{self, BufRead};

use logstuff::event::{Event, RsyslogdEvent};

fn format_message(line: &str) -> Result<Event, Box<dyn Error>> {
    let rsyslog_event: RsyslogdEvent = serde_json::from_str(line)?;
    let event: Event = rsyslog_event.into();
    eprintln!("{}", event);
    Ok(event)
}

fn main() {
    let mut client = postgres::Client::connect(
        "host=/var/run/postgresql/ user=karsten dbname=log",
        postgres::NoTls,
    )
    .unwrap();

    let stdin = io::stdin();
    println!("OK"); // tell rsyslogd that we are ready to process events

    for line in stdin.lock().lines() {
        let line = line.unwrap();
        eprintln!("raw: {}", line);
        match format_message(&line) {
            Ok(event) => {
                let search = event.search_string();
                client
                    .execute(
                        "insert into logs (tstamp, doc, search) values ($1, $2, to_tsvector($3))",
                        &[&event.timestamp.with_timezone(&Utc), &event.doc, &search],
                    )
                    .unwrap();
                println!("OK");
            }
            Err(err) => {
                println!("error");
                eprintln!("could not format message: {}", err);
            }
        };
    }
}
