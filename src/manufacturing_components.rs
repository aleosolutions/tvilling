use std::time::Duration;
use gpio_cdev::{Line, Chip};
use color_eyre::Result;
use serde::{Serialize, Deserialize};

#[derive(Deserialize, Serialize)]
enum PistonStates {
    /// Piston is raised and await for commands
    Steady,
    /// Piston is at its bottom
    Depressed,
}

pub struct Piston {
    piston_state: PistonStates,
    gpio_line: Line,
}

#[derive(Serialize, Deserialize)]
enum TrackPositions {
    /// Track position when the arm is picking materials from feeder A
    Position1,
    /// Track position when the arm is placing materials to the piston
    Position15,
    /// Track position when the arm is picking materials from feeder B
    Position66,
}


trait PistonActions {
    fn depress(self: &mut Self);
    fn steady(self: &mut Self);
    fn depress_for(duration: Duration);
}

impl Piston {
    fn new(chip: &mut Chip, line_num: u32) -> Result<Self> {
        Ok(Self {
            piston_state: PistonStates::Steady,
            gpio_line: chip.get_line(line_num)?,
        })
    }
}
