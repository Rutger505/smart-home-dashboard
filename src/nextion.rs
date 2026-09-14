use alloc::format;
use alloc::vec::Vec;
use esp_hal::Async;
use esp_hal::uart::{IoError, Uart};
use esp_println::println;

const COMMAND_TERMINATOR: [u8; 3] = [0xFF; 3];

pub struct Screen<'a> {
    uart: Uart<'a, Async>,
    rx_buf: [u8; 128],
    commands: Vec<u8>,
}

impl<'a> Screen<'a> {
    pub fn new(uart: Uart<'a, Async>) -> Self {
        Self {
            uart,
            rx_buf: [0; 128],
            commands: Vec::new(),
        }
    }

    pub async fn read(&mut self) {
        match self.uart.read_async(&mut self.rx_buf).await {
            Ok(size) => {
                println!("Rx Data: {:X?}", &self.rx_buf[0..size]);

                self.commands.extend_from_slice(&self.rx_buf[0..size]);
            }
            // read() already cleared the error flags, so logging is enough
            Err(e) => println!("UART Rx Error: {:?}", e),
        }
    }

    pub async fn process(&mut self) {
        self.read().await;

        for (i, byte) in self.commands.iter().enumerate() {
            println!("Command: {:02X} 0x{:02X}", i, byte);
        }
    }

    pub async fn update(&mut self, room_temp: u8) {
        let command = format!("t1.txt=\"{}\"", room_temp);

        if let Err(err) = self.send(command.as_bytes()).await {
            println!("Tx Error: {:?}", err);
            return;
        }

        println!("Written data: {}", command);
    }

    async fn send(&mut self, command: &[u8]) -> Result<(), IoError> {
        self.uart.write_async(command).await?;
        self.uart.write_async(&COMMAND_TERMINATOR).await?;
        self.uart.flush_async().await?;

        Ok(())
    }
}
