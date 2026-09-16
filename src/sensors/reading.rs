#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reading {
    pub temperature: i8,
    pub humidity: u8,
}

impl From<dht_sensor::dht11::Reading> for Reading {
    fn from(reading: dht_sensor::dht11::Reading) -> Self {
        Self {
            temperature: reading.temperature,
            humidity: reading.relative_humidity,
        }
    }
}
