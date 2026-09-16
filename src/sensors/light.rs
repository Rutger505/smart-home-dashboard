use esp_hal::gpio::Input;

pub struct Light<'d> {
    pin: Input<'d>,
}

impl<'d> Light<'d> {
    pub fn new(pin: Input<'d>) -> Self {
        Self { pin }
    }

    pub fn is_on(&self) -> bool {
        self.pin.is_high()
    }
}
