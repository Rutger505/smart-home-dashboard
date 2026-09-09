//! The dashboard: boot sequence, layout, and the once-a-second repaint.

pub mod nextion;

use core::fmt::Write;

use esp_hal::delay::Delay;
use esp_hal::peripherals::Peripherals;
use esp_hal::uart::{Config, Uart};

use self::nextion::{BOOT_BAUD, Error, Nextion, color};

/// Backlight level sent at boot, 0-100.
const BRIGHTNESS: u8 = 100;

const WIDTH: u16 = 400;
const HEIGHT: u16 = 240;
const HEADER_HEIGHT: u16 = 44;
const FONT: u8 = 0;

/// One tile of the 2x2 grid below the header.
struct Tile {
    label: &'static str,
    accent: u16,
}

const TILES: [Tile; 4] = [
    Tile {
        label: "LIVING",
        accent: color::CYAN,
    },
    Tile {
        label: "KITCHEN",
        accent: color::GREEN,
    },
    Tile {
        label: "OUTSIDE",
        accent: color::YELLOW,
    },
    Tile {
        label: "UPTIME",
        accent: color::RED,
    },
];

/// Stack formatting for the one line of text a tile shows.
#[derive(Clone, Copy)]
struct Line {
    bytes: [u8; 32],
    len: usize,
}

impl Line {
    fn new() -> Self {
        Self {
            bytes: [0; 32],
            len: 0,
        }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("?")
    }
}

impl Write for Line {
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

fn tile_box(index: usize) -> (u16, u16, u16, u16) {
    let width = WIDTH / 2;
    let height = (HEIGHT - HEADER_HEIGHT) / 2;
    let x = (index as u16 % 2) * width;
    let y = HEADER_HEIGHT + (index as u16 / 2) * height;
    (x, y, width, height)
}

fn draw_layout(screen: &mut Nextion<'_>) -> Result<(), Error> {
    screen.clear(color::DARK)?;
    screen.fill(0, 0, WIDTH, HEADER_HEIGHT, color::BLUE)?;
    screen.draw_text(
        0,
        6,
        WIDTH,
        HEADER_HEIGHT - 12,
        FONT,
        color::WHITE,
        color::BLUE,
        "SMART HOME",
    )?;

    for (index, tile) in TILES.iter().enumerate() {
        let (x, y, width, height) = tile_box(index);
        screen.fill(x + 4, y + 4, width - 8, height - 8, color::BLACK)?;
        screen.fill(x + 4, y + 4, width - 8, 4, tile.accent)?;
        screen.draw_text(
            x + 8,
            y + 12,
            width - 16,
            20,
            FONT,
            tile.accent,
            color::BLACK,
            tile.label,
        )?;
    }
    Ok(())
}

fn draw_reading(screen: &mut Nextion<'_>, index: usize, value: &str) -> Result<(), Error> {
    let (x, y, width, height) = tile_box(index);
    screen.draw_text(
        x + 8,
        y + 36,
        width - 16,
        height - 48,
        FONT,
        color::WHITE,
        color::BLACK,
        value,
    )
}

pub fn run(peripherals: Peripherals) -> ! {
    let delay = Delay::new();

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
        Ok(Some(baud)) => esp_println::println!("boot: screen up at {baud} baud"),
        Ok(None) => esp_println::println!(
            "boot: screen never replied, commands sent blind. Check the display TX wire and that \
             its baud rate is one of the probed rates"
        ),
        Err(error) => esp_println::println!("boot: screen setup failed: {error:?}"),
    }

    // Logging costs more than the drawing does at this point: every line goes
    // out of the console UART while the display waits.
    screen.set_logging(false);
    if let Err(error) = draw_layout(&mut screen) {
        esp_println::println!("boot: layout failed: {error:?}");
    }

    let mut seconds: u32 = 0;
    let mut shown = [Line::new(); TILES.len()];
    loop {
        tick(&mut screen, seconds, &mut shown);
        delay.delay_millis(1000);
        seconds += 1;
    }
}

/// Repaints the readings that changed. Kept out of `run` so its buffers do not
/// sit on the stack frame the `#[main]` macro generates.
///
/// Redrawing a tile costs about 55 bytes, which is 57 ms of the second at 9600
/// baud, so tiles whose text is unchanged are worth skipping.
#[inline(never)]
fn tick(screen: &mut Nextion<'_>, seconds: u32, shown: &mut [Line; TILES.len()]) {
    for (index, value) in [(0usize, 21), (1, 23), (2, 12)] {
        let mut line = Line::new();
        let _ = write!(line, "{value}.{} C", (seconds + index as u32) % 10);
        update(screen, index, line, shown);
    }

    let mut line = Line::new();
    let _ = write!(line, "{}:{:02}", seconds / 60, seconds % 60);
    update(screen, 3, line, shown);
}

fn update(screen: &mut Nextion<'_>, index: usize, line: Line, shown: &mut [Line; TILES.len()]) {
    if shown[index].as_str() == line.as_str() {
        return;
    }
    if draw_reading(screen, index, line.as_str()).is_ok() {
        shown[index] = line;
    }
}
