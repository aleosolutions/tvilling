use crate::utils::Iso8601Utc;
use color_eyre::Result;
use gpio_cdev::{AsyncLineEventHandle, Chip, EventRequestFlags, Line, LineRequestFlags};
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use std::fmt::Display;
use std::time::{Duration, SystemTime};

#[derive(Serialize)]
enum PistonStates {
    /// Piston is raised and await for commands, serialized to steady
    #[serde(rename = "steady")]
    Steady,
    /// Piston is at its bottom, serialized to depressed
    #[serde(rename = "depressed")]
    Depressed,
}

impl Default for PistonStates {
    fn default() -> Self {
        Self::Steady
    }
}

pub struct Piston {
    name: String,
    state: PistonStates,
    gpio_line: Line,
    pub event_handle: AsyncLineEventHandle,
}

impl Serialize for Piston {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_struct("piston", 3)?;
        s.serialize_field("name", &self.name)?;
        s.serialize_field("state", &self.state)?;

        let now = SystemTime::iso8601_now();
        s.serialize_field("updateTimestamp", &now)?;

        s.end()
    }
}

trait PistonActions {
    fn depress(self: &mut Self);
    fn steady(self: &mut Self);
    fn depress_for(duration: Duration);
}

impl Piston {
    pub fn new<S>(name: S, chip: &mut Chip, line: u32) -> Result<Self>
    where
        S: Into<String> + Display,
    {
        let line = chip.get_line(line)?;
        let event_handle = line.async_events(
            LineRequestFlags::INPUT,
            EventRequestFlags::RISING_EDGE,
            &format!("{name} consumer"),
        )?;

        Ok(Self {
            name: name.into(),
            state: PistonStates::default(),
            gpio_line: line,
            event_handle,
        })
    }
}

#[cfg(test)]
mod test {
    use crate::manufacturing_components::piston::Piston;
    use gpio_cdev::Chip;

    #[test]
    fn piston_to_json() {
        let mut chip = Chip::new("/dev/gpiochip0")
            .expect("Sorry the current hack requires access to /dev/gpiochip0");
        let piston = Piston::new("piston 1", &mut chip, 0).unwrap();
        let json = serde_json::to_string(&piston).unwrap();
        println!("{json}");
    }
}
