use color_eyre::eyre::eyre;
use eos::DateTime;
use tracing::info;

use crate::utils::I2c0;

pub struct EspRtc {
    rtc: pcf8563::PCF8563<I2c0>,
}

impl EspRtc {
    pub fn new(i2c: I2c0) -> color_eyre::Result<Self> {
        let mut rtc = pcf8563::PCF8563::new(i2c);
        rtc.rtc_init()
            .map_err(|e| eyre!("Failed to reset RTC: {:?}", e))?;

        Ok(Self { rtc })
    }

    pub fn set(&mut self, dt: eos::DateTime) -> color_eyre::Result<()> {
        let (year, century) = if dt.year() > 1999 {
            (dt.year() - 2000, 1)
        } else {
            (dt.year() - 1900, 0)
        };
        let dt_ = pcf8563::DateTime {
            year: year as u8,
            month: dt.month() as u8,
            weekday: dt.weekday().number_from_monday(),
            day: dt.day(),
            hours: dt.hour(),
            minutes: dt.minute(),
            seconds: dt.second(),
        };
        info!(eos_dt = ?dt, dt = ?dt_, "setting RTC");
        self.rtc
            .set_datetime(&dt_)
            .map_err(|err| eyre!("Couldn't set RTC: {:?}", err))?;
        self.rtc
            .set_century_flag(century)
            .map_err(|err| eyre!("Couldn't set RTC: {:?}", err))?;
        Ok(())
    }

    pub fn read(&mut self) -> color_eyre::Result<DateTime> {
        let dt = self
            .rtc
            .get_datetime()
            .map_err(|e| eyre!("Failed to fetch datetime from RTC: {:?}", e))?;
        let century = self
            .rtc
            .get_century_flag()
            .map_err(|e| eyre!("Failed to fetch datetime from RTC: {:?}", e))?;

        let century = if century == 0 { 1900 } else { 2000 };

        let out_dt = DateTime::new(dt.year as i16 + century, dt.month, dt.day)
            .ok_or_else(|| eyre!("Couldn't build a DateTime from {:?}", dt))?
            .with_time(
                eos::Time::new(dt.hours, dt.minutes, dt.seconds)
                    .ok_or_else(|| eyre!("Couldn't build a Time from {:?}", dt))?,
            );

        Ok(out_dt)
    }
}
