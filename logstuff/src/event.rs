use serde_json::{json, Map, Value};
use std::fmt;
use time::{macros::format_description, OffsetDateTime};

use crate::serde::de::rfc3339;

#[derive(PartialEq, Debug)]
#[repr(u8)]
pub enum SyslogSeverity {
    Emergency = 0,
    Alert = 1,
    Critical = 2,
    Error = 3,
    Warning = 4,
    Notice = 5,
    Info = 6,
    Debug = 7,
}

impl fmt::Display for SyslogSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use SyslogSeverity::*;
        write!(
            f,
            "{}",
            match self {
                Emergency => "emergency",
                Alert => "alert",
                Critical => "critical",
                Error => "error",
                Warning => "warning",
                Notice => "notice",
                Info => "info",
                Debug => "debug",
            }
        )
    }
}

#[derive(PartialEq, Debug)]
#[repr(u8)]
pub enum SyslogFacility {
    Kern = 0,
    User = 1,
    Mail = 2,
    Daemon = 3,
    Auth = 4,
    Syslog = 5,
    Lpr = 6,
    News = 7,
    Uucp = 8,
    Cron = 9,
    Authpriv = 10,
    Ftp = 11,
    Ntp = 12,
    Security = 13,
    Console = 14,
    SolarisCron = 15,
    Local0 = 16,
    Local1 = 17,
    Local2 = 18,
    Local3 = 19,
    Local4 = 20,
    Local5 = 21,
    Local6 = 22,
    Local7 = 23,
}

impl fmt::Display for SyslogFacility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use SyslogFacility::*;
        write!(
            f,
            "{}",
            match self {
                Kern => "kern",
                User => "user",
                Mail => "mail",
                Daemon => "daemon",
                Auth => "auth",
                Syslog => "syslog",
                Lpr => "lpr",
                News => "news",
                Uucp => "uucp",
                Cron => "cron",
                Authpriv => "authpriv",
                Ftp => "ftp",
                Ntp => "ntp",
                Security => "security",
                Console => "console",
                SolarisCron => "solariscron",
                Local0 => "local0",
                Local1 => "local1",
                Local2 => "local2",
                Local3 => "local3",
                Local4 => "local4",
                Local5 => "local5",
                Local6 => "local6",
                Local7 => "local7",
            }
        )
    }
}

mod severity_serde {
    use super::*;
    use serde::{de::Error, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(d: D) -> Result<SyslogSeverity, D::Error>
    where
        D: Deserializer<'de>,
    {
        use SyslogSeverity::*;
        let value = String::deserialize(d)?;
        match value.as_ref() {
            "0" => Ok(Emergency),
            "1" => Ok(Alert),
            "2" => Ok(Critical),
            "3" => Ok(Error),
            "4" => Ok(Warning),
            "5" => Ok(Notice),
            "6" => Ok(Info),
            "7" => Ok(Debug),
            o => Err(D::Error::custom(format_args!("Invalid value {}", o))),
        }
    }
}

mod facility_serde {
    use super::*;
    use serde::{de::Error, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(d: D) -> Result<SyslogFacility, D::Error>
    where
        D: Deserializer<'de>,
    {
        use SyslogFacility::*;
        let value = String::deserialize(d)?;
        match value.as_ref() {
            "0" => Ok(Kern),
            "1" => Ok(User),
            "2" => Ok(Mail),
            "3" => Ok(Daemon),
            "4" => Ok(Auth),
            "5" => Ok(Syslog),
            "6" => Ok(Lpr),
            "7" => Ok(News),
            "8" => Ok(Uucp),
            "9" => Ok(Cron),
            "10" => Ok(Authpriv),
            "11" => Ok(Ftp),
            "12" => Ok(Ntp),
            "13" => Ok(Security),
            "14" => Ok(Console),
            "15" => Ok(SolarisCron),
            "16" => Ok(Local0),
            "17" => Ok(Local1),
            "18" => Ok(Local2),
            "19" => Ok(Local3),
            "20" => Ok(Local4),
            "21" => Ok(Local5),
            "22" => Ok(Local6),
            "23" => Ok(Local7),
            o => Err(D::Error::custom(format_args!("Invalid value {}", o))),
        }
    }
}

/// log event formatted by rsyslog's "jsonmesg" property
#[derive(serde_derive::Deserialize, Debug)]
pub struct RsyslogdEvent {
    /// log message string
    msg: String,

    /// complete raw syslog message
    rawmsg: String,

    /// report time of the device sending this message
    #[serde(deserialize_with = "rfc3339")]
    timereported: OffsetDateTime,

    /// time stamp when rsyslog generated this message object
    #[serde(deserialize_with = "rfc3339")]
    timegenerated: OffsetDateTime,

    /// host name from the message
    hostname: String,

    /// tag of this message
    syslogtag: String,

    /// rsyslog input module which received this message
    inputname: String,

    /// host name of the sender that this message was received from (last hop before "our" rsyslog
    /// instance
    fromhost: String,

    /// IP address of "fromhost"
    #[serde(rename = "fromhost-ip")]
    fromhost_ip: String,

