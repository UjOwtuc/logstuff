use clap::Parser;
use postgres::types::ToSql;
use postgres_native_tls::MakeTlsConnector;
use std::thread;
use time::macros::format_description;

use logstuff::event::Event;
use logstuff::tls::TlsSettings;
use logstuff_query::{ExpressionParser, QueryParams};

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

const DEFAULT_DB_CONFIG: &str =
    "user=stufftail password=stufftail-password host=localhost port=5432 dbname=log";

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Database connect config
    ///
    /// see https://docs.rs/postgres/0.19.2/postgres/config/struct.Config.html for options
    #[arg(short, long = "database", value_name = "CONFIG", default_value = DEFAULT_DB_CONFIG)]
    db_connection: String,

    /// Maximum age of printed entries (postgres interval)
    #[arg(short, long, value_name = "AGE", default_value = "1 hour")]
    max_age: String,

    /// Maximum number of lines to print for each poll
    #[arg(short = 'l', long, value_name = "NUMBER", default_value_t = 1000)]
    max_lines: i64,

    /// Poll interval given in milliseconds
    #[arg(
        short = 'i',
        long = "poll-interval",
        value_name = "MSEC",
        default_value_t = 500
    )]
    poll_interval_ms: u64,

    /// logstuff query string
    #[arg(short, long)]
    query: Option<String>,

    /// Print field name in output
    #[arg(short, long, value_name = "NAME")]
    field: Vec<String>,

    /// CA certificate (bundle) to verify server's cert
    #[arg(short, long, value_name = "FILE")]
    ca_cert: Vec<String>,
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
    tls: TlsSettings,
}

impl Settings {
    fn from_cli_args() -> Self {
        let matches = Args::parse();
        let (query_expr, query_params) = match matches.query {
            Some(query) => {
                let parser = ExpressionParser::default();
                parser.to_sql(&query, 1).unwrap()
            }
            None => ("1 = 1".to_string(), Vec::new()),
        };

        let fields = if matches.field.is_empty() {
            vec![
                "hostname".to_string(),
                "syslogtag".to_string(),
                "msg".to_string(),
            ]
        } else {
            matches.field
        };

        let mut tls = TlsSettings::default();
        if !matches.ca_cert.is_empty() {
            tls.ca_certs = matches.ca_cert.to_vec();
        }

        Self {
            max_age: matches.max_age,
            max_lines: matches.max_lines,
            poll_interval_ms: matches.poll_interval_ms,
            query_expr,
            query_params,
            fields,
            db_config: matches.db_connection,
            tls,
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
    env_logger::init();
    let settings = Settings::from_cli_args();
    let connector = MakeTlsConnector::new(settings.tls.connector().unwrap());
    let mut client = postgres::Client::connect(&settings.db_config, connector).unwrap();

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
        thread::sleep(std::time::Duration::from_millis(settings.poll_interval_ms));
    }
}

fn print_event(event: Event, settings: &Settings) {
    let timeformat = format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
    println!(
        "{} {}",
        event.timestamp.format(&timeformat).unwrap(),
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
