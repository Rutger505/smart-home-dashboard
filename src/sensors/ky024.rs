use super::input_pin::InputPin;

pub struct Ky024<P> {
    pin: P,
}

impl<P: InputPin> Ky024<P> {
    pub fn new(pin: P) -> Self {
        Self { pin }
    }

    pub fn detects_magnet(&self) -> bool {
        self.pin.is_high()
    }
}
