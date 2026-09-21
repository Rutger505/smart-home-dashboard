//! This file is managed by Agents, these coordinates where not manually created, nor is the code.
//! The floor plan of each page: walls, staircase, doors and windows. The ESP
//! draws these over UART with Nextion `line` commands.
//!
//! No wall or stair line may overlap a component box. A component repaints
//! its whole rectangle when updated and would erase the line. Doors and
//! windows are the exception: an open one swings into a room box, so the
//! firmware draws them after the rooms on every render. Every opening has to
//! swing into a room box, because the black line that erases a closed one
//! only gets repainted there.

use alloc::vec::Vec;
use core::f32::consts::{FRAC_1_SQRT_2, PI};
use libm::{cosf, roundf, sinf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Line {
    pub x1: u16,
    pub y1: u16,
    pub x2: u16,
    pub y2: u16,
}

const fn line(x1: u16, y1: u16, x2: u16, y2: u16) -> Line {
    Line { x1, y1, x2, y2 }
}

// Living room is an L: long arm (205,6 to 295,233), short arm on top of the
// corridor (155,6 to 205,126). Corridor is 155,126 to 205,233.
const PAGE0_WALLS: &[Line] = &[
    // Outer top wall, split for a window (210 to 240)
    line(155, 6, 210, 6),
    line(240, 6, 295, 6),
    // Outer right wall, split for a window (100 to 130)
    line(295, 6, 295, 100),
    line(295, 130, 295, 233),
    // Outer bottom wall, split for the main entrance door (160 to 190)
    line(155, 233, 160, 233),
    line(190, 233, 295, 233),
    // Corridor outer left wall, split for the staircase opening (163 to 213)
    line(155, 6, 155, 163),
    line(155, 213, 155, 233),
    // Corridor / living room dividing walls, split for a door (167 to 191)
    line(155, 126, 205, 126),
    line(205, 126, 205, 167),
    line(205, 191, 205, 233),
];

// Top bedroom spans the full width (155,6 to 295,63). Below it the corridor
// (155,63 to 205,233) reaches 3 bedrooms stacked on the right, B, C and D.
const PAGE1_WALLS: &[Line] = &[
    // Outer top wall, split for the top bedroom's window (210 to 240)
    line(155, 6, 210, 6),
    line(240, 6, 295, 6),
    // Outer right wall, split for the windows of B (80 to 100), C (135 to
    // 155) and D (195 to 215)
    line(295, 6, 295, 63),
    line(295, 63, 295, 80),
    line(295, 100, 295, 119),
    line(295, 119, 295, 135),
    line(295, 155, 295, 176),
    line(295, 176, 295, 195),
    line(295, 215, 295, 233),
    // Outer bottom wall, no exterior door on this floor
    line(155, 233, 295, 233),
    // Corridor outer left wall, split for the staircase opening (63 to 113)
    line(155, 6, 155, 63),
    line(155, 113, 155, 233),
    // Top bedroom / corridor wall, split for its door (170 to 182)
    line(155, 63, 170, 63),
    line(182, 63, 205, 63),
    line(205, 63, 295, 63),
    // Bedroom / corridor wall, split for the doors of B (85 to 97), C (141
    // to 153) and D (198 to 210)
    line(205, 63, 205, 85),
    line(205, 97, 205, 141),
    line(205, 153, 205, 198),
    line(205, 210, 205, 233),
    // Bedroom / bedroom walls
    line(205, 119, 295, 119),
    line(205, 176, 295, 176),
];

// The staircase is a box left of the corridor, the same on both floors, with
// the corridor wall as its right side. It turns a quarter at each end, into
// the ground floor corridor at the bottom and the second floor corridor at
// the top. Each opening is as tall as the box is wide, so the first step is
// flush with both walls.
const STAIRS_LEFT: f32 = 105.0;
const STAIRS_RIGHT: f32 = 155.0;
const STAIRS_TOP: f32 = 63.0;
const STAIRS_BOTTOM: f32 = 213.0;
const STAIRS_WIDTH: f32 = STAIRS_RIGHT - STAIRS_LEFT;
const STAIRS_TOP_PIVOT: (f32, f32) = (STAIRS_RIGHT, STAIRS_TOP + STAIRS_WIDTH);
const STAIRS_BOTTOM_PIVOT: (f32, f32) = (STAIRS_RIGHT, STAIRS_BOTTOM - STAIRS_WIDTH);
const STAIRS_STEP: usize = 10;
const STAIRS_WINDERS: usize = 6;

pub struct Opening {
    hinge: (i32, i32),
    end: (i32, i32),
    swing: (i32, i32),
}

// A door or window in a horizontal or vertical wall. The gap runs from hinge
// to end, and it swings open toward `swing`.
const fn opening(hinge: (i32, i32), end: (i32, i32), swing: (i32, i32)) -> Opening {
    Opening { hinge, end, swing }
}

const PAGE0_DOORS: &[Opening] = &[
    // Main entrance, into the corridor
    opening((160, 233), (190, 233), (0, -1)),
    // Corridor to living room
    opening((205, 167), (205, 191), (1, 0)),
];

const PAGE0_WINDOWS: &[Opening] = &[
    opening((210, 6), (240, 6), (0, 1)),
    opening((295, 100), (295, 130), (-1, 0)),
];

const PAGE1_DOORS: &[Opening] = &[
    opening((170, 63), (182, 63), (0, -1)),
    opening((205, 85), (205, 97), (1, 0)),
    opening((205, 141), (205, 153), (1, 0)),
    opening((205, 198), (205, 210), (1, 0)),
];

const PAGE1_WINDOWS: &[Opening] = &[
    opening((210, 6), (240, 6), (0, 1)),
    opening((295, 80), (295, 100), (-1, 0)),
    opening((295, 135), (295, 155), (-1, 0)),
    opening((295, 195), (295, 215), (-1, 0)),
];

impl Opening {
    /// White line over the whole gap.
    pub fn closed(&self) -> Line {
        to_line(self.hinge, self.end)
    }

    /// The gap one pixel in from both ends, drawn black to erase `closed`
    /// without eating the wall ends.
    pub fn gap(&self) -> Line {
        let (ax, ay) = self.along();
        to_line(
            (self.hinge.0 + ax, self.hinge.1 + ay),
            (self.end.0 - ax, self.end.1 - ay),
        )
    }

    /// The door or window swung 45 degrees open from its hinge.
    pub fn opened(&self) -> Line {
        let (ax, ay) = self.along();
        let length = (self.end.0 - self.hinge.0).abs() + (self.end.1 - self.hinge.1).abs();
        let reach = length as f32 * FRAC_1_SQRT_2;
        let opened = (
            self.hinge.0 + roundf((ax + self.swing.0) as f32 * reach) as i32,
            self.hinge.1 + roundf((ay + self.swing.1) as f32 * reach) as i32,
        );
        to_line(self.hinge, opened)
    }

    fn along(&self) -> (i32, i32) {
        (
            (self.end.0 - self.hinge.0).signum(),
            (self.end.1 - self.hinge.1).signum(),
        )
    }
}

fn to_line(from: (i32, i32), to: (i32, i32)) -> Line {
    line(from.0 as u16, from.1 as u16, to.0 as u16, to.1 as u16)
}

/// Walls and staircase for a page, drawn once after the page loads. Empty
/// Vec for an unknown page.
pub fn floor_plan(page: u8) -> Vec<Line> {
    let walls = match page {
        0 => PAGE0_WALLS,
        1 => PAGE1_WALLS,
        _ => return Vec::new(),
    };
    let mut lines = walls.to_vec();
    lines.extend(stairs());
    lines
}

/// Doors of a page, index i is door i of that floor. Empty slice for an
/// unknown page.
pub fn doors(page: u8) -> &'static [Opening] {
    match page {
        0 => PAGE0_DOORS,
        1 => PAGE1_DOORS,
        _ => &[],
    }
}

