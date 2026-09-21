use crate::display::floor_plan::Line;
use crate::display::touch_event::TouchEvent;

pub trait Hmi {
    async fn get_page(&mut self) -> u8;
    async fn show_page(&mut self, page: u8);
    async fn show_value(&mut self, component: &str, value: &str);
    async fn set_number(&mut self, component: &str, attribute: &str, value: u32);
    async fn draw_line(&mut self, line: Line, color: u32);
    async fn next_touch_event(&mut self) -> TouchEvent;
}
