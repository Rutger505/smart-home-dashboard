#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_hal::delay::Delay;
use esp_hal::main;
use esp_hal::time::{Duration, Instant};
use esp_hal::uart::{Config, Uart};
use esp_println::println;
use screen::nextion::{BOOT_BAUD, Nextion, color};

extern crate alloc;

esp_bootloader_esp_idf::esp_app_desc!();

/// The console TX FIFO overruns and scrambles anything printed faster than it
/// drains, so results go out one line at a time with a gap between them.
fn report(delay: &Delay, label: &str, value: usize) {
    println!("RESULT {label}={value}");
    delay.delay_millis(120);
}

/// Checks the link end to end: find the display, write state, read it back,
/// then draw. Reading back is the only honest test, because this display
/// acknowledges nothing even with `bkcmd=3`.
#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 98768);
    let delay = Delay::new();

    let uart = Uart::new(
        peripherals.UART2,
        Config::default().with_baudrate(BOOT_BAUD),
    )
    .expect("UART2 config rejected")
    .with_rx(peripherals.GPIO16)
    .with_tx(peripherals.GPIO17);

    let mut screen = Nextion::new(uart);
    screen.set_logging(false);

    match screen.configure(100) {
        Ok(Some(baud)) => report(&delay, "baud", baud as usize),
        Ok(None) => report(&delay, "baud", 0),
        Err(_) => report(&delay, "configure_failed", 1),
    }

    screen.send("sendme").ok();
    report(&delay, "sendme_bytes", screen.dump_reply("page", 500));

    screen.set_text("t0", "PROOF").ok();
    delay.delay_millis(100);
    screen.send("get t0.txt").ok();
    report(&delay, "t0_bytes", screen.dump_reply("t0", 500));

    screen.set_value("n0", 4321).ok();
    delay.delay_millis(100);
    screen.send("get n0.val").ok();
    report(&delay, "n0_bytes", screen.dump_reply("n0", 500));

    screen.clear(color::BLACK).ok();
    screen.fill(10, 10, 380, 60, color::RED).ok();
    screen
        .draw_text(10, 90, 380, 60, 0, color::WHITE, color::BLUE, "SELF TEST")
        .ok();

    screen.send("connect").ok();
    report(&delay, "alive_after_drawing", screen.dump_reply("alive", 500));

    println!("RESULT done=1");
    loop {
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(500) {}
    }
}
