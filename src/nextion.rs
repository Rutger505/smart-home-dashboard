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
/// The reply is around 70 bytes, so the slow rates need most of this.
const CONNECT_REPLY_TIMEOUT_MS: u64 = 350;

/// Milliseconds of quiet after switching this side's rate. Bytes sent at the
/// wrong rate land in the display's parser as garbage, and it needs to see a
/// terminator and go idle again before it will read the next probe.
const PROBE_SETTLE_MS: u32 = 60;

/// Milliseconds to leave the display alone after a probe it did not answer.
/// A probe sent at the wrong rate leaves the display deaf for a while:
/// measured at 300 ms typically and 4.3 s at worst on an NX4024K032. Probing
/// straight through a rate list without this wait makes the display miss the
/// one probe that was sent at its own rate.
const PROBE_RECOVERY_MS: u32 = 600;

/// Milliseconds a page change takes. Drawing commands sent inside this window
/// are dropped, and with `bkcmd=0` they are dropped silently.
const PAGE_LOAD_MS: u32 = 200;

/// How many times to work through a rate list before giving up.
const PROBE_PASSES: u8 = 3;

/// The two rates worth trying first: the rate this driver switches to, and the
/// rate a factory display powers on at. A full sweep only happens if both miss.
const COMMON_BAUDS: [u32; 2] = [RUN_BAUD, BOOT_BAUD];

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

    fn as_str(&self) -> &str {
        core::str::from_utf8(self.as_slice()).unwrap_or("<non-utf8>")
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
            None => self.log(format_args!(
                "no reply on any rate, assuming {BOOT_BAUD} baud"
            )),
        }

        self.send("bkcmd=0")?;

        if detected.is_some_and(|baud| baud != RUN_BAUD) {
            self.set_baud(RUN_BAUD)?;
            if self.confirm_link()? {
                self.log(format_args!("display followed to {RUN_BAUD} baud"));
            } else {
                let fallback = detected.unwrap_or(BOOT_BAUD);
                self.log(format_args!(
                    "display did not follow to {RUN_BAUD} baud, staying at {fallback}"
                ));
                self.apply_baud(fallback)?;
                self.flush_parser();
            }
        }

        self.send("sleep=0")?;
        self.send_fmt(format_args!("dim={}", brightness.min(100)))?;
        self.send("thup=1")?;
        self.page(0)?;
        Ok(detected)
    }

    /// Sends `connect` at each rate in [`CANDIDATE_BAUDS`] until the display
    /// replies, leaving the UART open at the rate that worked.
    pub fn detect_baud(&mut self) -> Result<Option<u32>, Error> {
        if let Some(baud) = self.sweep(&COMMON_BAUDS, PROBE_PASSES)? {
            return Ok(Some(baud));
        }
        if let Some(baud) = self.sweep(&CANDIDATE_BAUDS, 1)? {
            return Ok(Some(baud));
        }
        self.apply_baud(BOOT_BAUD)?;
        Ok(None)
    }

    /// Probes each rate in `bauds`, `passes` times over. Every miss is
    /// followed by [`PROBE_RECOVERY_MS`] of silence, because the miss itself
    /// was garbage to the display and it ignores what comes straight after.
    fn sweep(&mut self, bauds: &[u32], passes: u8) -> Result<Option<u32>, Error> {
        for pass in 0..passes {
            for &baud in bauds {
                self.log(format_args!("probing {baud} baud, pass {}", pass + 1));
                self.apply_baud(baud)?;
                self.flush_parser();
                if self.handshake()? {
                    return Ok(Some(baud));
                }
                self.delay.delay_millis(PROBE_RECOVERY_MS);
            }
        }
        Ok(None)
    }

    /// Sends `connect` and waits for the display to name itself. True means
    /// the display is listening at this side's current rate.
    pub fn handshake(&mut self) -> Result<bool, Error> {
        self.write_command(b"connect")?;
        self.uart.flush().map_err(|_| Error::Uart)?;
        let answered = self.wait_for(b"comok", CONNECT_REPLY_TIMEOUT_MS);
        self.drain();
        Ok(answered)
    }

    /// Handshakes up to [`PROBE_PASSES`] times, waiting out the deaf spell a
    /// failed attempt leaves behind.
    pub fn confirm_link(&mut self) -> Result<bool, Error> {
        for _ in 0..PROBE_PASSES {
            if self.handshake()? {
                return Ok(true);
            }
            self.delay.delay_millis(PROBE_RECOVERY_MS);
            self.flush_parser();
        }
        Ok(false)
    }

    /// Ends whatever half-received command the display is sitting on and
    /// throws away anything it sent back, so the next command is read alone.
    /// Needed after every rate change, because bytes sent at the wrong rate
    /// leave a partial command in the display's parser.
    pub fn flush_parser(&mut self) {
        let _ = self.write_raw(&TERMINATOR);
        let _ = self.uart.flush();
        self.delay.delay_millis(PROBE_SETTLE_MS);
        self.drain();
    }

    /// Switches the display and then this side to `baud`. The setting is not
    /// stored in the display's EEPROM, so it reverts on the next power cycle.
    /// Use [`Nextion::store_baud`] to make it stick.
    pub fn set_baud(&mut self, baud: u32) -> Result<(), Error> {
        self.send_fmt(format_args!("baud={baud}"))?;
        self.uart.flush().map_err(|_| Error::Uart)?;
        self.delay.delay_millis(BAUD_SWITCH_SETTLE_MS);
        self.apply_baud(baud)?;
        self.flush_parser();
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
        self.flush_parser();
        Ok(())
    }

    /// Switches pages and waits for the load to finish. Anything drawn before
    /// the new page is up is lost.
    pub fn page(&mut self, page: u8) -> Result<(), Error> {
        self.send_fmt(format_args!("page {page}"))?;
        self.delay.delay_millis(PAGE_LOAD_MS);
        self.drain();
        Ok(())
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

    /// Paints the whole screen `color`.
    pub fn clear(&mut self, color: u16) -> Result<(), Error> {
        self.send_fmt(format_args!("cls {color}"))
    }

    pub fn fill(&mut self, x: u16, y: u16, width: u16, height: u16, color: u16) -> Result<(), Error> {
        self.send_fmt(format_args!("fill {x},{y},{width},{height},{color}"))
    }

    pub fn line(&mut self, x1: u16, y1: u16, x2: u16, y2: u16, color: u16) -> Result<(), Error> {
        self.send_fmt(format_args!("line {x1},{y1},{x2},{y2},{color}"))
    }

    /// Draws `text` centred in a box, over a solid `background`. `font` is an
    /// index into the fonts compiled into the HMI file.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text(
        &mut self,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        font: u8,
        color: u16,
        background: u16,
        text: &str,
    ) -> Result<(), Error> {
        self.send_fmt(format_args!(
            "xstr {x},{y},{width},{height},{font},{color},{background},1,1,1,\"{text}\""
        ))
    }

    /// Same as [`Nextion::draw_text`] but left aligned, for values that change
    /// length and would otherwise jump around.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_left(
        &mut self,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        font: u8,
        color: u16,
        background: u16,
        text: &str,
    ) -> Result<(), Error> {
        self.send_fmt(format_args!(
            "xstr {x},{y},{width},{height},{font},{color},{background},0,1,1,\"{text}\""
        ))
    }

    /// Logs everything the display sends over the next `timeout_ms`, as hex
    /// plus printable ASCII. Returns the number of bytes seen.
    pub fn dump_reply(&mut self, label: &str, timeout_ms: u64) -> usize {
        let deadline = Duration::from_millis(timeout_ms);
        let start = Instant::now();
        let mut total = 0usize;
        let mut line = CommandBuffer::new();
        let mut chunk = [0u8; 32];

        while start.elapsed() < deadline {
            let read = self.uart.read_buffered(&mut chunk).unwrap_or(0);
            for &byte in &chunk[..read] {
                total += 1;
                if line.len + 8 > line.bytes.len() {
                    println!("nextion: {label} <- {}", line.as_str());
                    line = CommandBuffer::new();
                }
                let printable = if (0x20..0x7f).contains(&byte) {
                    byte as char
                } else {
                    '.'
                };
                let _ = write!(line, "{byte:02X}{printable} ");
            }
        }

        if total == 0 {
            println!("nextion: {label} <- nothing");
        } else {
            println!("nextion: {label} <- {total} bytes: {}", line.as_str());
        }
        total
    }

    /// Switches only this side's rate. Probing uses it; normal code wants
    /// [`Nextion::set_baud`], which moves the display too.
    pub fn set_probe_baud(&mut self, baud: u32) -> Result<(), Error> {
        self.apply_baud(baud)
    }

    fn log(&self, args: core::fmt::Arguments<'_>) {
        if self.logging {
            println!("nextion: {args}");
        }
    }
}


/// RGB565 colours, the format every Nextion drawing command takes.
pub mod color {
    pub const BLACK: u16 = 0;
    pub const WHITE: u16 = 65535;
    pub const RED: u16 = 63488;
    pub const GREEN: u16 = 2016;
    pub const BLUE: u16 = 31;
    pub const YELLOW: u16 = 65504;
    pub const CYAN: u16 = 2047;
    pub const GRAY: u16 = 33840;
    pub const DARK: u16 = 2113;
}
