use crate::manufacturing_components::robot::RobotPosition::{Position1, Position15, Position66};
use crate::utils::Iso8601Utc;
use color_eyre::Result;
use futures::StreamExt;
use gpio_cdev::{AsyncLineEventHandle, Chip, EventRequestFlags, Line, LineRequestFlags};
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use std::fmt::Display;
use std::time::SystemTime;

#[derive(Serialize)]
enum RobotPosition {
    /// Track position when the arm is picking materials from feeder A, serializes to position1
    #[serde(rename = "position 1")]
    Position1,
    /// Track position when the arm is placing materials to the piston, serializes to position15
    #[serde(rename = "position 15")]
    Position15,

    /// Track position when the arm is picking materials from feeder B, serializes to position66
    #[serde(rename = "position 66")]
    Position66,
}

impl Default for RobotPosition {
    fn default() -> Self {
        Self::Position1
    }
}

pub struct Robot {
    name: String,
    position: RobotPosition,
    gpio_line: Line,
    pub event_handle: AsyncLineEventHandle,
}

impl Robot {
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
            position: RobotPosition::default(),
            gpio_line: line,
            event_handle,
        })
    }

    async fn async_next_event(self: &mut Self) -> Result<()> {
        if let Some(_event) = self.event_handle.next().await {
            match self.position {
                RobotPosition::Position1 => self.position = Position15,
                RobotPosition::Position15 => self.position = Position66,
                RobotPosition::Position66 => self.position = Position1,
            }
        }

        Ok(())
    }
}

impl Serialize for Robot {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_struct("robot", 3)?;
        s.serialize_field("name", &self.name)?;
        s.serialize_field("position", &self.position)?;
        let now = SystemTime::iso8601_now();
        s.serialize_field("updateTimestamp", &now)?;
        s.end()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn robot_to_json() {
        let mut chip = Chip::new("/dev/gpiochip0")
            .expect("Sorry the current hack requires access to /dev/gpiochip0");
        let robot = Robot::new("robot 1", &mut chip, 0).unwrap();
        let json = serde_json::to_string(&robot).unwrap();
        println!("{json}")
    }
}
