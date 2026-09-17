use crate::display::hmi::Hmi;
use crate::display::touch_event::TouchEvent;
use alloc::collections::VecDeque;
use alloc::format;
use alloc::vec::Vec;
use esp_hal::Async;
use esp_hal::uart::{IoError, Uart};
use log::{debug, error, info};


const COMMAND_TERMINATOR: [u8; 3] = [0xFF; 3];

const TOUCH_EVENT_ID: u8 = 0x65;
const SENDME_EVENT_ID: u8 = 0x66;

const RELEASE_EVENT: u8 = 0;

pub struct Nextion<'a> {
    uart: Uart<'a, Async>,
    rx_buf: [u8; 128],
    commands: VecDeque<u8>,
    page: u8,
}

impl<'a> Nextion<'a> {
    pub async fn new(uart: Uart<'a, Async>) -> Self {
        let mut instance = Self {
            uart,
            rx_buf: [0; 128],
            commands: VecDeque::new(),
            page: 0,
        };

        if let Some(actual_page) = instance.get_page_from_nextion().await {
            instance.page = actual_page;
        }

        instance
    }

    async fn get_page_from_nextion(&mut self) -> Option<u8> {
        loop {
            match self.send(b"sendme").await {
                Ok(()) => break,
                Err(e) => {
                    error!("Unable to send 'sendme' command, retrying. Error: {e}", );
                }
            }
        }

        let page;

        loop {
            self.read().await;

            if self.commands.is_empty() {
                continue;
            }

            let Some(end) = self.find_terminator() else {
                continue;
            };

            let command: Vec<u8> = self.commands.drain(..end).collect();
            self.commands.drain(..COMMAND_TERMINATOR.len());

            if command[0] == SENDME_EVENT_ID {
                page = command[1];
                break;
            }
        }

        Some(page)
    }

    async fn read(&mut self) {
        match self.uart.read_async(&mut self.rx_buf).await {
            Ok(size) => self.commands.extend(&self.rx_buf[0..size]),

            Err(e) => error!("UART Rx Error: {:?}", e),
        }
    }

    fn find_terminator(&mut self) -> Option<usize> {
        self.commands
            .make_contiguous()
            .windows(COMMAND_TERMINATOR.len())
            .position(|w| w == COMMAND_TERMINATOR)
    }


    async fn send(&mut self, command: &[u8]) -> Result<(), IoError> {
        self.uart.write_async(command).await?;
        self.uart.write_async(&COMMAND_TERMINATOR).await?;
        self.uart.flush_async().await?;

        Ok(())
    }
}

impl Hmi for Nextion<'_> {
    async fn get_page(&mut self) -> u8 {
        self.page
    }
    async fn show_page(&mut self, page: u8) {
        if self.page == page {
            return;
        }

        let command = format!("page {}", page);

        if let Err(err) = self.send(command.as_bytes()).await {
            error!("Tx Error: {:?}", err);
            return;
        }

        self.page = page;

        debug!("Written data: {}", command);
        info!("Page set to: {}", page);
    }

    async fn show_value(&mut self, component_name: &str, value: &str) {
        if !value.is_ascii() {
            error!("Only ascii allowed, value passed: {}", value);
            return;
        }

        let command = format!("{component_name}.txt=\"{value}\"");

        if let Err(err) = self.send(command.as_bytes()).await {
            error!("Tx Error: {:?}", err);
            return;
        }

        debug!("Written data: {}", command);
    }

    async fn next_touch_event(&mut self) -> TouchEvent {
        let command_data: [u8; 3];

        loop {
            self.read().await;

            if self.commands.is_empty() {
                continue;
            }

            let Some(end) = self.find_terminator() else {
                continue;
            };

            let command: Vec<u8> = self.commands.drain(..end).collect();
            self.commands.drain(..COMMAND_TERMINATOR.len());

            if command[0] == TOUCH_EVENT_ID {
                command_data = [command[1], command[2], command[3]];
                break;
            }
        }

        TouchEvent {
            page: command_data[0],
            component_id: command_data[1],
            pressed: command_data[2] != RELEASE_EVENT,
        }
    }
}
