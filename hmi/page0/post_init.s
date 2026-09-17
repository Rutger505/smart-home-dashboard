// House outline (400x240 display), centered on screen.
// Corridor is a vertical strip (155,126 to 205,233, 50 wide x 107 tall,
// 33% taller than before). Living room is an L: long arm vertical
// (205,6 to 295,233, full height), short arm horizontal, sitting on
// top of the corridor (155,6 to 205,126).
// Staircase is a separate small module bumped out to the left of the
// corridor, open into it.
// Door and window gaps stay open here. Small door_N and window_N text
// components cover them, and the firmware colors them white (closed) or
// red (open).

// Outer top wall (spans the living room's short arm and long arm),
// split for a window (210 to 240)
line 155,6,210,6,65535
line 240,6,295,6,65535
// Outer right wall (living room's long arm), split for a window (100 to 130)
line 295,6,295,100,65535
line 295,130,295,233,65535

// Outer bottom wall, split for the main entrance door gap
line 155,233,160,233,65535
line 190,233,295,233,65535

// Corridor outer left wall, split for the staircase opening (163 to 213)
line 155,6,155,163,65535
line 155,213,155,233,65535

// Corridor / living room dividing walls, split for a door (167 to 191)
line 155,126,205,126,65535
line 205,126,205,167,65535
line 205,191,205,233,65535

// Staircase module, small box bumped out to the left of the corridor,
// 50 tall, open on its right side into the corridor
line 105,163,155,163,65535
line 105,163,105,213,65535
line 105,213,155,213,65535

// Stair treads, starting a bit clear of the outer wall (x=113) so the
// first step doesn't blend into the wall line at x=105
line 113,168,147,168,65535
line 113,178,147,178,65535
line 113,188,147,188,65535
line 113,198,147,198,65535
line 113,208,147,208,65535
