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
session only, `bauds=` burns it into EEPROM and survives a power cycle. That
makes the rate the display comes up at unknowable from the firmware side, so
`Nextion::detect_baud` sends `connect` at each rate in `CANDIDATE_BAUDS` and
watches for the `comok` reply. Whatever answers wins, then `configure` moves both
sides to 115200 for the session.

If the display never replies, the firmware logs it and keeps going at 9600. The
commands still land as long as the ESP32 TX wire is good, you just lose the
confirmation. A missing reply usually means the display TX wire is not connected
or is on the wrong pin.

## Code layout

- `src/nextion.rs`: the driver. `no_std`, no heap. Commands are formatted into a
  128 byte stack buffer and terminated with `FF FF FF`. Every command is logged
  as `nextion: -> <command>` unless `set_logging(false)` turns that off, which
  is worth doing once the dashboard starts pushing updates every second.
- `src/bin/main.rs`: opens UART2, runs the boot sequence, reports what happened.
- `screen/interface.HMI`: the Nextion Editor project, plus `screen/default.zi`,
  a compiled font.

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

## Build and flash

```
cargo build
cargo run          # espflash flash --monitor, per .cargo/config.toml
```

`cargo clippy --all-targets` fails with "can't find crate for `test`" because
this is a bare metal target with no test harness. Use plain `cargo clippy`.

`esp-hal` is built with its `unstable` feature on, which is what makes
`esp_hal::delay::Delay` and the UART driver available.
