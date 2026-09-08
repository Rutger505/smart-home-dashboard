//! Driver for a Nextion HMI display connected over UART.
//!
//! Commands are ASCII text terminated by three `0xFF` bytes. Responses are
//! ignored unless `bkcmd` is raised; the boot sequence keeps it at 0 so the
//! receive FIFO never fills with unread acknowledgements. The one exception is
//! `connect`, which always answers and is used to find the display's current
//! baud rate.

use core::fmt::Write;

use esp_hal::Blocking;
use esp_hal::delay::Delay;
use esp_hal::time::{Duration, Instant};
use esp_hal::uart::{Config, ConfigError, Uart};
use esp_println::println;

const TERMINATOR: [u8; 3] = [0xFF, 0xFF, 0xFF];

/// Baud rate a factory-default display uses at power-on. A display whose
/// EEPROM was written with `bauds=` comes up at that rate instead, which is
/// why [`Nextion::detect_baud`] exists.
pub const BOOT_BAUD: u32 = 9600;

/// Baud rate both sides switch to once the link is up.
pub const RUN_BAUD: u32 = 115_200;

/// Every rate a Nextion display accepts, ordered so the common ones are tried
/// first.
pub const CANDIDATE_BAUDS: [u32; 13] = [
    9600, 115_200, 921_600, 57600, 38400, 19200, 230_400, 250_000, 256_000, 512_000, 4800, 2400,
    31250,
];

/// Milliseconds to wait for the display to finish its own boot before talking
/// to it. The ESP32 is ready well before the Nextion is.
const POWER_ON_DELAY_MS: u32 = 500;

/// Milliseconds of quiet on either side of a baud rate change, so the last
/// command drains and the display's UART restarts before the next byte.
const BAUD_SWITCH_SETTLE_MS: u32 = 50;

/// Milliseconds to wait for a `connect` reply while probing one baud rate.
const CONNECT_REPLY_TIMEOUT_MS: u64 = 120;

#[derive(Debug)]
pub enum Error {
    Uart,
    Config(ConfigError),
    /// A command did not fit in [`CommandBuffer`].
    CommandTooLong,
}

impl From<ConfigError> for Error {
    fn from(err: ConfigError) -> Self {
        Error::Config(err)
    }
}

/// Fixed-size scratch space for formatting one command.
struct CommandBuffer {
    bytes: [u8; 128],
    len: usize,
}

impl CommandBuffer {
    const fn new() -> Self {
        Self {
            bytes: [0; 128],
            len: 0,
        }
    }

    fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

impl Write for CommandBuffer {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let end = self.len + s.len();
        if end > self.bytes.len() {
            return Err(core::fmt::Error);
        }
        self.bytes[self.len..end].copy_from_slice(s.as_bytes());
        self.len = end;
        Ok(())
    }
}

pub struct Nextion<'d> {
    uart: Uart<'d, Blocking>,
    delay: Delay,
    logging: bool,
}

impl<'d> Nextion<'d> {
    pub fn new(uart: Uart<'d, Blocking>) -> Self {
        Self {
            uart,
            delay: Delay::new(),
            logging: true,
        }
    }

    /// Turns the per-command log lines on or off. On by default; turn it off
    /// once the dashboard starts pushing sensor updates every second.
    pub fn set_logging(&mut self, logging: bool) {
        self.logging = logging;
    }

    /// Brings the display to a known state: awake, backlit, on page 0, running
    /// at [`RUN_BAUD`].
    ///
    /// Returns the baud rate the display answered on, or `None` if it never
    /// answered. A display that is wired TX-only cannot answer, so a `None`
    /// here is not fatal and the boot sequence runs anyway at [`BOOT_BAUD`].
    pub fn configure(&mut self, brightness: u8) -> Result<Option<u32>, Error> {
        self.log(format_args!(
            "waiting {POWER_ON_DELAY_MS} ms for display boot"
        ));
        self.delay.delay_millis(POWER_ON_DELAY_MS);

        let detected = self.detect_baud()?;
        match detected {
            Some(baud) => self.log(format_args!("display answered at {baud} baud")),
            None => {
                self.log(format_args!(
                    "no reply on any rate, assuming {BOOT_BAUD} baud"
                ));
                self.apply_baud(BOOT_BAUD)?;
            }
        }

        // Flush any half-received command left over from a warm reset.
        self.write_raw(&TERMINATOR)?;

        self.send("bkcmd=0")?;
        self.set_baud(RUN_BAUD)?;
        self.send_fmt(format_args!("dim={}", brightness.min(100)))?;
        self.send("thup=1")?;
        self.send("page 0")?;
        self.log(format_args!("ready at {RUN_BAUD} baud"));
        Ok(detected)
    }

    /// Sends `connect` at each rate in [`CANDIDATE_BAUDS`] until the display
    /// replies, leaving the UART open at the rate that worked.
    pub fn detect_baud(&mut self) -> Result<Option<u32>, Error> {
        for baud in CANDIDATE_BAUDS {
            self.log(format_args!("probing {baud} baud"));
            self.apply_baud(baud)?;
            self.drain();
            self.write_raw(&TERMINATOR)?;
            self.write_command(b"connect")?;
            self.uart.flush().map_err(|_| Error::Uart)?;
            if self.wait_for(b"comok", CONNECT_REPLY_TIMEOUT_MS) {
                return Ok(Some(baud));
            }
        }
        Ok(None)
    }

