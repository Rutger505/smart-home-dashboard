use alloc::vec::Vec;
use alloc::{format, vec};
use esp_hal::uart::Uart;
use esp_hal::{Async, Blocking};
use esp_println::println;

pub struct Screen<'a> {
    uart: Uart<'a, Blocking>,
    tx_buf: [u8; 128],
    commands: Vec<u8>,
}

impl<'a> Screen<'a> {
    pub fn new(uart: Uart<'a, Blocking>) -> Self {
        Self {
            uart,
            tx_buf: [0; 128],
            commands: vec![],
        }
    }

    pub fn read(&mut self) {
        if !self.uart.read_ready() {
            return;
        }

        match self.uart.read(&mut self.tx_buf) {
            Ok(size) => {
                if size <= 0 {
                    return;
                }

                println!("Rx Data: {:X?}", &self.tx_buf[0..size]);

                self.commands.extend_from_slice(&self.tx_buf[0..size]);
            }
            // read() already cleared the error flags, so logging is enough
            Err(e) => println!("UART Rx Error: {:?}", e),
        }
    }

    pub fn process(&mut self) {
        for (i, byte) in self.commands.iter().enumerate() {
            println!("Command: {:02X} 0x{:02X}", i, byte);
        }
    }

    pub fn update(&mut self, room_temp: u8) {
        let command_string = format!("t1.txt=\"{}\"", room_temp);
        let mut command_bytes = command_string.clone().into_bytes();

        let finish_command = [0xFF; 3];
        command_bytes.extend_from_slice(&finish_command);

        match self.uart.write(&command_bytes) {
            Ok(_) => {
                println!("Written data: {}", command_string);
                // Flush TX to guarantee the write finishes before changing state
                let _ = self.uart.flush();
            }
            Err(err) => println!("Tx Error: {:?}", err),
        }
    }
}
