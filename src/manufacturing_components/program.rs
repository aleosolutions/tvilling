use gpio_cdev::LineRequestFlags;

/// A manufacturing program that can be started and stopped, the semantics of whether calling start
/// and stop multiple times and potentially interleaving is left undefined  
pub trait ManufacturingProgram {
    type Error;
    type Success;
    fn start(&mut self) -> Result<Self::Success, Self::Error>;
    fn stop(&mut self) -> Result<Self::Success, Self::Error>;
}

pub struct SimplifiedScenario2 {
    line: gpio_cdev::Line,
    line_handle: gpio_cdev::LineHandle,
}

impl SimplifiedScenario2 {
    pub fn new(chip: &mut gpio_cdev::Chip, line_num: u32) -> Result<Self, gpio_cdev::Error> {
        let line = chip.get_line(line_num)?;
        let line_handle =
            line.request(LineRequestFlags::OUTPUT, 0, "Simplified Scenario 2 program")?;

        Ok(Self { line, line_handle })
    }
}

impl ManufacturingProgram for SimplifiedScenario2 {
    type Error = gpio_cdev::Error;
    type Success = ();

    fn start(&mut self) -> Result<Self::Success, Self::Error> {
        self.line_handle.set_value(1)
    }

    fn stop(&mut self) -> Result<Self::Success, Self::Error> {
        self.line_handle.set_value(0)
    }
}
