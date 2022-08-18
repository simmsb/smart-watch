use eos::DateTime;

use crate::utils::I2c0;

pub struct EspRtc {
    rtc: pcf8563::PCF8563<I2c0>,
}

impl EspRtc {
    pub fn new(i2c: I2c0) -> color_eyre::eyre::Result<Self> {
        let mut rtc = pcf8563::PCF8563::new(i2c);
        rtc.rtc_init()
            .map_err(|e| color_eyre::eyre::eyre!("Failed to reset RTC: {:?}", e))?;

        Ok(Self { rtc })
    }

    pub fn read(&mut self) -> color_eyre::Result<DateTime> {
        let dt = self
            .rtc
            .get_datetime()
            .map_err(|e| color_eyre::eyre::eyre!("Failed to fetch datetime from RTC: {:?}", e))?;

        let out_dt = DateTime::new(dt.year as i16, dt.month, dt.day)
            .ok_or_else(|| color_eyre::eyre::eyre!("Couldn't build a DateTime from {:?}", dt))?
            .with_time(
                eos::Time::new(dt.hours, dt.minutes, dt.seconds).ok_or_else(|| {
                    color_eyre::eyre::eyre!("Couldn't build a Time from {:?}", dt)
                })?,
            );

        Ok(out_dt)
    }
}
