use clap::{crate_version, App, Arg};
use log::LevelFilter;
use postgres::types::ToSql;
use std::{thread, time};

use logstuff::event::Event;
use logstuff::query::{parse_query, QueryParams};

fn max<T>(a: T, b: T) -> T
where
    T: PartialOrd,
{
    if a > b {
        a
    } else {
        b
    }
}

#[derive(Default, Debug)]
struct Settings {
    max_age: String,
    max_lines: i64,
    poll_interval_ms: u64,
    query_expr: String,
    query_params: QueryParams,
    fields: Vec<String>,
    db_config: String,
}

impl Settings {
    fn from_cli_args() -> Self {
        let username = std::env::var("USERNAME").unwrap_or_else(|_| "stufftail".into());
        let default_db_config = format!("host=/var/run/postgresql/ user={} dbname=log", username);
        let matches = App::new("stufftail")
            .about("Poll for new entries in logstuff's database.")
            .version(crate_version!())
            .arg(
                Arg::with_name("db_connection")
                    .short("d")
                    .long("database")
                    .value_name("CONFIG")
                    .help("Database connect config (see https://docs.rs/postgres/0.19.2/postgres/config/struct.Config.html for options)")
                    .takes_value(true)
                    .default_value(&default_db_config))
            .arg(
                Arg::with_name("max_age")
                    .short("a")
                    .long("max-age")
                    .value_name("AGE")
                    .help("Maximum age of printed entries (postgres interval)")
                    .takes_value(true)
                    .default_value("1 hour"),
            )
            .arg(
                Arg::with_name("max_lines")
                    .short("l")
                    .long("max-lines")
                    .value_name("NUMBER")
                    .help("Maximum number of lines to print for each poll")
                    .takes_value(true)
                    .default_value("1000")
                    .validator(|val| match val.parse::<usize>() {
                        Ok(_) => Ok(()),
                        Err(_) => Err("Not a positive integer".to_string()),
                    }),
            )
            .arg(
                Arg::with_name("poll_interval_ms")
                    .short("i")
                    .long("poll-interval")
                    .value_name("MSEC")
                    .help("Poll interval given in milliseconds")
                    .takes_value(true)
                    .default_value("500")
                    .validator(|val| match val.parse::<usize>() {
                        Ok(_) => Ok(()),
                        Err(_) => Err("Not a positive integer".to_string()),
                    }),
            )
            .arg(
                Arg::with_name("query")
                    .short("q")
                    .long("query")
                    .value_name("STRING")
                    .help("logstuff query string")
                    .takes_value(true),
            )
            .arg(
                Arg::with_name("fields")
                    .short("f")
                    .long("field")
                    .value_name("NAME")
                    .help("Print field name in output")
                    .takes_value(true)
                    .multiple(true)
                    .number_of_values(1),
            )
            .get_matches();

        let (query_expr, query_params) = match matches.value_of("query") {
            Some(query) => parse_query(query).unwrap(),
            None => ("1 = 1".to_string(), Vec::new()),
        };

        let fields = match matches.values_of("fields") {
            Some(iter) => iter.map(|e| e.to_string()).collect(),
            None => vec![
                "hostname".to_string(),
                "syslogtag".to_string(),
                "msg".to_string(),
            ],
        };

        Self {
            max_age: matches.value_of("max_age").unwrap_or("1 hour").into(),
            max_lines: matches
                .value_of("max_lines")
                .unwrap_or("1000")
                .parse()
                .unwrap(),
            poll_interval_ms: matches
                .value_of("poll_interval_ms")
                .unwrap_or("500")
                .parse()
                .unwrap(),
            query_expr,
            query_params,
            fields,
            db_config: matches
                .value_of("db_connection")
                .unwrap_or(&default_db_config)
                .to_string(),
        }
    }
}

fn prepare_query<'a>(
    client: &'_ mut postgres::Client,
    settings: &'a Settings,
) -> (postgres::Statement, Vec<&'a (dyn ToSql + Sync)>) {
    let next_param = settings.query_params.len() + 1;
    let query = format!(
        r#"
        select id, tstamp, doc from logs
        where {}
        and id > ${}
        and tstamp > now() - cast(${}::varchar as interval)
        order by id desc
        limit ${}
        "#,
        settings.query_expr,
        next_param,
        next_param + 1,
        next_param + 2
    );

    let our_params = settings
        .query_params
        .iter()
        .map(|e| e as &(dyn ToSql + Sync))
        .collect::<Vec<&(dyn ToSql + Sync)>>();

    let stmt = client.prepare(query.as_str()).unwrap();
    (stmt, our_params)
}

fn main() {
    simple_logging::log_to_stderr(LevelFilter::Info);
    let settings = Settings::from_cli_args();

    let mut client = postgres::Client::connect(
        "host=/var/run/postgresql/ user=karsten dbname=log",
        postgres::NoTls,
    )
    .unwrap();

    let (stmt, our_params) = prepare_query(&mut client, &settings);
    let mut last_id = 0;
    loop {
        let mut query_params = our_params[..].to_vec();
        query_params.push(&last_id);
        query_params.push(&settings.max_age);
        query_params.push(&settings.max_lines);
        client
            .query(&stmt, &query_params)
            .unwrap()
            .iter()
            .rev()
            .for_each(|row| {
                let event = Event {
                    timestamp: row.get("tstamp"),
                    doc: row.get("doc"),
                };
                print_event(event, &settings);
                let id: i32 = row.get("id");
                last_id = max(last_id, id);
            });
        thread::sleep(time::Duration::from_millis(settings.poll_interval_ms));
    }
}

fn print_event(event: Event, settings: &Settings) {
    println!(
        "{} {}",
        event.timestamp.format("%F %T"),
        settings
            .fields
            .iter()
            .map(|field| {
                match event.get_printable(field) {
                    Some(content) => content,
                    None => "None".to_string(),
                }
            })
            .collect::<Vec<String>>()
            .join(" ")
    );
}