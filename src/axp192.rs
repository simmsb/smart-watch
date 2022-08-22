#![allow(dead_code)]

use std::sync::atomic::AtomicU8;
use std::thread::JoinHandle;
use std::time::Duration;

use embedded_hal::i2c::blocking::I2c;

use crate::utils::I2c0;

pub static BATTERY_PERCENT: AtomicU8 = AtomicU8::new(0);

#[derive(Clone)]
pub struct Axp192 {
    inner: I2c0,
}

const ADDR: u8 = 0x34;
const POWER_STATUS: u8 = 0x00;
const MODE_CHARGING_STATUS: u8 = 0x01;

const EXTEN_DCDC2_CTRL: u8 = 0x10;
const EXTEN_DCDC2_CTRL_EXTEN: u8 = 0b0000_0100;
const EXTEN_DCDC2_CTRL_DCDC2: u8 = 0b0000_0001;

const DCDC13_LDO23_CTRL: u8 = 0x12;
const DCDC13_LDO23_CTRL_LDO3: u8 = 0b0000_1000;
const DCDC13_LDO23_CTRL_LDO2: u8 = 0b0000_0100;
const DCDC13_LDO23_CTRL_DCDC3: u8 = 0b0000_0010;
const DCDC13_LDO23_CTRL_DCDC1: u8 = 0b0000_0001;

const LDO23_OUT_VOLTAGE: u8 = 0x28;
const LDO23_OUT_VOLTAGE_LDO2_3_0V: u8 = 0b1100_0000;
const LDO23_OUT_VOLTAGE_LDO2_MASK: u8 = 0b1111_0000;
const LDO23_OUT_VOLTAGE_LDO3_3_0V: u8 = 0b0000_1100;
const LDO23_OUT_VOLTAGE_LDO3_MASK: u8 = 0b0000_1111;

const VBUS_IPSOUT: u8 = 0x30;
const VBUS_IPSOUT_IGNORE_VBUSEN: u8 = 0b1000_0000;
const VBUS_IPSOUT_VHOLD_LIMIT: u8 = 0b0100_0000;
const VBUS_IPSOUT_VHOLD_VOLTAGE_4_4V: u8 = 0b0010_0000;
const VBUS_IPSOUT_VHOLD_VOLTAGE_MASK: u8 = 0b0011_1000;
const VBUS_IPSOUT_VBUS_LIMIT_CURRENT: u8 = 0b0000_0010;
const VBUS_IPSOUT_VBUS_LIMIT_CURRENT_500MA: u8 = 0b0000_0001;
const VBUS_IPSOUT_VBUS_LIMIT_CURRENT_100MA: u8 = 0b0000_0000;

const POWER_OFF_VOLTAGE: u8 = 0x31;
const POWER_OFF_VOLTAGE_2_6V: u8 = 0b0000;
const POWER_OFF_VOLTAGE_2_7V: u8 = 0b0001;
const POWER_OFF_VOLTAGE_2_8V: u8 = 0b0010;
const POWER_OFF_VOLTAGE_2_9V: u8 = 0b0011;
const POWER_OFF_VOLTAGE_3_0V: u8 = 0b0100;
const POWER_OFF_VOLTAGE_3_1V: u8 = 0b0101;
const POWER_OFF_VOLTAGE_3_2V: u8 = 0b0110;
const POWER_OFF_VOLTAGE_3_3V: u8 = 0b0111;
const POWER_OFF_VOLTAGE_MASK: u8 = 0b0111;

const POWER_OFF_BATT_CHGLED_CTRL: u8 = 0x32;
const POWER_OFF_BATT_CHGLED_CTRL_OFF: u8 = 0b1000_0000;