    /// raw "PRI" of this message
    pri: String, // TODO: this is an int

    /// numerical severity of the message
    #[serde(with = "severity_serde")]
    syslogseverity: SyslogSeverity,

    /// numerical facility of the message
    #[serde(with = "facility_serde")]
    syslogfacility: SyslogFacility,

    /// part of the tag before the optional pid
    programname: String,

    /// syslog "PROTOCOL-VERSION"
    #[serde(rename = "protocol-version")]
    protocol_version: String, // <-- TODO: parse::<u8>()

    /// syslog "STRUCTURED-DATA"
    #[serde(rename = "structured-data")]
    structured_data: String, // <-- TODO: Value?

    /// syslog "APP-NAME"
    #[serde(rename = "app-name")]
    app_name: String,

    /// syslog "PROCID"
    #[serde(skip_serializing_if = "Option::is_none")]
    procid: Option<String>,

    /// syslog "MSGID"
    #[serde(skip_serializing_if = "Option::is_none")]
    msgid: Option<String>,

    /// ???
    #[serde(skip_serializing_if = "Option::is_none")]
    uuid: Option<String>,

    /// rsyslog message variables
    #[serde(rename = "$!", skip_serializing_if = "Option::is_none")]
    message_variables: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct Event {
    pub timestamp: OffsetDateTime,
    pub doc: Value,
}

const FTS_FIELDS: &[&str] = &["hostname", "syslogtag", "msg"];

impl Event {
    pub fn search_string(&self) -> String {
        let mut parts = Vec::new();
        self.doc.as_object().unwrap().iter().for_each(|pair| {
            if FTS_FIELDS.contains(&&pair.0[..]) {
                parts.push(pair.1.to_string());
            } else if pair.0.starts_with("vars.") {
                parts.push(format!("{}={}", pair.0, pair.1));
            }
        });
        parts.join(" ")
    }

    pub fn get_printable(&self, index: &str) -> Option<String> {
        if let Some(value) = self.doc.get(index) {
            match value {
                Value::String(s) => Some(s.as_str().to_string()),
                Value::Array(_) => Some(flatten(value)),
                Value::Bool(true) => Some("true".to_string()),
                Value::Bool(false) => Some("false".to_string()),
                Value::Null => Some("null".to_string()),
                Value::Number(n) => Some(format!("{}", n)),
                Value::Object(_) => Some(flatten(value)),
            }
        } else {
            None
        }
    }
}

fn flatten(value: &Value) -> String {
    let mut unnested = Value::Object(Map::new());
    flatten_value(value, &mut unnested, "".to_string(), ".");
    unnested
        .as_object()
        .unwrap()
        .iter()
        .map(|pair| format!("{}={}", pair.0, pair.1))
        .collect::<Vec<String>>()
        .join(" ")
}

fn flatten_value(value: &Value, target: &mut Value, prefix: String, separator: &str) {
    match value {
        Value::Null => target[prefix] = Value::Null,
        Value::Object(map) => {
            map.iter().for_each(|pair| {
                let subprefix = if prefix.is_empty() {
                    pair.0.to_string()
                } else {
                    format!("{}{}{}", prefix, separator, pair.0)
                };
                flatten_value(pair.1, target, subprefix, separator);
            });
        }
        scalar => target[prefix] = scalar.to_owned(),
    };
}

impl From<RsyslogdEvent> for Event {
    fn from(event: RsyslogdEvent) -> Self {
        let mut doc = json!({
            "msg": event.msg,
            "timereported": event.timereported,
            "timegenerated": event.timegenerated,
            "hostname": event.hostname,
            "inputname": event.inputname,
            "syslogtag": event.syslogtag,
            "fromhost": event.fromhost,
            "fromhost_ip": event.fromhost_ip,
            "syslogfacility": event.syslogfacility.to_string(),
            "syslogseverity": event.syslogseverity.to_string(),
            "programname": event.programname,
            "procid": event.procid,
            "protocol_version": event.protocol_version,
            "app_name": event.app_name,
        });
        // Some field were left out do reduce duplication:
        // * rawmsg
        // * pri
        // * structured_data
        if let Some(vars) = event.message_variables {
            flatten_value(&vars, &mut doc, "vars".to_string(), ".");
        }
        if let Some(msgid) = event.msgid {
            doc["msgid"] = msgid.into();
        }
        if let Some(uuid) = event.uuid {
            doc["uuid"] = uuid.into();
        }

        Event {
            timestamp: event.timereported,
            doc,
        }
    }
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let timeformat = format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
        write!(f, "{}", self.timestamp.format(&timeformat).unwrap())?;
        if let Some(host) = self.get_printable("hostname") {
            write!(f, " {}", host)?;
        }
        if let Some(tag) = self.get_printable("syslogtag") {
            write!(f, " {}", tag)?;
        }
        if let Some(msg) = self.get_printable("msg") {
            write!(f, " {}", msg)?;
        }
        if let Some(vars) = self.get_printable("message_variables") {
            write!(f, " {}", vars)?;
        }
        Ok(())
    }
}
