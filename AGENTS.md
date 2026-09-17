# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project overview

Smart home dashboard. An ESP32 runs bare-metal Rust (esp-hal + esp-rtos/Embassy) and drives a Nextion touch display over UART. It shows and controls lights, doors, windows, temperatures and washing machine status, one display page per floor of the house.

## Hardware

- ESP32 (Xtensa, target `xtensa-esp32-none-elf`)
- Nextion Enhanced NX4024K032, 3.2 inch, 400x240, touchscreen
- Display on UART2 at 9600 baud, TX on GPIO17, RX on GPIO16 (see `src/bin/main.rs`)
- Sensors: DHT11 (temperature/humidity), KY-024 hall sensor for door/window magnets

## Commands

The crate needs the `esp` Rust toolchain (`rust-toolchain.toml`), installed with `espup`. `.cargo/config.toml` sets the default target and `build-std`, so plain `cargo` commands cross-compile.

```sh
cargo build --release                                  # CI build
cargo run --release                                    # flash over USB and open serial monitor (espflash)
cargo fmt --all -- --check                             # CI format check
cargo clippy --all-features --workspace -- -D warnings # CI lint, warnings fail the build
```

There are no tests. The crate is `no_std` and has no test harness. Verify changes by flashing the board and reading the serial log. `wokwi.toml` and `diagram.json` let you run the debug build in the Wokwi simulator, though the simulated board has no display attached.

`.clippy.toml` sets `stack-size-threshold = 1024`, so clippy flags large stack values. Put big buffers on the heap or in a struct.

## Architecture

`src/lib.rs` is a `no_std` library with `alloc`. `src/bin/main.rs` is the firmware entry point that wires hardware to the library.

- `main` sets up the logger, a heap (`esp_alloc`, reclaimed RAM), esp-rtos and the display UART, then spawns `display_task`.
- `display_task` runs a `select` loop. It handles touch events from the display and re-renders whenever `sensor_data_task` signals new (currently mocked) floor data.
- `display::hmi::Hmi` is the trait for anything that can show pages and values and report touches. It uses `async fn` in a trait, so it is not object safe and callers take it as a generic.
- `display::display::Display<D: Hmi>` holds the app logic. `render` takes a slice of `Floor` and draws the floor whose index equals the current page. It keeps the last rendered floor and only sends values that changed, because every write repaints the component. A page change clears that cache, since loading a page resets its components. Touching component id 1 switches to page 1, any other touch switches to page 0.
- `display::nextion::Nextion` implements `Hmi` over the Nextion serial protocol. Every command and response ends with `0xFF 0xFF 0xFF`. Incoming bytes go into a `VecDeque` and get split on that terminator. `0x65` is a touch event (page, component id, pressed). `0x66` is the reply to `sendme` (current page), which `Nextion::new` uses to sync the starting page.
- `floor::Floor` is the per-floor state passed to `render`.
- `sensors` wraps the DHT11 and KY-024 drivers. They are not wired into `main` yet.
- `logger` is a custom `log` backend on `esp_println`. It uses `log::set_logger_racy` because the Xtensa core has no atomic compare-and-swap, so `logger::init` must run once before any task is spawned.

### Contract between firmware and display

`Display::render` writes to components by name, so the names in the Nextion project must match. All of them are text components with a solid background (`sta` = solid color). Index `i` counts within the floor.

- `room_{i}`: a box filling the room's inside. `txt` is the temperature as `21C` (empty if the room has no sensor), `bco` is dim yellow (25344) when the light is on and black when off. `lights` sets the number of rooms, `temperatures[i]` belongs to room `i`.
- `door_{i}`, `window_{i}`: a thin box covering the gap in the wall. `bco` is white (65535) when closed and red (63488) when open.
- `show_value` rejects non-ASCII strings, so no degree sign.

Boxes must not overlap any line from `post_init.s`. A component repaints its whole rectangle when updated and would erase the line, and post-init only runs again on page load.

Page 0's living room is an L, so it has a second box `room_0x` for the short arm. Timer `tm0` copies `room_0.bco` to it (`hmi/page0/tm0_timer.s`).

Page numbers equal floor indices. Renaming a component or reordering pages in the editor needs a matching firmware change.

## Display programming

The display is built by hand in Nextion Editor, not generated from code. `hmi/interface.HMI` is the editor project and `hmi/default.zi` is a pre-compiled font resource.

Nextion Editor has no readable export. After changing a page's post-initialization code in the editor, copy that code into `hmi/pageN/post_init.s` (one folder per page, currently `page0` and `page1`). These files hold the drawing commands for the floor plan (walls, doors, windows, staircase) with comments giving the coordinates. Other event code on a page goes in the same folder, named after the component and event (`tm0_timer.s`). Keep them in sync with the `.HMI` file so the layout can be reviewed and diffed in git.
