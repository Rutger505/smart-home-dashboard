# smart-home-dashboard

ESP32 firmware that drives a Nextion touch display over UART. The ESP32 owns the
layout: the display holds an almost empty HMI project and gets drawn by serial
commands from the firmware.

## Hardware

- Board: ESP32 DOIT DevKit V1 (ESP-WROOM-32, no PSRAM, so GPIO16 and GPIO17 are free).
- Display: Nextion Enhanced NX4024K032, 3.2 inch, 400x240, resistive touch.
  Enhanced series, which adds an RTC, 1 KB EEPROM, GPIO pins and an SD slot over
  the Basic series.

Wiring, UART2:

| ESP32 pin | Silkscreen | Display wire |
|-----------|-----------|--------------|
| GPIO16    | D16 / RX2 | display TX (blue) |
| GPIO17    | D17 / TX2 | display RX (yellow) |

The display needs 5 V and draws more than the USB port on the DevKit reliably
supplies once the backlight is at full brightness. Power it separately and tie
the grounds together.

Do not move the display to GPIO1/GPIO3. That is UART0, which carries the USB
serial console that `esp_println` logs to.

## Baud rate

A factory display boots at 9600. `baud=` changes the rate for the current
session only, `bauds=` burns it into EEPROM and survives a power cycle. The rate
the display comes up at is therefore unknowable from the firmware side, so
`Nextion::configure` asks.

The fast path is `quick_link`: one handshake at 9600, retried up to three times,
which is what succeeds on this desk. Only if that fails does it wait
`POWER_ON_DELAY_MS` for a display that is still booting and fall back to
`detect_baud`, which sweeps 9600 and 115200 three times over before working
through `CANDIDATE_BAUDS` once.

`CANDIDATE_BAUDS` is in ascending order on purpose. Probing below the display's
real rate is harmless. Probing far above it produces a burst of framing errors,
which used to be fatal for the reason in the next section, so the list starts at
the rate a factory display powers on with and the two rates below 9600 come
last.

The firmware stays at whatever rate it found. `set_upgrade_baud(true)` turns on
the move to 115200, and it is off by default: on this unit the display takes the
`baud=` command but then answers nothing at the new rate, and a probe afterwards
finds it at neither rate until the ESP32 reboots. Drawing at 9600 costs about
450 ms for a full repaint and 57 ms per tile update, which is cheaper than a
link that might not come back. If you want 115200 for good, burn it once with
`store_baud` and let the probe find it there.

### The bug that looked like a deaf display

Symptom: the display answers the first probe of a session and then nothing ever
again, at any rate, until the ESP32 reboots. It looks exactly like the display
going deaf after being spoken to at the wrong rate, and it is not.

`Uart::read_buffered` returns `Err(RxError)` for any receive error seen since
the last check, and leaves the FIFO untouched when it does. Probing at the wrong
rate produces framing and glitch errors by the dozen. The driver used to write
`read_buffered(..).unwrap_or(0)`, which threw that error away without clearing
it, so every later read returned the same error forever and the display appeared
mute. `Nextion::read_some` now calls `check_for_rx_errors` on the error path,
which clears the events and resets the FIFO on overflow. Never call
`read_buffered` directly here; go through `read_some`.

Two smaller timing rules came out of the same hunt:

- A terminator sent right after a `baud=` change stops the display answering the
  handshake that follows, even though the rate change itself worked. `set_baud`
  waits, then drains, and sends nothing.
- `bkcmd=0` silences every reply, `connect` included. `configure` sets it last,
  after all the handshakes are done. Setting it earlier makes a working link
  look dead.

## Code layout

One crate, two binaries, the layout esp-generate produced. `src/lib.rs` is the
library both binaries import; it declares `pub mod nextion;` and `pub mod
screen;` and holds nothing else.

- `src/screen/nextion.rs`: the driver. `no_std`, no heap. Commands are formatted
  into a 128 byte stack buffer and terminated with `FF FF FF`. Every command is
  logged as `nextion: -> <command>` unless `set_logging(false)` turns that off,
  which is worth doing once the dashboard starts pushing updates every second.
