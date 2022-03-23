use crate::utils::Iso8601Utc;
use color_eyre::Result;
use futures::StreamExt;
use gpio_cdev::{AsyncLineEventHandle, Chip, EventRequestFlags, Line, LineRequestFlags};
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use std::fmt::{Debug, Display, Formatter};
use std::time::SystemTime;

pub struct Feeder {
    name: String,
    count: u32,
    gpio_line: Line,
    pub event_handle: AsyncLineEventHandle,
}

#[derive(Debug)]
pub enum Error {
    NoMoreSupply,
}

#[derive(Debug, Serialize)]
pub enum Event {
    MaterialPickedUp,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoMoreSupply => write!(f, "Error: There are no more supply in the feeder"),
        }
    }
}

impl std::error::Error for Error {}

impl Serialize for Feeder {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_struct("feeder", 3)?;
        s.serialize_field("name", &self.name)?;
        s.serialize_field("count", &self.count)?;

        let now = SystemTime::iso8601_now();
        s.serialize_field("updateTimestamp", &now)?;
        s.end()
    }
}

impl Feeder {
    pub fn new<S>(name: S, count: u32, chip: &mut Chip, line: u32) -> Result<Self>
    where
        S: Into<String> + Display,
    {
        let line = chip.get_line(line)?;
        let event_handle = line.async_events(
            LineRequestFlags::INPUT,
            EventRequestFlags::BOTH_EDGES,
            &format!("{name} consumer"),
        )?;

        Ok(Self {
            name: name.into(),
            count,
            gpio_line: line,
            event_handle,
        })
    }

    pub async fn async_next_event(self: &mut Self) -> Result<Event, Error> {
        if self.count == 0 {
            return Err(Error::NoMoreSupply);
        }

        if let Some(_event) = self.event_handle.next().await {
            self.count -= 1;
        }

        Ok(Event::MaterialPickedUp)
    }

    /// Returns true if the material has no materials left at the current moment
    ///
    /// # Note
    /// This relies on the current event stream having nothing, meaning if you await now,
    /// you should block. Currently I don't know ensure this since the stream doesn't provide
    /// a non blocking way to see if it will block to read the next one
    pub fn is_empty(&self) -> bool {
        // if unwrap fails, then that means we have some how lost connection to the line, we can't
        // recover
        let request = self.event_handle.as_ref();

        // similar rationale for unwrap above
        request.get_value().unwrap() == 1
    }

    pub fn add_new_material(&mut self, new_material_count: u32) {
        self.count += new_material_count;
    }
}

#[cfg(test)]
mod test {
    use crate::manufacturing_components::feeder::Feeder;
    use gpio_cdev::Chip;

    #[test]
    fn feeder_to_json() {
        let mut chip = Chip::new("/dev/gpiochip0")
            .expect("Sorry the current hack requires access to /dev/gpiochip0");
        let feeder = Feeder::new("material feeder", 5, &mut chip, 0).unwrap();

        let json = serde_json::to_string(&feeder).unwrap();
        println!("{json}")
    }
}
