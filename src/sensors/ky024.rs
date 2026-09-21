use defmt::trace;
use esp_hal::gpio::Input;

pub struct Ky024<'d> {
    pin: Input<'d>,
}

impl<'d> Ky024<'d> {
    pub fn new(pin: Input<'d>) -> Self {
        Self { pin }
    }

    pub fn detects_magnet(&self) -> bool {
        let magnet = self.pin.is_high();
        trace!("KY-024 detects magnet: {}", magnet);
        magnet
    }
}