const CHARGING_CTRL1: u8 = 0x33;
const CHARGING_CTRL1_ENABLE: u8 = 0b1000_0000;
const CHARGING_CTRL1_VOLTAGE_4_36V: u8 = 0b0110_0000;
const CHARGING_CTRL1_VOLTAGE_4_20V: u8 = 0b0100_0000;
const CHARGING_CTRL1_VOLTAGE_4_15V: u8 = 0b0010_0000;
const CHARGING_CTRL1_VOLTAGE_4_10V: u8 = 0b0000_0000;
const CHARGING_CTRL1_VOLTAGE_MASK: u8 = 0b0110_0000;
const CHARGING_CTRL1_CHARGING_THRESH_15PERC: u8 = 0b0001_0000;
const CHARGING_CTRL1_CHARGING_THRESH_10PERC: u8 = 0b0000_0000;
const CHARGING_CTRL1_CHARGING_THRESH_MASK: u8 = 0b0001_0000;
const CHARGING_CTRL1_CURRENT_100MA: u8 = 0b0000_0000;
const CHARGING_CTRL1_CURRENT_MASK: u8 = 0b0000_1111;

const CHARGING_CTRL2: u8 = 0x34;

const BACKUP_BATT: u8 = 0x35;
const BACKUP_BATT_CHARGING_ENABLE: u8 = 0b1000_0000;
const BACKUP_BATT_CHARGING_VOLTAGE_2_5V: u8 = 0b0110_0000;
const BACKUP_BATT_CHARGING_VOLTAGE_3_0V: u8 = 0b0010_0000;
const BACKUP_BATT_CHARGING_VOLTAGE_3_1V: u8 = 0b0000_0000;
const BACKUP_BATT_CHARGING_VOLTAGE_MASK: u8 = 0b0110_0000;
const BACKUP_BATT_CHARGING_CURRENT_400UA: u8 = 0b0000_0011;
const BACKUP_BATT_CHARGING_CURRENT_200UA: u8 = 0b0000_0010;
const BACKUP_BATT_CHARGING_CURRENT_100UA: u8 = 0b0000_0001;
const BACKUP_BATT_CHARGING_CURRENT_50UA: u8 = 0b0000_0000;
const BACKUP_BATT_CHARGING_CURRENT_MASK: u8 = 0b0000_0011;

const PEK: u8 = 0x36;
const PEK_SHORT_PRESS_1S: u8 = 0b1100_0000;
const PEK_SHORT_PRESS_512MS: u8 = 0b1000_0000;
const PEK_SHORT_PRESS_256MS: u8 = 0b0100_0000;
const PEK_SHORT_PRESS_128MS: u8 = 0b0000_0000;
const PEK_SHORT_PRESS_MASK: u8 = 0b1100_0000;
const PEK_LONG_PRESS_2_5S: u8 = 0b0011_0000;
const PEK_LONG_PRESS_2_0S: u8 = 0b0010_0000;
const PEK_LONG_PRESS_1_5S: u8 = 0b0001_0000;
const PEK_LONG_PRESS_1_0S: u8 = 0b0000_0000;
const PEK_LONG_PRESS_MASK: u8 = 0b0011_0000;
const PEK_LONG_PRESS_POWER_OFF: u8 = 0b0000_1000;
const PEK_PWROK_DELAY_64MS: u8 = 0b0000_0100;
const PEK_PWROK_DELAY_32MS: u8 = 0b0000_0000;
const PEK_PWROK_DELAY_MASK: u8 = 0b0000_0100;
const PEK_POWER_OFF_TIME_12S: u8 = 0b0000_0011;
const PEK_POWER_OFF_TIME_8S: u8 = 0b0000_0010;
const PEK_POWER_OFF_TIME_6S: u8 = 0b0000_0001;
const PEK_POWER_OFF_TIME_4S: u8 = 0b0000_0000;
const PEK_POWER_OFF_TIME_MASK: u8 = 0b0000_0011;

const BATT_TEMP_LOW_THRESH: u8 = 0x38;
const BATT_TEMP_HIGH_THRESH: u8 = 0x39;
const BATT_TEMP_HIGH_THRESH_DEFAULT: u8 = 0b1111_1100;

const IRQ_1_ENABLE: u8 = 0x40;
const IRQ_2_ENABLE: u8 = 0x41;
const IRQ_3_ENABLE: u8 = 0x42;
const IRQ_4_ENABLE: u8 = 0x43;
const IRQ_5_ENABLE: u8 = 0x4a;

