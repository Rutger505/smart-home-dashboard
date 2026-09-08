#![no_std]
#![no_main]
#![deny(clippy::large_stack_frames)]

use esp_backtrace as _;
use esp_hal::main;
use esp_hal::time::{Duration, Instant};
use esp_hal::uart::{Config, Uart};
use smart_home_dashboard::nextion::{BOOT_BAUD, Nextion};

extern crate alloc;

esp_bootloader_esp_idf::esp_app_desc!();

/// Backlight level sent at boot, 0-100.
const BRIGHTNESS: u8 = 100;

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // The following pins are used to bootstrap the chip. They are available
    // for use, but check the datasheet of the module for more information on them.
    // - GPIO0
    // - GPIO2
    // - GPIO5
    // - GPIO12
    // - GPIO15
    // These GPIO pins are in use by some feature of the module and should not be used.
    let _ = peripherals.GPIO6;
    let _ = peripherals.GPIO7;
    let _ = peripherals.GPIO8;
    let _ = peripherals.GPIO9;
    let _ = peripherals.GPIO10;
    let _ = peripherals.GPIO11;

    let _ = peripherals.GPIO20;

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 98768);

    esp_println::println!("boot: opening UART2 at {BOOT_BAUD} baud");

    // D16/D17 on the DOIT DevKit V1 silkscreen, wired to the display's TX/RX.
    let uart = Uart::new(
        peripherals.UART2,
        Config::default().with_baudrate(BOOT_BAUD),
    )
    .expect("UART2 config rejected")
    .with_rx(peripherals.GPIO16)
    .with_tx(peripherals.GPIO17);

    let mut screen = Nextion::new(uart);
    match screen.configure(BRIGHTNESS) {
        Ok(Some(baud)) => esp_println::println!("boot: screen up, was talking at {baud} baud"),
        Ok(None) => esp_println::println!(
            "boot: screen never replied, commands sent blind. Check the display TX wire and that \
             its baud rate is one of the probed rates"
        ),
        Err(error) => esp_println::println!("boot: screen setup failed: {error:?}"),
    }

    loop {
        let delay_start = Instant::now();
        while delay_start.elapsed() < Duration::from_millis(500) {}
    }
}
