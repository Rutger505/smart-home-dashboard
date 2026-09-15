#![no_std]
#![no_main]

extern crate alloc;

use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Instant, Ticker, Timer};
use esp_backtrace as _;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::rng::Rng;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::uart::{Config, Uart};
use log::{LevelFilter, info};
use smart_home_dashboard::logger;
use smart_home_dashboard::nextion::Screen;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    logger::init(LevelFilter::Debug);

    let peripherals = esp_hal::init(esp_hal::Config::default());
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 98768);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

    let display_uart = Uart::new(peripherals.UART2, Config::default().with_baudrate(9600))
        .expect("Could not initialize UART for display")
        .with_tx(peripherals.GPIO17)
        .with_rx(peripherals.GPIO16)
        .into_async();

    spawner.spawn(display(Screen::new(display_uart)).unwrap());

    loop {
        Timer::after(Duration::from_secs(60)).await;
        info!("Uptime: {}s", Instant::now().as_secs());
    }
}

#[embassy_executor::task]
async fn display(mut screen: Screen<'static>) {
    let rng = Rng::new();

    let mut update_screen_ticker = Ticker::every(Duration::from_millis(100));

    loop {
        match select(screen.process(), update_screen_ticker.next()).await {
            Either::First(()) => {}
            Either::Second(()) => screen.update(rng.random() as u8).await,
        }
    }
}
