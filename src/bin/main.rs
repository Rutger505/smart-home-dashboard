#![no_std]
#![no_main]
#![deny(clippy::large_stack_frames)]

use esp_backtrace as _;
use esp_hal::main;
use smart_home_dashboard::screen;

extern crate alloc;

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 98768);

    screen::run(peripherals)
}
