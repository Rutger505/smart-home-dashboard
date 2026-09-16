#![no_std]
#![no_main]

extern crate alloc;

use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Ticker, Timer};
use esp_backtrace as _;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::rng::Rng;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::uart::{Config, Uart};
use log::LevelFilter;
use smart_home_dashboard::display::display::Display;
use smart_home_dashboard::display::nextion::Nextion;
use smart_home_dashboard::floor::Floor;
use smart_home_dashboard::logger;

esp_bootloader_esp_idf::esp_app_desc!();

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
    let nextion = Nextion::new(display_uart);

    // TODO set actual page
    let display = Display::new(nextion, 0);
    spawner.spawn(display_task(display).unwrap());

    loop {
        Timer::after_secs(1).await;
    }
}

#[embassy_executor::task]
async fn display_task(mut display: Display<Nextion<'static>>) {
    let rng = Rng::new();

    let mut update_screen_ticker = Ticker::every(Duration::from_millis(100));

    loop {
        match select(display.next_touch_event(), update_screen_ticker.next()).await {
            Either::First(event) => {
                display.handle_touch_event(event).await;
            }
            Either::Second(()) => display.render(&[Floor {
                temperatures: &[rng.random() as u8],
                doors: &[false],
                windows: &[true],
                lights: &[false],
            }]).await,
        }
    }
}
