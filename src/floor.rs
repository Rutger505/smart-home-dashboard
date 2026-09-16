pub struct Floor<'a> {
    pub temperatures: &'a [u8],
    pub doors: &'a [bool],
    pub windows: &'a [bool],
    pub lights: &'a [bool],
}
