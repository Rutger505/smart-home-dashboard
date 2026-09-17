use crate::display::hmi::Hmi;
use crate::display::touch_event::TouchEvent;
use crate::floor::Floor;
use alloc::format;
use alloc::string::ToString;
use log::error;

const SECOND_FLOOR_ID: u8 = 1;

pub struct Display<D> {
    hmi: D,
    page: u8,
}

impl<D> Display<D> {
    pub fn new(hmi: D, current_page: u8) -> Self {
        Self { hmi, page: current_page }
    }
}

impl<D: Hmi> Display<D> {
    pub async fn render(&mut self, floors: &[Floor]) {
        let Some(floor) = floors.get(self.page as usize) else {
            error!("Floors passed to render does not contain current floor");
            return;
        };

        for (i, temperature) in floor.temperatures.iter().enumerate() {
            let component = format!("temperature_{i}");
            self.hmi.show_value(&component, &temperature.to_string()).await;
        }
        for (i, state) in floor.doors.iter().enumerate() {
            let component = format!("door_{i}");
            self.hmi.show_value(&component, if *state { "Closed" } else { "Open" }).await;
        }
        for (i, state) in floor.windows.iter().enumerate() {
            let component = format!("window_{i}");
            self.hmi.show_value(&component, if *state { "Closed" } else { "Open" }).await;
        }
        for (i, state) in floor.lights.iter().enumerate() {
            let component = format!("lights_{i}");
            self.hmi.show_value(&component, if *state { "On" } else { "Off" }).await;
        }
    }

    pub async fn next_touch_event(&mut self) -> TouchEvent {
        self.hmi.next_touch_event().await
    }

    pub async fn handle_touch_event(&mut self, event: TouchEvent) {
        self.hmi.show_page(if event.component_id == SECOND_FLOOR_ID {
            1
        } else {
            0
        })
            .await;
    }
}