    /// Switches the display and then this side to `baud`. The setting is not
    /// stored in the display's EEPROM, so it reverts on the next power cycle.
    /// Use [`Nextion::store_baud`] to make it stick.
    pub fn set_baud(&mut self, baud: u32) -> Result<(), Error> {
        self.send_fmt(format_args!("baud={baud}"))?;
        self.uart.flush().map_err(|_| Error::Uart)?;
        self.delay.delay_millis(BAUD_SWITCH_SETTLE_MS);
        self.apply_baud(baud)?;
        self.delay.delay_millis(BAUD_SWITCH_SETTLE_MS);
        Ok(())
    }

    /// Same as [`Nextion::set_baud`], but writes the rate to the display's
    /// EEPROM so it powers on there. Costs an EEPROM write cycle, so call it
    /// once by hand, not on every boot.
    pub fn store_baud(&mut self, baud: u32) -> Result<(), Error> {
        self.send_fmt(format_args!("bauds={baud}"))?;
        self.uart.flush().map_err(|_| Error::Uart)?;
        self.delay.delay_millis(BAUD_SWITCH_SETTLE_MS);
        self.apply_baud(baud)?;
        self.delay.delay_millis(BAUD_SWITCH_SETTLE_MS);
        Ok(())
    }

    pub fn page(&mut self, page: u8) -> Result<(), Error> {
        self.send_fmt(format_args!("page {page}"))
    }

    /// Sets the backlight, 0-100.
    pub fn brightness(&mut self, level: u8) -> Result<(), Error> {
        self.send_fmt(format_args!("dim={}", level.min(100)))
    }

    pub fn set_text(&mut self, object: &str, value: &str) -> Result<(), Error> {
        self.send_fmt(format_args!("{object}.txt=\"{value}\""))
    }

    pub fn set_value(&mut self, object: &str, value: i32) -> Result<(), Error> {
        self.send_fmt(format_args!("{object}.val={value}"))
    }

    /// Sets a 16-bit RGB565 colour attribute, such as `pco` or `bco`.
    pub fn set_color(&mut self, object: &str, attribute: &str, color: u16) -> Result<(), Error> {
        self.send_fmt(format_args!("{object}.{attribute}={color}"))
    }

    pub fn send(&mut self, command: &str) -> Result<(), Error> {
        self.write_command(command.as_bytes())
    }

    fn send_fmt(&mut self, args: core::fmt::Arguments<'_>) -> Result<(), Error> {
        let mut buffer = CommandBuffer::new();
        buffer.write_fmt(args).map_err(|_| Error::CommandTooLong)?;
        self.write_command(buffer.as_slice())
    }

    fn write_command(&mut self, command: &[u8]) -> Result<(), Error> {
        if self.logging {
            match core::str::from_utf8(command) {
                Ok(text) => println!("nextion: -> {text}"),
                Err(_) => println!("nextion: -> {} non-utf8 bytes", command.len()),
            }
        }
        let result = self
            .write_raw(command)
            .and_then(|()| self.write_raw(&TERMINATOR));
        if result.is_err() && self.logging {
            println!("nextion: write failed");
        }
        result
    }

    fn write_raw(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let mut written = 0;
        while written < bytes.len() {
            written += self
                .uart
                .write(&bytes[written..])
                .map_err(|_| Error::Uart)?;
        }
        Ok(())
    }

    fn apply_baud(&mut self, baud: u32) -> Result<(), Error> {
        self.uart
            .apply_config(&Config::default().with_baudrate(baud))?;
        Ok(())
    }

    /// Throws away whatever the display has already sent, so a probe only sees
    /// the reply to its own `connect`.
    fn drain(&mut self) {
        let mut scratch = [0u8; 64];
        while matches!(self.uart.read_buffered(&mut scratch), Ok(n) if n > 0) {}
    }

    /// Reads until `needle` shows up or `timeout_ms` passes.
    fn wait_for(&mut self, needle: &[u8], timeout_ms: u64) -> bool {
        let mut window = [0u8; 64];
        let mut len = 0usize;
        let deadline = Duration::from_millis(timeout_ms);
        let start = Instant::now();

        while start.elapsed() < deadline {
            let mut chunk = [0u8; 32];
            let read = self.uart.read_buffered(&mut chunk).unwrap_or(0);
            for &byte in &chunk[..read] {
                if len == window.len() {
                    window.copy_within(1.., 0);
                    len -= 1;
                }
                window[len] = byte;
                len += 1;
            }
            if window[..len].windows(needle.len()).any(|w| w == needle) {
                return true;
            }
        }
        false
    }

    fn log(&self, args: core::fmt::Arguments<'_>) {
        if self.logging {
            println!("nextion: {args}");
        }
    }
}