/// Windows of a page, index i is window i of that floor. Empty slice for an
/// unknown page.
pub fn windows(page: u8) -> &'static [Opening] {
    match page {
        0 => PAGE0_WINDOWS,
        1 => PAGE1_WINDOWS,
        _ => &[],
    }
}

fn stairs() -> Vec<Line> {
    let (left, right) = (STAIRS_LEFT as u16, STAIRS_RIGHT as u16);
    let (top, bottom) = (STAIRS_TOP as u16, STAIRS_BOTTOM as u16);
    let mut lines = Vec::from([
        line(left, top, right, top),
        line(left, top, left, bottom),
        line(left, bottom, right, bottom),
    ]);

    // Winders fan out from the inner corner in equal angle steps, from the
    // opening (vertical) to the first straight tread (horizontal).
    let step = 90.0 / STAIRS_WINDERS as f32;
    for i in (1..STAIRS_WINDERS).rev() {
        lines.push(winder(STAIRS_TOP_PIVOT, -(i as f32) * step));
    }
    let straight_top = STAIRS_TOP_PIVOT.1 as u16;
    let straight_bottom = STAIRS_BOTTOM_PIVOT.1 as u16;
    for y in (straight_top..=straight_bottom).step_by(STAIRS_STEP) {
        lines.push(line(right, y, left, y));
    }
    for i in 1..STAIRS_WINDERS {
        lines.push(winder(STAIRS_BOTTOM_PIVOT, i as f32 * step));
    }
    lines
}

/// Line from the pivot to wherever the ray hits the left, top or bottom
/// wall. Negative angles point up-left.
fn winder(pivot: (f32, f32), degrees: f32) -> Line {
    let (px, py) = pivot;
    let radians = degrees * PI / 180.0;
    let (dx, dy) = (-cosf(radians), sinf(radians));
    let mut distance = (STAIRS_LEFT - px) / dx;
    if dy > 0.0 {
        distance = distance.min((STAIRS_BOTTOM - py) / dy);
    } else if dy < 0.0 {
        distance = distance.min((STAIRS_TOP - py) / dy);
    }
    line(
        roundf(px) as u16,
        roundf(py) as u16,
        roundf(px + dx * distance) as u16,
        roundf(py + dy * distance) as u16,
    )
}
