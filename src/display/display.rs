use crate::display::floor_plan::{Opening, doors, floor_plan, windows};
use crate::display::hmi::Hmi;
use crate::display::touch_event::TouchEvent;
use crate::floor::Floor;
use alloc::format;
use defmt::{error, trace};

const SECOND_FLOOR_ID: u8 = 1;

const WHITE: u32 = 65535;
const BLACK: u32 = 0;
const DIM_YELLOW: u32 = 25344;

/// Extra boxes that belong to a room, as [page](room, component). The
/// living room on page 0 is an L, drawn as two boxes.
const ROOM_EXTENSIONS: &[&[(usize, &str)]] = &[&[(0, "room_0x")]];

pub struct Display<D> {
    hmi: D,
    page: u8,
}

impl<D> Display<D> {
    pub fn new(hmi: D, current_page: u8) -> Self {
        Self {
            hmi,
            page: current_page,
        }
    }
}

impl<D: Hmi> Display<D> {
    pub async fn draw_floor_plan(&mut self) {
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

        self.erase_openings(doors(self.page)).await;
        self.erase_openings(windows(self.page)).await;

        for (i, light) in floor.lights.iter().enumerate() {
            let component = format!("room_{i}");
            let text = floor
                .temperatures
                .get(i)
                .map(|t| format!("{t}C"))
                .unwrap_or_default();
            self.hmi
                .set_number(&component, "bco", light_color(*light))
                .await;
            self.hmi.show_value(&component, &text).await;
        }
        if let Some(room_extensions) = ROOM_EXTENSIONS.get(self.page as usize) {
            for (room, component) in room_extensions.iter() {
                if let Some(light) = floor.lights.get(*room) {
                    self.hmi
                        .set_number(component, "bco", light_color(*light))
                        .await;
                }
            }
        }

        // Draw door on top of light background for rooms
        self.draw_openings(doors(self.page), &floor.doors).await;
        self.draw_openings(windows(self.page), &floor.windows).await;
        trace!("Render done, page {}", self.page);
    }

    async fn erase_openings(&mut self, openings: &[Opening]) {
        for opening in openings {
            self.hmi.draw_line(opening.opened(), BLACK).await;
            self.hmi.draw_line(opening.closed(), BLACK).await;
        }
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
        self.draw_floor_plan().await;
        trace!("Touch event handled, page {}", page);
    }
}

fn light_color(on: bool) -> u32 {
    if on { DIM_YELLOW } else { BLACK }
}
