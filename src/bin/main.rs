#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;
use esp_backtrace as _;
use esp_hal::delay::Delay; // 1. Use the proper delay utility
use esp_hal::main;
use esp_hal::rng::Rng;
use esp_hal::time::{Duration, Instant};
use esp_hal::uart::{Config, Uart};
use esp_println::println;
use smart_home_dashboard::nextion::Screen;

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 98768);

    let delay = Delay::new();
    let rng = Rng::new();

    // with_rx enables the internal pull-up on the RX pin
    let rx_pin = peripherals.GPIO16;
    let tx_pin = peripherals.GPIO17;

    let display_uart = Uart::new(peripherals.UART2, Config::default().with_baudrate(9600))
        .expect("Could not initialize UART for display")
        .with_tx(tx_pin)
        .with_rx(rx_pin);
    let mut display = Screen::new(display_uart);

    let mut last_tx_time = Instant::now();
    loop {
        display.process();

        if last_tx_time.elapsed() >= Duration::from_millis(300) {
            display.update(rng.random() as u8);

            last_tx_time = Instant::now()
        }
    }
}
