pub trait InputPin {
    fn is_high(&self) -> bool;
}

impl InputPin for esp_hal::gpio::Input<'_> {
    fn is_high(&self) -> bool {
        esp_hal::gpio::Input::is_high(self)
    }
}