const IRQ_1_STATUS: u8 = 0x44;
const IRQ_2_STATUS: u8 = 0x45;
const IRQ_3_STATUS: u8 = 0x46;
const IRQ_4_STATUS: u8 = 0x47;
const IRQ_5_STATUS: u8 = 0x4d;

const IRQ_3_PEK_SHORT_PRESS: u8 = 0b0000_0010;
const IRQ_3_PEK_LONG_PRESS: u8 = 0b0000_0001;

const ADC_ACIN_VOLTAGE_H: u8 = 0x56;
const ADC_ACIN_VOLTAGE_L: u8 = 0x57;
const ADC_ACIN_CURRENT_H: u8 = 0x58;
const ADC_ACIN_CURRENT_L: u8 = 0x59;
const ADC_VBUS_VOLTAGE_H: u8 = 0x5a;
const ADC_VBUS_VOLTAGE_L: u8 = 0x5b;
const ADC_VBUS_CURRENT_H: u8 = 0x5c;
const ADC_VBUS_CURRENT_L: u8 = 0x5d;
const ADC_INTERNAL_TEMP_H: u8 = 0x5e;
const ADC_INTERNAL_TEMP_L: u8 = 0x5f;

const ADC_BATT_VOLTAGE_H: u8 = 0x78;
const ADC_BATT_VOLTAGE_L: u8 = 0x79;

const ADC_BATT_POWER_H: u8 = 0x70;
const ADC_BATT_POWER_M: u8 = 0x71;
const ADC_BATT_POWER_L: u8 = 0x72;

const ADC_BATT_CHARGE_CURRENT_H: u8 = 0x7a;
const ADC_BATT_CHARGE_CURRENT_L: u8 = 0x7b;
const ADC_BATT_DISCHARGE_CURRENT_H: u8 = 0x7c;
const ADC_BATT_DISCHARGE_CURRENT_L: u8 = 0x7d;
const ADC_APS_VOLTAGE_H: u8 = 0x7e;
const ADC_APS_VOLTAGE_L: u8 = 0x7f;

const ADC_ENABLE_1: u8 = 0x82;
const ADC_ENABLE_1_BATT_VOL: u8 = 0b1000_0000;
const ADC_ENABLE_1_BATT_CUR: u8 = 0b0100_0000;
const ADC_ENABLE_1_ACIN_VOL: u8 = 0b0010_0000;
const ADC_ENABLE_1_ACIN_CUR: u8 = 0b0001_0000;
const ADC_ENABLE_1_VBUS_VOL: u8 = 0b0000_1000;
const ADC_ENABLE_1_VBUS_CUR: u8 = 0b0000_0100;
const ADC_ENABLE_1_APS_VOL: u8 = 0b0000_0010;
const ADC_ENABLE_1_TS_PIN: u8 = 0b0000_0001;

const ADC_ENABLE_2: u8 = 0x83;
const ADC_ENABLE_2_TEMP_MON: u8 = 0b1000_0000;
const ADC_ENABLE_2_GPIO0: u8 = 0b0000_1000;
const ADC_ENABLE_2_GPIO1: u8 = 0b0000_0100;
const ADC_ENABLE_2_GPIO2: u8 = 0b0000_0010;
const ADC_ENABLE_2_GPIO3: u8 = 0b0000_0001;

