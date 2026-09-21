# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project overview

Smart home dashboard. An ESP32 runs bare-metal Rust (esp-hal + esp-rtos/Embassy) and drives a Nextion touch display over UART. It shows and controls lights, doors, windows, temperatures and washing machine status, one display page per floor of the house.

## Hardware

- ESP32 (Xtensa, target `xtensa-esp32-none-elf`)
- Nextion Enhanced NX4024K032, 3.2 inch, 400x240, touchscreen
- Display on UART2 at 921600 baud, TX on GPIO17, RX on GPIO16 (see `src/bin/main.rs`)
- Sensors: DHT11 (temperature/humidity), KY-024 hall sensor for door/window magnets

## Commands

The crate needs the `esp` Rust toolchain (`rust-toolchain.toml`), installed with `espup`. `.cargo/config.toml` sets the default target and `build-std`, so plain `cargo` commands cross-compile.

```sh
cargo build --release                                  # CI build
cargo run --release                                    # flash over USB and open serial monitor (espflash)
cargo fmt --all -- --check                             # CI format check
cargo clippy --all-features --workspace -- -D warnings # CI lint, warnings fail the build
```

There are no tests. The crate is `no_std` and has no test harness. Verify changes by flashing the board and reading the serial log.

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
- Logging uses `defmt` over UART0 through `esp-println` (`defmt-espflash`). The device sends compact binary frames and `espflash monitor` decodes them against the ELF, so a plain serial terminal shows garbage. `DEFMT_LOG` in `.cargo/config.toml` sets the level at build time. Filtered levels are compiled out, so changing the level needs a rebuild. Logged types need `defmt::Format`, and defmt has its own format syntax (no inline `{name}` args, `{=[u8]:02X}` for hex bytes).

### Contract between firmware and display

`Display::render` writes to components by name, so the names in the Nextion project must match. All of them are text components with a solid background (`sta` = solid color). Index `i` counts within the floor.

- `room_{i}`: a box filling the room's inside. `txt` is the temperature as `21C` (empty if the room has no sensor), `bco` is dim yellow (25344) when the light is on and black when off. `lights` sets the number of rooms, `temperatures[i]` belongs to room `i`.
- `show_value` rejects non-ASCII strings, so no degree sign.

Page 0's living room is an L, so it has a second box `room_0x` for the short arm. `ROOM_EXTENSIONS` in `display.rs` lists it, and `render` gives it the same `bco` as `room_0`.

Page numbers equal floor indices. Renaming a component or reordering pages in the editor needs a matching firmware change.

### Floor plan

The ESP draws the floor plan itself with Nextion `line` commands, so the editor project holds only the components. `display::floor_plan` has the wall coordinates of each page, computes the staircase treads (with `libm`, since `core` has no trig) and holds the hinge, gap and swing direction of each door and window.

- `Display::draw_floor_plan` draws the walls and staircase. It runs when `display_task` starts and after every page change, because loading a page clears the screen. After a page change `display_task` also renders the last sensor data right away.
- `render` draws doors and windows after the rooms. Closed is a white line over the gap. Open is the gap in black plus a white line swung 45 degrees into a room box. Before the rooms, `render` draws the open line of every closed door and window in black, which erases the pixels between the wall and the box. The room repaint right after gives the part inside the box its color back, so every door and window has to swing into a room box.

Component boxes must not overlap any wall or stair line. A component repaints its whole rectangle when updated and would erase the line, and the floor plan is only drawn again on a page change.

## Display programming

The display is built by hand in Nextion Editor, not generated from code. `hmi/interface.HMI` is the editor project and `hmi/default.zi` is a pre-compiled font resource.

Nextion Editor has no readable export. If a page gets event code in the editor, copy it into `hmi/pageN/`, named after the component and event (for example `b0_press.s`), so it can be reviewed and diffed in git. `hmi/Program.s` is the startup program, which sets the baud rate to match `main.rs`. Drawing belongs in `floor_plan.rs`, not in page events.
