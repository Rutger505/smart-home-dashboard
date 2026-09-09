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

/// Every rate a Nextion display accepts, in the order it is safe to probe them.
///
/// Ascending from the rate a factory display boots at, because the direction of
/// a wrong guess decides whether the display survives it. Probing below the
/// display's real rate is harmless and it answers the next probe about 300 ms
/// later. Probing above it wedges it: measured here, a single `connect` sent at
/// 115200 to a display listening at 9600 left it silent through 10 s of quiet
/// and every later probe, until the ESP32 rebooted. So 9600 goes first, the
/// faster rates follow in order, and the two rates below 9600 come last, where
/// they are only reached after everything else has already missed.
pub const CANDIDATE_BAUDS: [u32; 13] = [
    9600, 19200, 31250, 38400, 57600, 115_200, 230_400, 250_000, 256_000, 512_000, 921_600, 4800,
    2400,
];

/// Milliseconds to wait for the display to finish its own boot before talking
/// to it. The ESP32 is ready well before the Nextion is.
const POWER_ON_DELAY_MS: u32 = 500;

/// Milliseconds of quiet on either side of a baud rate change, so the last
/// command drains and the display's UART restarts before the next byte. 50 ms
/// was not enough: the terminator that follows went out at the new rate while
/// the display was still listening at the old one, and the switch then looked
/// like it had failed.
const BAUD_SWITCH_SETTLE_MS: u32 = 300;

/// Milliseconds to wait for a `connect` reply while probing one baud rate.
/// The reply is around 70 bytes, so the slow rates need most of this.
const CONNECT_REPLY_TIMEOUT_MS: u64 = 350;

/// Milliseconds of quiet between flushing the display's parser and speaking to
/// it again, so the terminator that ends a half-received command lands before
/// the next one starts.
const PROBE_SETTLE_MS: u32 = 60;

/// Milliseconds to leave the display alone after a probe it did not answer, so
/// the garbage that probe delivered is not still arriving when the next one
/// goes out.
const PROBE_RECOVERY_MS: u32 = 200;

/// Milliseconds a page change takes. Drawing commands sent inside this window
/// are dropped, and with `bkcmd=0` they are dropped silently.
const PAGE_LOAD_MS: u32 = 200;

/// How many times to work through a rate list before giving up.
const PROBE_PASSES: u8 = 3;

/// The rates worth trying before the full list: the rate a factory display
/// powers on at, then the rate this driver switches to. Ascending, like
/// [`CANDIDATE_BAUDS`], and for the same reason.
const COMMON_BAUDS: [u32; 2] = [BOOT_BAUD, RUN_BAUD];

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
    upgrade_baud: bool,
}

impl<'d> Nextion<'d> {
    pub fn new(uart: Uart<'d, Blocking>) -> Self {
        Self {
            uart,
            delay: Delay::new(),
            logging: true,
            upgrade_baud: false,
        }
    }

    /// Lets [`Nextion::configure`] move a display it found at some other rate
    /// up to [`RUN_BAUD`]. Off by default.
    ///
    /// The switch is not reliable on this NX4024K032: the display takes the
    /// `baud=` command but then answers nothing at the new rate, and a probe
    /// afterwards finds it at neither rate until the ESP32 reboots. Drawing at
    /// 9600 costs about 0.6 s for a full repaint and 0.2 s for the four
    /// readings, which is cheap enough to prefer over a link that may not come
    /// back. Burn the rate into the display's EEPROM with
    /// [`Nextion::store_baud`] if you want 115200 for good.
    pub fn set_upgrade_baud(&mut self, upgrade: bool) {
        self.upgrade_baud = upgrade;
    }

    /// Turns the per-command log lines on or off. On by default; turn it off
    /// once the dashboard starts pushing sensor updates every second.
    pub fn set_logging(&mut self, logging: bool) {
        self.logging = logging;
    }

