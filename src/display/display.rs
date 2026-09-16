use crate::display::hmi::Hmi;
use crate::display::touch_event::TouchEvent;
use crate::floor::Floor;

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
    pub async fn render(&mut self, floors: &[Floor<'_>]) {}

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