const ADC_TS: u8 = 0x84;
const ADC_TS_SAMPLE_200HZ: u8 = 0b1100_0000;
const ADC_TS_SAMPLE_100HZ: u8 = 0b1000_0000;
const ADC_TS_SAMPLE_50HZ: u8 = 0b0100_0000;
const ADC_TS_SAMPLE_25HZ: u8 = 0b0000_0000;
const ADC_TS_SAMPLE_MASK: u8 = 0b1100_0000;
const ADC_TS_OUT_CUR_80UA: u8 = 0b0011_0000;
const ADC_TS_OUT_CUR_60UA: u8 = 0b0010_0000;
const ADC_TS_OUT_CUR_40UA: u8 = 0b0001_0000;
const ADC_TS_OUT_CUR_20UA: u8 = 0b0000_0000;
const ADC_TS_OUT_CUR_MASK: u8 = 0b0011_0000;
const ADC_TS_PIN_TEMP_MON: u8 = 0b0000_0000;
const ADC_TS_PIN_EXTERN_ADC: u8 = 0b0000_0100;
const ADC_TS_PIN_OUT_ALWAYS: u8 = 0b0000_0011;
const ADC_TS_PIN_OUT_SAVE_ENG: u8 = 0b0000_0010;
const ADC_TS_PIN_OUT_CHG: u8 = 0b0000_0001;
const ADC_TS_PIN_OUT_DIS: u8 = 0b0000_0000;
const ADC_TS_PIN_OUT_MASK: u8 = 0b0000_0011;

const GPIO0_FUNCTION: u8 = 0x90;
const GPIO0_FUNCTION_FLOATING: u8 = 0b0000_0111;
const GPIO0_FUNCTION_LOW_OUTPUT: u8 = 0b0000_0101;
const GPIO0_FUNCTION_ADC_INPUT: u8 = 0b0000_0100;
const GPIO0_FUNCTION_LDO_OUTPUT: u8 = 0b0000_0010;
const GPIO0_FUNCTION_GENERAL_INPUT: u8 = 0b0000_0001;
const GPIO0_FUNCTION_OPEN_DRAIN_OUTPUT: u8 = 0b0000_0000;

const GPIO0_LDO_VOLTAGE: u8 = 0x91;
const GPIO0_LDO_VOLTAGE_3_3V: u8 = 0b1111_0000;
const GPIO0_LDO_VOLTAGE_2_8V: u8 = 0b1010_0000;
const GPIO0_LDO_VOLTAGE_1_8V: u8 = 0b0000_0000;

impl Axp192 {
    pub fn new(i2c: I2c0) -> color_eyre::Result<Self> {
        let this = Axp192 { inner: i2c };
        this.init()?;

        Ok(this)
    }

    pub fn start_battery_thread(&self) -> JoinHandle<()> {
        let this = self.clone();
        std::thread::spawn(move || loop {
            if let Ok(pct) = this.get_batt_pct() {
                BATTERY_PERCENT.store(pct, std::sync::atomic::Ordering::Relaxed);
            }

            std::thread::sleep(Duration::from_secs(10));
        })
    }

    pub fn get_batt_pct(&self) -> color_eyre::Result<u8> {
        let batt_volt = self.get_batt_voltage()?;
        let batt_pct = (batt_volt.clamp(3.0, 4.2) - 3.0) / (4.2 - 3.0);
        let batt_pct = (batt_pct * 100.0) as u8;
        Ok(batt_pct)
    }

    pub fn get_batt_voltage(&self) -> color_eyre::Result<f32> {
        let upper = (self.read(ADC_BATT_VOLTAGE_H)? as u16) << 4;
        let lower = self.read(ADC_BATT_VOLTAGE_L)? as u16;
        let val = (upper | lower) as f32;
        Ok(val * 1.1 / 1000.0)
    }

    pub fn get_batt_power(&self) -> color_eyre::Result<f32> {
        let upper = (self.read(ADC_BATT_POWER_H)? as u32) << 16;
        let middle = (self.read(ADC_BATT_POWER_M)? as u32) << 8;
        let lower = self.read(ADC_BATT_POWER_L)? as u32;
        let val = (upper | middle | lower) as f32;
        Ok(val * 1.1 * 0.5 / 1000.0)
    }

    pub fn get_vbus_current(&self) -> color_eyre::Result<f32> {
        let upper = (self.read(ADC_VBUS_CURRENT_H)? as u16) << 4;
        let lower = self.read(ADC_VBUS_CURRENT_L)? as u16;
        let val = (upper | lower) as f32;
        Ok(val * 0.375 / 1000.0)
    }

