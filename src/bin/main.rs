#![no_std]
#![no_main]

extern crate alloc;

use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Ticker, Timer};
use esp_backtrace as _;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::rng::Rng;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::uart::{Config, Uart};
use log::LevelFilter;
use smart_home_dashboard::display::display::Display;
use smart_home_dashboard::display::hmi::Hmi;
use smart_home_dashboard::display::nextion::Nextion;
use smart_home_dashboard::floor::Floor;
use smart_home_dashboard::logger;

esp_bootloader_esp_idf::esp_app_desc!();


static SENSOR_DATA_SIGNAL: Signal<CriticalSectionRawMutex, [Floor; 2]> = Signal::new();

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    logger::init(LevelFilter::Debug);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 98768);

    let peripherals = esp_hal::init(esp_hal::Config::default());

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);


    let display_uart = Uart::new(peripherals.UART2, Config::default().with_baudrate(9600))
        .expect("Could not initialize UART for display")
        .with_tx(peripherals.GPIO17)
        .with_rx(peripherals.GPIO16)
        .into_async();
    let mut nextion = Nextion::new(display_uart).await;
    let page = nextion.get_page().await;
    let display = Display::new(nextion, page);
    spawner.spawn(display_task(display).unwrap());
    spawner.spawn(sensor_data_task().unwrap());

    loop {
        Timer::after_secs(1).await;
    }
}


#[embassy_executor::task]
async fn sensor_data_task() {
    let rng = Rng::new();

    let mut poll_sensor_ticker = Ticker::every(Duration::from_secs(1));

    loop {
        poll_sensor_ticker.next().await;


        let temperatures = |count: usize| (0..count).map(|_| rng.random() as u8).collect();
        let states = |count: usize| (0..count).map(|_| rng.random() / 2 % 2 == 0).collect();

        let data = [
            Floor {
                temperatures: temperatures(1),
                doors: states(2),
                windows: states(2),
                lights: states(2),
            },
            Floor {
                temperatures: temperatures(4),
                doors: states(4),
                windows: states(4),
                lights: states(4),
            }];

        SENSOR_DATA_SIGNAL.signal(data);
    }
}

#[embassy_executor::task]
async fn display_task(mut display: Display<Nextion<'static>>) {
    loop {
        match select(display.next_touch_event(), SENSOR_DATA_SIGNAL.wait()).await {
            Either::First(event) => {
                display.handle_touch_event(event).await;
            }
            Either::Second(data) => display.render(&data).await,
        }
    }
}
