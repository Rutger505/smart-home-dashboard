// Second floor (400x240 display), same footprint as page0.
// Top bedroom spans the full width (155,6 to 295,63). Below it, the
// corridor is a vertical strip (155,63 to 205,233, 50 wide x 170
// tall) reaching the other 3 bedrooms, stacked on the right
// (205,63 to 295,233), each opening into the corridor through its
// own door. Each bedroom has one window. Staircase box is unchanged
// from page0: same position, size, and opening into the corridor.

// Outer top wall, split for the top bedroom's window (210 to 240)
line 155,6,210,6,65535
line 240,6,295,6,65535

// Outer right wall, split for a window in each of the 3 lower
// bedrooms (B: 80-100, C: 135-155, D: 195-215)
line 295,6,295,63,65535
line 295,63,295,80,65535
line 295,100,295,119,65535
line 295,119,295,135,65535
line 295,155,295,176,65535
line 295,176,295,195,65535
line 295,215,295,233,65535

// Outer bottom wall (corridor + bottom bedroom, solid, no exterior
// door on this floor)
line 155,233,295,233,65535

// Corridor outer left wall, split for the staircase opening (163 to 213)
line 155,6,155,163,65535
line 155,213,155,233,65535

// Top bedroom / corridor dividing wall (y=63), split for its door
// (170 to 182). Right side (205 to 295) is solid: bedroom B doesn't
// connect directly to the top bedroom, only via the corridor.
line 155,63,170,63,65535
line 182,63,205,63,65535
line 205,63,295,63,65535

// Bedroom / corridor dividing wall (x=205), split for the 3 lower
// bedroom doors (B: 85-97, C: 141-153, D: 198-210)
line 205,63,205,85,65535
line 205,97,205,141,65535
line 205,153,205,198,65535
line 205,210,205,233,65535

// Bedroom / bedroom dividing walls (solid, rooms don't connect to each other)
line 205,119,295,119,65535
line 205,176,295,176,65535

// Door leaves, each swinging open into the bedroom it enters
line 170,63,182,51,65535
line 205,85,217,97,65535
line 205,141,217,153,65535
line 205,198,217,210,65535

// Window leaves, all swinging open outward (out of the house)
line 210,6,240,0,65535
line 295,80,315,100,65535
line 295,135,315,155,65535
line 295,195,315,215,65535

// Staircase module, unchanged from page0: same box, same opening
// into the corridor
line 105,163,155,163,65535
line 105,163,105,213,65535
line 105,213,155,213,65535

// Stair treads, unchanged from page0
line 113,168,147,168,65535
line 113,178,147,178,65535
line 113,188,147,188,65535
line 113,198,147,198,65535
line 113,208,147,208,65535