    pub fn set_backlight(&self, on: bool) -> color_eyre::Result<()> {
        let val = self.read(DCDC13_LDO23_CTRL)?;
        let val = if on {
            val | DCDC13_LDO23_CTRL_LDO2
        } else {
            val & !DCDC13_LDO23_CTRL_LDO2
        };
        self.write(DCDC13_LDO23_CTRL, val)?;

        Ok(())
    }

    fn init(&self) -> color_eyre::Result<()> {
        self.write(
            LDO23_OUT_VOLTAGE,
            LDO23_OUT_VOLTAGE_LDO2_3_0V | LDO23_OUT_VOLTAGE_LDO3_3_0V,
        )?;

        self.write(EXTEN_DCDC2_CTRL, EXTEN_DCDC2_CTRL_EXTEN)?;

        // rtc: LDO1
        // tft backlight: LDO2
        // tft ic: LDO3
        // mpu6886: DCDC1
        let val = self.read(DCDC13_LDO23_CTRL)?;
        self.write(
            DCDC13_LDO23_CTRL,
            val | DCDC13_LDO23_CTRL_LDO2 | DCDC13_LDO23_CTRL_LDO3 | DCDC13_LDO23_CTRL_DCDC1,
        )?;

        self.write(
            ADC_TS,
            ADC_TS_SAMPLE_200HZ
                | ADC_TS_OUT_CUR_80UA
                | ADC_TS_PIN_TEMP_MON
                | ADC_TS_PIN_OUT_SAVE_ENG,
        )?;

        self.write(
            ADC_ENABLE_1,
            ADC_ENABLE_1_BATT_VOL
                | ADC_ENABLE_1_BATT_CUR
                | ADC_ENABLE_1_ACIN_VOL
                | ADC_ENABLE_1_ACIN_CUR
                | ADC_ENABLE_1_VBUS_VOL
                | ADC_ENABLE_1_VBUS_CUR
                | ADC_ENABLE_1_APS_VOL
                | ADC_ENABLE_1_TS_PIN,
        )?;

        self.write(
            VBUS_IPSOUT,
            VBUS_IPSOUT_VHOLD_LIMIT
                | VBUS_IPSOUT_VHOLD_VOLTAGE_4_4V
                | VBUS_IPSOUT_VBUS_LIMIT_CURRENT
                | VBUS_IPSOUT_VBUS_LIMIT_CURRENT_500MA,
        )?;

        self.write(POWER_OFF_VOLTAGE, POWER_OFF_VOLTAGE_3_0V)?;

        self.write(
            CHARGING_CTRL1,
            CHARGING_CTRL1_ENABLE
                | CHARGING_CTRL1_VOLTAGE_4_20V
                | CHARGING_CTRL1_CHARGING_THRESH_10PERC
                | CHARGING_CTRL1_CURRENT_100MA,
        )?;

        self.write(
            PEK,
            PEK_SHORT_PRESS_128MS
                | PEK_LONG_PRESS_1_5S
                | PEK_LONG_PRESS_POWER_OFF
                | PEK_PWROK_DELAY_64MS
                | PEK_POWER_OFF_TIME_4S,
        )?;

        self.write(
            BACKUP_BATT,
            BACKUP_BATT_CHARGING_ENABLE
                | BACKUP_BATT_CHARGING_VOLTAGE_3_0V
                | BACKUP_BATT_CHARGING_CURRENT_200UA,
        )?;

        self.write(GPIO0_LDO_VOLTAGE, GPIO0_LDO_VOLTAGE_3_3V)?;
        self.write(GPIO0_FUNCTION, GPIO0_FUNCTION_LDO_OUTPUT)?;

        Ok(())
    }

    fn read(&self, reg: u8) -> color_eyre::Result<u8> {
        let mut buf = [0u8; 1];
        self.inner
            .lock()
            .unwrap()
            .write_read(ADDR, &[reg], &mut buf)?;
        Ok(buf[0])
    }

    fn write(&self, reg: u8, val: u8) -> color_eyre::Result<()> {
        self.inner.lock().unwrap().write(ADDR, &[reg, val])?;
        Ok(())
    }
}