- `src/screen/mod.rs`: the dashboard. `screen::run` takes the peripherals, runs
  the boot sequence, draws a header and a 2x2 grid of tiles, and repaints the
  readings once a second. Nothing appears on the display unless the firmware
  draws it, so an empty screen after a clean boot log means this file, not the
  wiring.
- `src/bin/main.rs`: the binary that runs. It initialises the chip, sets up the
  heap, and calls `screen::run`. Keep it that short. This is the one place the
  two sides meet, so `pub fn run(peripherals: Peripherals) -> !` is a contract:
  Claude can change what happens inside it but not its name or signature, since
  fixing the caller means editing a file Claude does not own.
- `src/nextion.rs`: Rutger's own driver, flat file, currently empty.
- `src/bin/playground.rs`: Rutger's binary, built every time and flashed only
  when he asks for it.
- `screen-hmi/interface.HMI`: the Nextion Editor project, plus
  `screen-hmi/default.zi`, a compiled font.

`build.rs` at the root passes `-Tlinkall.x` to the linker. Without it the binary
links at the wrong addresses and espflash rejects it with "appdesc segment not
found".

## The HMI file

`interface.HMI` is a proprietary binary, roughly 7.4 MB, almost all of it font
and image blobs. The object definitions sit in the last 10 KB or so as records
of `u32 length`, a 16 byte zero padded name field, then the value. It currently
holds `page0` with a text field `t0` and a number `n0`, and a second block that
looks like an editor revision copy.

Attributes inside that region can be rewritten in place as long as the length
prefix is fixed up. Adding objects or pages means synthesizing whole records and
has not been proven safe, so treat it as risky and check the result opens in
Nextion Editor.

Nothing here can produce a `.tft`. Compiling needs Nextion Editor on Windows.
That is the main argument for keeping the HMI nearly empty and drawing from the
firmware instead.

## Who owns what

Claude writes `src/screen/**` and `screen-hmi/`. Everything else is Rutger's:
`src/nextion.rs`, where he writes a driver by hand to learn the Nextion protocol
from the official documentation, `src/bin/playground.rs`, `src/bin/main.rs` and
`src/lib.rs`. Claude must not create, edit or refactor those, and must not offer
finished code for them. Reading them to answer a direct question is fine.

Both drivers talk to the same UART2. They never run at once, since each binary
picks one, but any future sharing has to hand the link over drained and with the
receive errors cleared, for the reason in the section above.

## Build and flash

`cargo build` builds both binaries, and `default-run` makes the dashboard the
one a bare `cargo run` picks.

The dashboard:

```
cargo build
espflash flash --port /dev/ttyUSB0 --chip esp32 target/xtensa-esp32-none-elf/debug/smart-home-dashboard
```

Rutger's playground is the same command with
`target/xtensa-esp32-none-elf/debug/playground`, or `cargo run --bin playground`
for espflash with a monitor attached, per `.cargo/config.toml`.

`cargo run` opens an interactive monitor that never exits, which is useless in
an automated loop. To flash and capture a log without hanging:

```
espflash flash --port /dev/ttyUSB0 --chip esp32 target/xtensa-esp32-none-elf/debug/screen
timeout 30 espflash monitor --port /dev/ttyUSB0 --chip esp32 --non-interactive
```

A short pyserial script that toggles DTR and RTS to reset the board, then reads
for a fixed number of seconds, is the better tool when you want several boots in
a row: espflash spends about seven seconds connecting each time.

The console log arrives scrambled, with fragments of earlier lines cut into
later ones. That is the ESP32 overrunning its own UART0 transmit FIFO, not the
monitor: raw reads straight from `/dev/ttyUSB0` show the same damage. Printing
fewer lines with a delay between them comes out clean, which is what
a diagnostic binary should do with `RESULT name=value` lines. Grep for what you
want rather than reading the log top to bottom, and do not trust the order.

`cargo clippy --all-targets` fails with "can't find crate for `test`" because
this is a bare metal target with no test harness. Use plain `cargo clippy`.

`esp-hal` is built with its `unstable` feature on, which is what makes
`esp_hal::delay::Delay` and the UART driver available.