    /// Brings the display to a known state: awake, backlit, on page 0, running
    /// at [`RUN_BAUD`].
    ///
    /// Returns the baud rate the link ended up on, or `None` if the display
    /// never answered. A display that is wired TX-only cannot answer, so a
    /// `None` here is not fatal and the boot sequence runs anyway at
    /// [`BOOT_BAUD`].
    pub fn configure(&mut self, brightness: u8) -> Result<Option<u32>, Error> {
        // The display is only still booting when it was powered up alongside
        // the ESP32. After a reflash or a warm reset it has been running for
        // ages, so ask first and wait only if nobody answers.
        let detected = match self.quick_link()? {
            Some(baud) => Some(baud),
            None => {
                self.log(format_args!(
                    "no answer yet, waiting {POWER_ON_DELAY_MS} ms for display boot"
                ));
                self.delay.delay_millis(POWER_ON_DELAY_MS);
                self.detect_baud()?
            }
        };
        match detected {
            Some(baud) => self.log(format_args!("display answered at {baud} baud")),
            None => self.log(format_args!(
                "no reply on any rate, assuming {BOOT_BAUD} baud"
            )),
        }

        let mut rate = detected.unwrap_or(BOOT_BAUD);
        if self.upgrade_baud && detected.is_some_and(|baud| baud != RUN_BAUD) {
            self.set_baud(RUN_BAUD)?;
            if self.confirm_link()? {
                rate = RUN_BAUD;
                self.log(format_args!("display followed to {RUN_BAUD} baud"));
            } else {
                self.log(format_args!("no answer at {RUN_BAUD} baud, probing again"));
                match self.detect_baud()? {
                    Some(found) => {
                        rate = found;
                        self.log(format_args!("display is at {found} baud"));
                    }
                    None => {
                        rate = RUN_BAUD;
                        self.apply_baud(RUN_BAUD)?;
                        self.log(format_args!("still no answer, assuming {RUN_BAUD} baud"));
                    }
                }
            }
        }

        // Silence the acknowledgements only once every handshake is done.
        // With `bkcmd=0` the display answers nothing at all, `connect`
        // included, so setting it earlier makes the link check fail on a link
        // that is working.
        self.send("bkcmd=0")?;
        self.send("sleep=0")?;
        self.send_fmt(format_args!("dim={}", brightness.min(100)))?;
        self.send("thup=1")?;
        Ok(detected.map(|_| rate))
    }

    /// One handshake at [`BOOT_BAUD`], for the common case where the display
    /// is already up and running at the rate it powers on with. Costs about
    /// 80 ms when it works, against roughly 700 ms for a full probe.
    pub fn quick_link(&mut self) -> Result<Option<u32>, Error> {
        self.apply_baud(BOOT_BAUD)?;
        for _ in 0..PROBE_PASSES {
            // A reset in the middle of a command leaves the display holding
            // half of one, and it would swallow the probe that follows.
            self.flush_parser();
            if self.handshake()? {
                return Ok(Some(BOOT_BAUD));
            }
            self.delay.delay_millis(PROBE_RECOVERY_MS);
        }
        Ok(None)
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
        let (answered, seen) = self.wait_for(b"comok", CONNECT_REPLY_TIMEOUT_MS);
        if !answered {
            self.log(format_args!("no comok, {seen} bytes came back"));
        }
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

    /// Sends `count` terminator bytes, to end a command the display is half
    /// way through parsing. Only useful for experiments; normal code wants
    /// [`Nextion::flush_parser`].
    pub fn flush_terminators(&mut self, count: usize) {
        for _ in 0..count {
            let _ = self.write_raw(&[0xFF]);
        }
        let _ = self.uart.flush();
        self.drain();
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
        self.delay.delay_millis(BAUD_SWITCH_SETTLE_MS);
        // No terminator here. The display has just restarted its UART, and a
        // terminator sent into that window stops it answering the handshake
        // that follows, even though the rate change itself worked.
        self.drain();
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
        // No terminator here. The display has just restarted its UART, and a
        // terminator sent into that window stops it answering the handshake
        // that follows, even though the rate change itself worked.
        self.drain();
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
        while self.read_some(&mut scratch) > 0 {}
    }

    /// Reads whatever is already buffered, clearing the receiver's error state
    /// first.
    ///
    /// `read_buffered` reports any receive error seen since the last check and
    /// leaves the FIFO untouched when it does. Probing at the wrong baud rate
    /// produces framing and glitch errors by the dozen, so without this the
    /// first bad probe makes every later read return that same error forever
    /// and the display looks dead until the ESP32 reboots.
    fn read_some(&mut self, buffer: &mut [u8]) -> usize {
        match self.uart.read_buffered(buffer) {
            Ok(read) => read,
            Err(_) => {
                let _ = self.uart.check_for_rx_errors();
                self.uart.read_buffered(buffer).unwrap_or(0)
            }
        }
    }

    /// Reads until `needle` shows up or `timeout_ms` passes. Also reports how
    /// many bytes arrived, which separates a display that said the wrong thing
    /// from one that said nothing at all.
    fn wait_for(&mut self, needle: &[u8], timeout_ms: u64) -> (bool, usize) {
        let mut window = [0u8; 64];
        let mut len = 0usize;
        let mut seen = 0usize;
        let deadline = Duration::from_millis(timeout_ms);
        let start = Instant::now();

        while start.elapsed() < deadline {
            let mut chunk = [0u8; 32];
            let read = self.read_some(&mut chunk);
            seen += read;
            for &byte in &chunk[..read] {
                if len == window.len() {
                    window.copy_within(1.., 0);
                    len -= 1;
                }
                window[len] = byte;
                len += 1;
            }
            if window[..len].windows(needle.len()).any(|w| w == needle) {
                return (true, seen);
            }
        }
        (false, seen)
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
            let read = self.read_some(&mut chunk);
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
