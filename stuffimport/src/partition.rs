use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Weekday};
use serde::Deserialize;
use std::{error, fmt};

use logstuff::event::Event;

#[derive(Debug)]
pub enum Error {
    Postgres(postgres::Error),
    NoPartition(String),
}

impl error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::Error::*;
        match self {
            Postgres(e) => write!(f, "Database connection error: {}", e),
            NoPartition(e) => write!(f, "No parition: {}", e),
        }
    }
}

#[typetag::serde(tag = "kind")]
pub trait Partitioner: std::fmt::Debug {
    fn table_name(&self, event: &Event) -> Result<String, Error>;
    fn partition_by(&self) -> String;
    fn bounds(&self, event: &Event) -> String;
    fn schema(&self) -> &str {
        unimplemented!()
    }
}

impl From<postgres::Error> for Error {
    fn from(error: postgres::Error) -> Self {
        Error::Postgres(error)
    }
}

/// root table, usually "logs"
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct Root {
    pub table: String,
    pub schema: String,
}

impl Default for Root {
    fn default() -> Self {
        Self {
            table: "logs".into(),
            schema: format!(
                "({})",
                [
                    "id integer not null default nextval('logs_id'),",
                    "tstamp timestamp with time zone not null,",
                    "doc jsonb not null,",
                    "search tsvector",
                ]
                .join(" ")
            ),
        }
    }
}

#[typetag::serde(name = "root")]
impl Partitioner for Root {
    fn table_name(&self, _event: &Event) -> Result<String, Error> {
        Ok(self.table.to_string())
    }

    fn partition_by(&self) -> String {
        unreachable!()
    }

    fn bounds(&self, _event: &Event) -> String {
        unreachable!()
    }

    fn schema(&self) -> &str {
        &self.schema
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub enum TimeTruncate {
    Year,
    Quarter,
    Month,
    Week,
    Day,
    Hour,
    Minute,
}

impl TimeTruncate {
    pub fn lower_bound(&self, timestamp: &NaiveDateTime) -> NaiveDateTime {
        let date = match self {
            Self::Year => NaiveDate::from_ymd(timestamp.year(), 1, 1),
            Self::Quarter => {
                let month = match timestamp.month() {
                    1 | 2 | 3 => 1,
                    4 | 5 | 6 => 4,
                    7 | 8 | 9 => 7,
                    10 | 11 | 12 => 10,
                    _ => unreachable!(),
                };
                NaiveDate::from_ymd(timestamp.year(), month, 1)
            }
            Self::Month => timestamp.date().with_day(1).unwrap(),
            Self::Week => {
                let week = timestamp.iso_week();
                NaiveDate::from_isoywd(week.year(), week.week(), Weekday::Mon)
            }
            _ => timestamp.date(),
        };

        let time = match self {
            Self::Hour => NaiveTime::from_hms(timestamp.hour(), 0, 0),
            Self::Minute => timestamp.time().with_second(0).unwrap(),
            _ => NaiveTime::from_hms(0, 0, 0),
        };

        date.and_time(time)
    }

    pub fn upper_bound(&self, timestamp: &NaiveDateTime) -> NaiveDateTime {
        let next = match self {
            Self::Year => timestamp.with_year(timestamp.year() + 1).unwrap(),
            Self::Quarter => {
                let mut year = timestamp.year();
                let month = match timestamp.month() {
                    1 | 2 | 3 => 4,
                    4 | 5 | 6 => 7,
                    7 | 8 | 9 => 10,
                    10 | 11 | 12 => {
                        year += 1;
                        1
                    }
                    _ => unreachable!(),
                };
                NaiveDate::from_ymd(year, month, timestamp.day()).and_time(timestamp.time())
            }
            Self::Month => {
                let mut year = timestamp.year();
                let month = match timestamp.month() {
                    12 => {
                        year += 1;
                        1
                    }
                    month => month + 1,
                };
                NaiveDate::from_ymd(year, month, timestamp.day()).and_time(timestamp.time())
            }
            Self::Week => *timestamp + Duration::weeks(1),
            Self::Day => *timestamp + Duration::days(1),
            Self::Hour => *timestamp + Duration::hours(1),
            Self::Minute => *timestamp + Duration::minutes(1),
        };

        self.lower_bound(&next)
    }
}

/// partition parent table by time ranges
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct Timerange {
    pub name_template: String,
    pub interval: TimeTruncate,
}

impl Default for Timerange {
    fn default() -> Self {
        Self {
            name_template: "logs_%Y_%m".into(),
            interval: TimeTruncate::Month,
        }
    }
}

#[typetag::serde(name = "timerange")]
impl Partitioner for Timerange {
    fn table_name(&self, event: &Event) -> Result<String, Error> {
        Ok(event.timestamp.format(&self.name_template).to_string())
    }

    fn partition_by(&self) -> String {
        "range (tstamp)".into()
    }

    fn bounds(&self, event: &Event) -> String {
        let from = self.interval.lower_bound(&event.timestamp.naive_local());
        let to = self.interval.upper_bound(&event.timestamp.naive_local());
        format!(
            "from ('{}') to ('{}')",
            from.format("%Y-%m-%d"),
            to.format("%Y-%m-%d")
        )
    }
}

fn single_create_statement(
    event: &Event,
    parent: Option<&dyn Partitioner>,
    this: &dyn Partitioner,
    child: Option<&dyn Partitioner>,
) -> Result<String, Error> {
    let parent_stmt = match parent {
        Some(part) => format!(
            "partition of {} for values {}",
            part.table_name(event)?,
            this.bounds(event)
        ),
        None => this.schema().to_string(),
    };
    let child_stmt = match child {
        Some(part) => format!("partition by {}", part.partition_by()),
        None => "".to_string(),
    };
    Ok(format!(
        "create table if not exists {} {} {}",
        this.table_name(event)?,
        parent_stmt,
        child_stmt
    ))
}

pub fn create_tables(
    client: &mut impl postgres::GenericClient,
    event: &Event,
    parts: &[&dyn Partitioner],
) -> Result<(), Error> {
    parts
        .iter()
        .enumerate()
        .try_for_each(|(index, part)| -> Result<(), Error> {
            let parent = match index {
                0 => None,
                i => Some(parts[i - 1]),
            };
            let child = if index == parts.len() - 1 {
                None
            } else {
                Some(parts[index + 1])
            };
            client.execute(
                single_create_statement(event, parent, *part, child)?.as_str(),
                &[],
            )?;

            // TODO configurable owner
            client.execute(
                format!(
                    "alter table {} owner to write_logs",
                    part.table_name(event)?
                )
                .as_str(),
                &[],
            )?;
            Ok(())
        })?;
    Ok(())
}
