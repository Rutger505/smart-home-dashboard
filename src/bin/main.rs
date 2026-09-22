#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use defmt::{debug, trace, warn};
use dht_sensor::DhtError;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Ticker, Timer};
use esp_backtrace as _;
use esp_hal::delay::Delay;
use esp_hal::gpio::{DriveMode, Flex, Input, InputConfig, OutputConfig, Pull};
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::uart::{Config, Uart};
use esp_println as _;
use smart_home_dashboard::display::display::Display;
use smart_home_dashboard::display::hmi::Hmi;
use smart_home_dashboard::display::nextion::Nextion;
use smart_home_dashboard::floor::Floor;
use smart_home_dashboard::sensors::dht11::Dht11;
use smart_home_dashboard::sensors::ky024::Ky024;

esp_bootloader_esp_idf::esp_app_desc!();

const LOG_BAUD_RATE: u32 = 921600;

static SENSOR_DATA_SIGNAL: Signal<CriticalSectionRawMutex, [Floor; 2]> = Signal::new();

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 98768);

    let peripherals = esp_hal::init(esp_hal::Config::default());

    // esp-println writes to the UART0 registers directly and keeps the ROM's
    // 115200 baud unless something reconfigures it. Keep this alive, because
    // dropping it may turn the peripheral off.
    let _log_uart = Uart::new(
        peripherals.UART0,
        Config::default().with_baudrate(LOG_BAUD_RATE),
    )
    .expect("Could not initialize UART for logging")
    .with_tx(peripherals.GPIO1)
    .with_rx(peripherals.GPIO3);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);
    trace!("esp-rtos started");

    let display_uart = Uart::new(peripherals.UART2, Config::default().with_baudrate(921600))
        .expect("Could not initialize UART for display")
        .with_tx(peripherals.GPIO17)
        .with_rx(peripherals.GPIO16)
        .into_async();
    trace!("Display UART ready");

    let mut nextion = Nextion::new(display_uart).await;
    let page = nextion.get_page().await;
    trace!("Nextion ready on page {}", page);

    let display = Display::new(nextion, page);
    let sensors = Sensors {
        dht11: Dht11::new(open_drain(peripherals.GPIO4.into())),
        doors: Ky024::new(Input::new(peripherals.GPIO25, InputConfig::default())),
        windows: Ky024::new(Input::new(peripherals.GPIO26, InputConfig::default())),
        light_switch: Input::new(
            peripherals.GPIO27,
            InputConfig::default().with_pull(Pull::Up),
        ),
    };

    spawner.spawn(display_task(display).unwrap());
    spawner.spawn(sensor_data_task(sensors).unwrap());
    trace!("Tasks spawned");

    loop {
        Timer::after_secs(1).await;
    }
}

struct Sensors {
    dht11: Dht11<Flex<'static>>,
    doors: Ky024<'static>,
    windows: Ky024<'static>,
    /// Switch between the pin and GND, so closed reads low.
    light_switch: Input<'static>,
}

// The DHT11 data line is bidirectional: the ESP pulls it low to start a
// reading, then releases it and listens.
fn open_drain(pin: esp_hal::gpio::AnyPin<'static>) -> Flex<'static> {
    let mut flex = Flex::new(pin);
    flex.apply_output_config(
        &OutputConfig::default()
            .with_drive_mode(DriveMode::OpenDrain)
            .with_pull(Pull::Up),
    );
    flex.set_high();
    flex.set_output_enable(true);
    flex.set_input_enable(true);
    flex
}

#[embassy_executor::task]
async fn sensor_data_task(mut sensors: Sensors) {
    let mut delay = Delay::new();
    let mut temperature = None;

    // The DHT11 needs at least a second between reads.
    let mut poll_sensor_ticker = Ticker::every(Duration::from_millis(300));

    loop {
        poll_sensor_ticker.next().await;
        trace!("Sensor tick");

        match sensors.dht11.read(&mut delay) {
            Ok(reading) => temperature = Some(reading.temperature.max(0) as u8),
            Err(DhtError::Timeout) => warn!("DHT11 timeout"),
            Err(DhtError::ChecksumMismatch) => warn!("DHT11 checksum mismatch"),
            Err(DhtError::PinError(_)) => warn!("DHT11 pin error"),
        }

        let door_closed = sensors.doors.detects_magnet();
        let window_closed = sensors.windows.detects_magnet();
        let light_on = sensors.light_switch.is_low();

        let temperatures = |count: usize| -> Vec<u8> {
            temperature.map_or_else(Vec::new, |temperature| vec![temperature; count])
        };

        let data = [
            Floor {
                temperatures: temperatures(2),
                doors: vec![door_closed; 2],
                windows: vec![window_closed; 2],
                lights: vec![light_on; 2],
            },
            Floor {
                temperatures: temperatures(5),
                doors: vec![door_closed; 4],
                windows: vec![window_closed; 4],
                lights: vec![light_on; 5],
            },
        ];

        debug!("Sending data to display_task: {:?}", data);

        SENSOR_DATA_SIGNAL.signal(data);
    }
}

#[embassy_executor::task]
async fn display_task(mut display: Display<Nextion<'static>>) {
    display.draw_floor_plan().await;

    loop {
        trace!("display_task waiting for touch or sensor data");
        match select(display.next_touch_event(), SENSOR_DATA_SIGNAL.wait()).await {
            Either::First(event) => {
                trace!("display_task got touch event");
                display.handle_touch_event(event).await;
            }
            Either::Second(data) => {
                trace!("display_task got sensor data");
                display.render(&data).await;
            }
        }
    }
}
