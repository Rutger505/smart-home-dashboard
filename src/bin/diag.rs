#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_hal::delay::Delay;
use esp_hal::main;
use esp_hal::time::{Duration, Instant};
use esp_hal::uart::{Config, Uart};
use esp_println::println;
use smart_home_dashboard::nextion::{BOOT_BAUD, Nextion, color};

extern crate alloc;

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 98768);
    let delay = Delay::new();

    println!("diag: self test");
    let uart = Uart::new(
        peripherals.UART2,
        Config::default().with_baudrate(BOOT_BAUD),
    )
    .expect("UART2 config rejected")
    .with_rx(peripherals.GPIO16)
    .with_tx(peripherals.GPIO17);

    let mut screen = Nextion::new(uart);
    match screen.configure(100) {
        Ok(Some(baud)) => println!("diag: link up, display was at {baud}"),
        Ok(None) => println!("diag: no link"),
        Err(error) => println!("diag: configure failed: {error:?}"),
    }

    // Mutating state and reading it back is the honest test: if the display
    // executes commands at all, it executes drawing commands too.
    screen.send("dim=42").ok();
    delay.delay_millis(100);
    screen.send("get dim").ok();
    screen.dump_reply("dim after dim=42", 400);

    screen.set_text("t0", "PROOF").ok();
    delay.delay_millis(100);
    screen.send("get t0.txt").ok();
    screen.dump_reply("t0 after set", 400);

    screen.set_value("n0", 4321).ok();
    delay.delay_millis(100);
    screen.send("get n0.val").ok();
    screen.dump_reply("n0 after set", 400);

    screen.clear(color::BLACK).ok();
    screen.fill(10, 10, 380, 60, color::RED).ok();
    screen
        .draw_text(10, 90, 380, 60, 0, color::WHITE, color::BLUE, "SELF TEST")
        .ok();
    screen.dump_reply("drawing", 400);

    screen.send("dim=100").ok();
    delay.delay_millis(100);
    screen.send("get dim").ok();
    screen.dump_reply("dim restored", 400);

    screen.send("bkcmd=0").ok();
    println!("diag: done");
    loop {
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(500) {}
    }
}
