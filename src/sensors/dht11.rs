use dht_sensor::{DhtError, dht11};
use embedded_hal::delay::DelayNs;
use embedded_hal::digital::{InputPin, OutputPin};
use log::trace;

use super::reading::Reading;

pub struct Dht11<P> {
    pin: P,
}

impl<P: InputPin + OutputPin> Dht11<P> {
    pub fn new(pin: P) -> Self {
        Self { pin }
    }

    pub fn read(&mut self, delay: &mut impl DelayNs) -> Result<Reading, DhtError<P::Error>> {
        trace!("DHT11 read start");
        let reading = dht11::blocking::read(delay, &mut self.pin).map(Reading::from);
        trace!("DHT11 read done, ok: {}", reading.is_ok());
        reading
    }
}
