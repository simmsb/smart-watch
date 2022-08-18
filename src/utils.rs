use std::sync::{Arc, Mutex};

use esp_idf_hal::gpio::{Gpio21, Gpio22, InputOutput};
use esp_idf_hal::i2c::{I2cError, Master, I2C0};

#[derive(Clone)]
pub struct I2c0(Arc<Mutex<Master<I2C0, Gpio21<InputOutput>, Gpio22<InputOutput>>>>);

impl I2c0 {
    pub fn new(inner: Master<I2C0, Gpio21<InputOutput>, Gpio22<InputOutput>>) -> Self {
        Self(Arc::new(Mutex::new(inner)))
    }
}

impl eh_0_2::blocking::i2c::Write for I2c0 {
    type Error = I2cError;

    fn write(&mut self, address: u8, bytes: &[u8]) -> Result<(), Self::Error> {
        self.0.lock().unwrap().write(address, bytes)
    }
}

impl eh_0_2::blocking::i2c::Read for I2c0 {
    type Error = I2cError;

    fn read(&mut self, address: u8, buffer: &mut [u8]) -> Result<(), Self::Error> {
        self.0.lock().unwrap().read(address, buffer)
    }
}

impl eh_0_2::blocking::i2c::WriteRead for I2c0 {
    type Error = I2cError;

    fn write_read(
        &mut self,
        address: u8,
        bytes: &[u8],
        buffer: &mut [u8],
    ) -> Result<(), Self::Error> {
        self.0.lock().unwrap().write_read(address, bytes, buffer)
    }
}
