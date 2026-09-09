#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_hal::main;
use esp_hal::time::{Duration, Instant};
use esp_hal::uart::{Config, Uart};

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    esp_println::println!("handmade: start");

    let _uart = Uart::new(peripherals.UART2, Config::default().with_baudrate(9600))
        .expect("UART2 config rejected")
        .with_rx(peripherals.GPIO16)
        .with_tx(peripherals.GPIO17);

    loop {
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(500) {}
    }
}
