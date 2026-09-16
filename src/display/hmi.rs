use crate::display::touch_event::TouchEvent;

pub trait Hmi {
    async fn show_page(&mut self, page: u8);
    async fn show_number(&mut self, component: &str, value: u8);
    async fn next_touch_event(&mut self) -> TouchEvent;
}
