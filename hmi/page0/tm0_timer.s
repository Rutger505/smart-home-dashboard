// Timer tm0 (tim=100, en=1). The living room is an L, drawn as two text
// components. The firmware only colors room_0 (long arm), so this copies
// its light color to room_0x (short arm).
if(room_0x.bco!=room_0.bco)
{
  room_0x.bco=room_0.bco
}
