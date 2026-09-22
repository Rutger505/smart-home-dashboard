use alloc::vec::Vec;

#[derive(Clone, PartialEq, defmt::Format)]
pub struct Floor {
    pub temperatures: Vec<u8>,
    pub doors: Vec<bool>,
    pub windows: Vec<bool>,
    pub lights: Vec<bool>,
}
