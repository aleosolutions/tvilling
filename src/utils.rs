use chrono::{DateTime, Utc};
pub use std::time::SystemTime;

pub trait Iso8601Utc {
    fn iso8601_now() -> String;
}

impl Iso8601Utc for SystemTime {
    fn iso8601_now() -> String {
        let now = SystemTime::now();
        let now: DateTime<Utc> = now.into();
        now.to_rfc3339()
    }
}
