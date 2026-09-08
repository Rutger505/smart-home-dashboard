#![no_std]
#![no_main]
#![deny(clippy::large_stack_frames)]

use esp_backtrace as _;
use esp_hal::main;
use esp_hal::time::{Duration, Instant};

extern crate alloc;

esp_bootloader_esp_idf::esp_app_desc!();

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
    let _ = peripherals.GPIO16;

    let _ = peripherals.GPIO20;


    // Rx and TX2 on board need to be used for serial connection

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 98768);




    loop {
        let delay_start = Instant::now();
        while delay_start.elapsed() < Duration::from_millis(500) {}
    }

}
