// House outline (400x240 display), centered on screen.
// Corridor is a vertical strip (155,126 to 205,233, 50 wide x 107 tall,
// 33% taller than before). Kitchen is an L: long arm vertical
// (205,6 to 295,233, full height), short arm horizontal, sitting on
// top of the corridor (155,6 to 205,126).
// Staircase is a separate small module bumped out to the left of the
// corridor, open into it.

// Outer top wall (spans the kitchen's short arm and long arm)
line 155,6,295,6,65535
// Outer right wall (kitchen's long arm)
line 295,6,295,233,65535

// Outer bottom wall, split for the main entrance door gap
line 155,233,160,233,65535
line 190,233,295,233,65535

// Corridor outer left wall, split for the staircase opening (163 to 213)
line 155,6,155,163,65535
line 155,213,155,233,65535

// Corridor / kitchen dividing walls
line 155,126,205,126,65535
line 205,126,205,233,65535

// Main entrance door leaf, swinging open into the corridor
line 160,233,190,208,65535

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
