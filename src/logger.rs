use esp_hal::time::Instant;
use esp_println::println;
use log::{Level, LevelFilter, Log, Metadata, Record};

const RESET: &str = "\u{1B}[0m";

fn color(level: Level) -> &'static str {
    match level {
        Level::Error => "\u{1B}[31m",
        Level::Warn => "\u{1B}[33m",
        Level::Info => "\u{1B}[32m",
        Level::Debug => "\u{1B}[34m",
        Level::Trace => "\u{1B}[35m",
    }
}

struct Logger;

impl Log for Logger {
    fn enabled(&self, _: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        println!(
            "{}{:>8} {:<5} [{}] {}{}",
            color(record.level()),
            Instant::now().duration_since_epoch().as_millis(),
            record.level(),
            record.target(),
            record.args(),
            RESET
        );
    }

    fn flush(&self) {}
}

static LOGGER: Logger = Logger;

pub fn init(level: LevelFilter) {
    // The ESP32's Xtensa core has no atomic compare-and-swap, so only the racy setters exist.
    // Safe as long as this runs once, before any task is spawned.
    unsafe {
        log::set_logger_racy(&LOGGER).unwrap();
        log::set_max_level_racy(level);
    }
}
