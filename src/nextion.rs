use alloc::collections::VecDeque;
use alloc::format;
use alloc::vec::Vec;
use esp_hal::Async;
use esp_hal::uart::{IoError, Uart};
use log::{debug, error, info, warn};

const COMMAND_TERMINATOR: [u8; 3] = [0xFF; 3];

const TOUCH_EVENT_ID: u8 = 0x65;

pub struct Screen<'a> {
    uart: Uart<'a, Async>,
    rx_buf: [u8; 128],
    commands: VecDeque<u8>,
    page: u8,
}

impl<'a> Screen<'a> {
    pub fn new(uart: Uart<'a, Async>) -> Self {
        Self {
            uart,
            rx_buf: [0; 128],
            commands: VecDeque::new(),
            page: 0,
        }

        // TODO: set actual page id.
    }

    pub async fn read(&mut self) {
        match self.uart.read_async(&mut self.rx_buf).await {
            Ok(size) => {
                debug!("Rx Data: {:X?}", &self.rx_buf[0..size]);

                self.commands.extend(&self.rx_buf[0..size]);
            }
            // read() already cleared the error flags, so logging is enough
            Err(e) => error!("UART Rx Error: {:?}", e),
        }
    }

    pub async fn process(&mut self) {
        self.read().await;

        if self.commands.is_empty() {
            return;
        }

        let Some(end) = self.find_terminator() else {
            return;
        };

        let mut command: Vec<u8> = self.commands.drain(..end).collect();
        self.commands.drain(..COMMAND_TERMINATOR.len());

        let Some((command_type, command_data)) = command.split_first() else {
            return;
        };

        match *command_type {
            TOUCH_EVENT_ID => self.process_touch(command_data).await,
            _ => {
                warn!("Command type not implemented")
            }
        }
    }

    fn find_terminator(&mut self) -> Option<usize> {
        self.commands
            .make_contiguous()
            .windows(COMMAND_TERMINATOR.len())
            .position(|w| w == COMMAND_TERMINATOR)
    }

    async fn process_touch(&mut self, data: &[u8]) {
        let page = data[0];
        let component_id = data[1];
        const PRESS_EVENT: u8 = 1;
        const RELEASE_EVENT: u8 = 0;
        let event = data[2];
        if event != RELEASE_EVENT && event != PRESS_EVENT {
            warn!("Event not present RELEASE_EVENT, or PRESS_EVENT, returning");
            return;
        }

        let ground_floor_button = 2;
        let second_floor_button = 1;

        self.set_page(if component_id == ground_floor_button {
            0
        } else if component_id == second_floor_button {
            1
        } else {
            0
        })
        .await;
    }

    pub async fn update(&mut self, room_temp: u8) {
        // TODO: Proper update method to carry information on which page and component data should be updated.
        return;

        let command = format!("t1.txt=\"{}\"", room_temp);

        if let Err(err) = self.send(command.as_bytes()).await {
            error!("Tx Error: {:?}", err);
            return;
        }

        debug!("Written data: {}", command);
    }

    async fn set_page(&mut self, page: u8) {
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

    async fn send(&mut self, command: &[u8]) -> Result<(), IoError> {
        self.uart.write_async(command).await?;
        self.uart.write_async(&COMMAND_TERMINATOR).await?;
        self.uart.flush_async().await?;

        Ok(())
    }
}
