use crate::display::floor_plan::{Opening, doors, floor_plan, windows};
use crate::display::hmi::Hmi;
use crate::display::touch_event::TouchEvent;
use crate::floor::Floor;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use defmt::{debug, error, trace};
use embassy_time::{Duration, Timer};

const SECOND_FLOOR_ID: u8 = 1;

const ROOM_REPAINT_DELAY: Duration = Duration::from_millis(40);

const WHITE: u32 = 65535;
const BLACK: u32 = 0;
const DIM_YELLOW: u32 = 25344;

/// Extra boxes that belong to a room, as [page](room, component). The
/// living room on page 0 is an L, drawn as two boxes.
const ROOM_EXTENSIONS: &[&[(usize, &str)]] = &[&[(0, "room_0x")]];

pub struct Display<D> {
    hmi: D,
    page: u8,
    /// What the screen shows now. `None` after a page load, which resets
    /// every component.
    shown: Option<Floor>,
}

impl<D> Display<D> {
    pub fn new(hmi: D, current_page: u8) -> Self {
        Self {
            hmi,
            page: current_page,
            shown: None,
        }
    }
}

impl<D: Hmi> Display<D> {
    pub async fn draw_floor_plan(&mut self) {
        debug!("Drawing floor {}", self.page);
        for line in floor_plan(self.page) {
            self.hmi.draw_line(line, WHITE).await;
        }
    }

    pub async fn render(&mut self, floors: &[Floor]) {
        trace!("Render start, page {}", self.page);

        let Some(floor) = floors.get(self.page as usize) else {
            error!("Floors passed to render does not contain current floor");
            return;
        };

        if self.shown.as_ref() == Some(floor) {
            trace!("Nothing changed, page {}", self.page);
            return;
        }
        let shown = self.shown.take();

        let doors_changed = changed_openings(
            doors(self.page),
            shown.as_ref().map(|f| &f.doors),
            &floor.doors,
        );
        let windows_changed = changed_openings(
            windows(self.page),
            shown.as_ref().map(|f| &f.windows),
            &floor.windows,
        );
        let openings_changed =
            doors_changed.clone().next().is_some() || windows_changed.clone().next().is_some();

        for opening in doors_changed.chain(windows_changed) {
            self.erase_opening(opening).await;
        }

        // Erasing an opening leaves a black line inside the room box it
        // swings into, so every box gets repainted after an erase.
        let mut repainted = false;
        for (i, light) in floor.lights.iter().enumerate() {
            let component = format!("room_{i}");
            let text = temperature_text(floor, i);
            let shown_light = shown.as_ref().and_then(|f| f.lights.get(i));
            let shown_text = shown.as_ref().map(|f| temperature_text(f, i));

            if openings_changed || shown_light != Some(light) {
                self.hmi
                    .set_number(&component, "bco", light_color(*light))
                    .await;
                repainted = true;
            }
            if shown_text.as_ref() != Some(&text) {
                self.hmi.show_value(&component, &text).await;
                repainted = true;
            }
        }
        if let Some(room_extensions) = ROOM_EXTENSIONS.get(self.page as usize) {
            for (room, component) in room_extensions.iter() {
                let Some(light) = floor.lights.get(*room) else {
                    continue;
                };
                let shown_light = shown.as_ref().and_then(|f| f.lights.get(*room));
                if openings_changed || shown_light != Some(light) {
                    self.hmi
                        .set_number(component, "bco", light_color(*light))
                        .await;
                    repainted = true;
                }
            }
        }

        // A repainted box covers the part of a door that swings into it, so
        // all doors and windows get drawn again. The Nextion can still be
        // repainting the boxes when the door commands arrive, and would then
        // paint the background over a door.
        if repainted {
            Timer::after(ROOM_REPAINT_DELAY).await;
            self.draw_openings(doors(self.page), &floor.doors).await;
            self.draw_openings(windows(self.page), &floor.windows).await;
        }

        self.shown = Some(floor.clone());
        trace!("Render done, page {}", self.page);
    }

    async fn erase_opening(&mut self, opening: &Opening) {
        self.hmi.draw_line(opening.opened(), BLACK).await;
        self.hmi.draw_line(opening.closed(), BLACK).await;
    }

    async fn draw_openings(&mut self, openings: &[Opening], closed: &[bool]) {
        for (opening, closed) in openings.iter().zip(closed) {
            self.hmi
                .draw_line(
                    if *closed {
                        opening.closed()
                    } else {
                        opening.opened()
                    },
                    WHITE,
                )
                .await;
        }
    }

    pub async fn next_touch_event(&mut self) -> TouchEvent {
        self.hmi.next_touch_event().await
    }

    pub async fn handle_touch_event(&mut self, event: TouchEvent) {
        trace!(
            "Touch event: page {}, component {}, pressed {}",
            event.page, event.component_id, event.pressed
        );

        let page = if event.component_id == SECOND_FLOOR_ID {
            1
        } else {
            0
        };

        self.hmi.show_page(page).await;
        self.page = page;
        self.shown = None;
        self.draw_floor_plan().await;
        trace!("Touch event handled, page {}", page);
    }
}

fn light_color(on: bool) -> u32 {
    if on { DIM_YELLOW } else { BLACK }
}

fn temperature_text(floor: &Floor, room: usize) -> String {
    floor
        .temperatures
        .get(room)
        .map(|t| format!("{t}C"))
        .unwrap_or_default()
}

/// The openings whose state differs from what the screen shows, or all of
/// them when the screen shows nothing yet.
fn changed_openings<'a>(
    openings: &'a [Opening],
    shown: Option<&'a Vec<bool>>,
    closed: &'a [bool],
) -> impl Iterator<Item = &'a Opening> + Clone {
    openings
        .iter()
        .enumerate()
        .filter(move |(i, _)| shown.and_then(|s| s.get(*i)) != closed.get(*i))
        .map(|(_, opening)| opening)
}
