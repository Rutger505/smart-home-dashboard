use alloc::vec::Vec;

pub struct Floor {
    pub temperatures: Vec<u8>,
    pub doors: Vec<bool>,
    pub windows: Vec<bool>,
    pub lights: Vec<bool>,
}
