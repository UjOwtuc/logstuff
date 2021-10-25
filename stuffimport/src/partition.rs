use serde::Deserialize;
use std::{error, fmt};
use time::error::{Format, InvalidFormatDescription};
use time::{
    format_description, Date, Duration, Month, OffsetDateTime, PrimitiveDateTime, Time, Weekday,
};

use logstuff::event::Event;

#[derive(Debug)]
pub enum Error {
    Postgres(postgres::Error),
    NoPartition(String),
    InvalidDateTimeFormat(InvalidFormatDescription),
    DateTimeFormat(Format),
}

impl error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::Error::*;
        match self {
            Postgres(e) => write!(f, "Database connection error: {}", e),
            NoPartition(e) => write!(f, "No parition: {}", e),
            InvalidDateTimeFormat(e) => write!(f, "Invalid date and time format: {}", e),
            DateTimeFormat(e) => write!(f, "Could not format time stamp: {}", e),
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

impl From<InvalidFormatDescription> for Error {
    fn from(error: InvalidFormatDescription) -> Self {
        Error::InvalidDateTimeFormat(error)
    }
}

impl From<Format> for Error {
    fn from(error: Format) -> Self {
        Error::DateTimeFormat(error)
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
    pub fn lower_bound(&self, timestamp: &OffsetDateTime) -> OffsetDateTime {
        let date = match self {
            Self::Year => Date::from_calendar_date(timestamp.year(), Month::January, 1).unwrap(),
            Self::Quarter => {
                let month = match timestamp.month() {
                    Month::January | Month::February | Month::March => Month::January,
                    Month::April | Month::May | Month::June => Month::April,
                    Month::July | Month::August | Month::September => Month::July,
                    Month::October | Month::November | Month::December => Month::October,
                };
                Date::from_calendar_date(timestamp.year(), month, 1).unwrap()
            }
            Self::Month => {
                Date::from_calendar_date(timestamp.year(), timestamp.month(), 1).unwrap()
            }
            Self::Week => {
                let week = timestamp.iso_week();
                Date::from_iso_week_date(timestamp.year(), week, Weekday::Monday).unwrap()
            }
            _ => timestamp.date(),
        };

        let time = match self {
            Self::Hour => Time::from_hms(timestamp.hour(), 0, 0).unwrap(),
            Self::Minute => Time::from_hms(timestamp.hour(), timestamp.minute(), 0).unwrap(),
            _ => Time::from_hms(0, 0, 0).unwrap(),
        };

        date.with_time(time).assume_utc()
    }

    pub fn upper_bound(&self, timestamp: &OffsetDateTime) -> OffsetDateTime {
        let next = match self {
            Self::Year => timestamp.replace_date(
                Date::from_calendar_date(timestamp.year() + 1, Month::January, 1).unwrap(),
            ),
            Self::Quarter => {
                let mut year = timestamp.year();
                let month = match timestamp.month() {
                    Month::January | Month::February | Month::March => Month::April,
                    Month::April | Month::May | Month::June => Month::July,
                    Month::July | Month::August | Month::September => Month::October,
                    Month::October | Month::November | Month::December => {
                        year += 1;
                        Month::January
                    }
                };
                PrimitiveDateTime::new(
                    Date::from_calendar_date(year, month, timestamp.day()).unwrap(),
                    timestamp.time(),
                )
                .assume_utc()
            }
            Self::Month => {
                let mut year = timestamp.year();
                let month = match timestamp.month() {
                    Month::December => {
                        year += 1;
                        Month::January
                    }
                    month => month.next(),
                };
                PrimitiveDateTime::new(
                    Date::from_calendar_date(year, month, timestamp.day()).unwrap(),
                    timestamp.time(),
                )
                .assume_utc()
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
        let format = format_description::parse(&self.name_template)?;
        Ok(event.timestamp.format(&format)?)
    }

    fn partition_by(&self) -> String {
        "range (tstamp)".into()
    }

    fn bounds(&self, event: &Event) -> String {
        let from = self.interval.lower_bound(&event.timestamp);
        let to = self.interval.upper_bound(&event.timestamp);
        let format = time::macros::format_description!("[year]-[month]-[day]");
        format!(
            "from ('{}') to ('{}')",
            from.format(&format).unwrap(),
            to.format(&format).unwrap()
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
