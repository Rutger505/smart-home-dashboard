# AGENTS.md

## Project Overview

Smart home dashboard built with ESP32 (Rust, esp-hal bare metal) and Nextion touch screen display. The system monitors and controls household devices: lights, doors, windows, temperature sensors, and washing machine status.

## Hardware

- **Microcontroller:** ESP32 (bare metal Rust via esp-hal)
- **Display:** Nextion Enhanced NX4024K032 HMI Display 3.2 Inch 400x240 with Touchscreen
- **Communication:** UART serial between ESP32 and Nextion display
- **Sensors:** DHT11 (temperature), Reed switches (door/window detection)

## Display Programming

The Nextion display is programmed **manually** using the Nextion Editor software, not through code generation. All visual elements, page layouts, and touch interactions are designed in the editor.

## File Structure

```
hmi/
├── interface.HMI         # Nextion Editor project file
├── default.zi            # Pre-compiled display resources
├── page0/
│   ├── post_init.s        # Post-initialization state
│   └── runtime.s          # Runtime state changes
└── README.md              # File organization notes
```

## Page 0 Initialization

All code and component states for page 0 **must** be saved in the `hmi/page0/` folder after post-initialization. This includes:

- Initial component states (labels, buttons, gauges)
- Layout configurations
- Touch event handlers
- Serial command templates
