use core::{convert::Infallible, slice};

use embedded_hal::{
    blocking::{spi, delay::DelayMs},
    digital::v2::{OutputPin, InputPin},
};
use stm32f4xx_hal::{block, gpio::Input, prelude::*};

const REPLY_FLAG: u8 = 1 << 7;
const DATA_FLAG: u8 = 0x40;
const START_CMD: u8 = 0xe0;
const END_CMD: u8 = 0xee;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CmdCode {
    SetNet = 0x10,
    SetPassPhrase = 0x11,
    SetKey = 0x12,
    SetIPConfig = 0x14,
    SetDNSConfig = 0x15,
    SetHostname = 0x16,
    SetPowerMode = 0x17,
    SetAPNet = 0x18,
    SetAPPassPhrase = 0x19,
    SetDebug = 0x1a,
    GetTemperature = 0x1b,
    GetConnStatus = 0x20,
    GetIPAddress = 0x21,
    GetMACAddress = 0x22,
    GetCurrentSSID = 0x23,
    GetCurrentBSSID = 0x24,
    GetCurrentRSSI = 0x25,
    GetCurrentEncryption = 0x26,
    ScanNetwork = 0x27,
    StartServerTCP = 0x28,
    GetStateTCP = 0x29,
    DataSentTCP = 0x2a,
    AvailableDataTCP = 0x2b,
    GetDataTCP = 0x2c,
    StartClientTCP = 0x2d,
    StopClientTCP = 0x2e,
    GetClientStateTCP = 0x2f,
    Disconnect = 0x30,
    GetIndexRSSI = 0x32,
    GetIndexEncryption = 0x33,
    RequestHostByName = 0x34,
    GetHostByName = 0x35,
    StartScanNetworks = 0x36,
    GetFirmwareVersion = 0x37,
    SendUDPData = 0x39,
    GetRemoteData = 0x3a,
    GetTime = 0x3b,
    GetIndexBSSID = 0x3c,
    GetIndexChannel = 0x3d,
    Ping = 0x3e,
    GetSocket = 0x3f,
    SetClientCert = 0x40,
    SetCertKey = 0x41,
    SendDataTCP = 0x44,
    GetDataBufTCP = 0x45,
    InsertDataBuf = 0x46,
    WPA2EnterpriseSetIdentity = 0x4a,
    WPA2EnterpriseSetUsername = 0x4b,
    WPA2EnterpriseSetPassword = 0x4c,
    WPA2EnterpriseSetCACert = 0x4d,
    WPA2EnterpriseSetCertKey = 0x4e,
    WPA2EnterpriseEnable = 0x4f,
    SetPinMode = 0x50,
    SetDigitalWrite = 0x51,
    SetAnalogWrite = 0x52,
    SetDigitalRead = 0x53,
    SetAnalogRead = 0x54,
}

struct ParamWriter<'a, SPI> {
    spi: &'a mut SPI,
}

impl<SPI, E> ParamWriter<'_, SPI>
where
    SPI: spi::Transfer<u8, Error = E> + spi::Write<u8, Error = E>,
{
    fn write_bytes<const LEN: usize>(&mut self, data: &[u8; LEN]) -> Result<(), E> {
        assert!(LEN <= u8::MAX as usize);

        self.spi.write(&[data.len() as u8])?;
        self.spi.write(data)?;

        Ok(())
    }
}

struct Driver<SPI> {
    spi: SPI,
}

impl<SPI, E> Driver<SPI>
where
    SPI: spi::Transfer<u8, Error = E> + spi::Write<u8, Error = E>,
{
    fn send_cmd<F>(&mut self, cmd: CmdCode, param_num: u8, f: F) -> Result<(), E>
    where
        F: FnOnce(ParamWriter<SPI>) -> Result<(), E>,
    {
        self.spi.write(&[
            START_CMD,
            cmd as u8 & !REPLY_FLAG,
            param_num
        ])?;

        f(ParamWriter { spi: &mut self.spi })?;

        self.spi.write(&[END_CMD])?;

        Ok(())
    }

    fn receive_reply<'a>(&mut self, cmd: CmdCode, param_num: u8, data: &'a mut [u8]) -> Result<&'a [u8], E> {
        while self.read_byte()? != START_CMD {}
        // assert_eq!(self.read_byte()?, START_CMD);
        assert_eq!(self.read_byte()?, cmd as u8 | REPLY_FLAG);
        assert_eq!(self.read_byte()?, param_num);
        assert_eq!(self.read_byte()?, data.len() as u8);

        self.spi.transfer(data)?;
        
        assert_eq!(self.read_byte()?, END_CMD);

        Ok(data)
    }

    fn read_byte(&mut self) -> Result<u8, E> {
        let mut b = 0xff;
        self.spi.transfer(slice::from_mut(&mut b))?;

        Ok(b)
    }
}

pub struct Wifi<SPI, CS, READY, RESET, GPIO0, TIMER> {
    driver: Driver<SPI>,
    cs: CS,
    ready: READY,
    reset: RESET,
    gpio0: GPIO0,

    timer: TIMER,
}

impl<SPI, CS, READY, RESET, GPIO0, TIMER, E> Wifi<SPI, CS, READY, RESET, GPIO0, TIMER>
where
    SPI: spi::Transfer<u8, Error = E> + spi::Write<u8, Error = E>,
    CS: OutputPin<Error = Infallible>,
    READY: InputPin<Error = Infallible>,
    RESET: OutputPin<Error = Infallible>,
    GPIO0: OutputPin<Error = Infallible>,
    TIMER: DelayMs<u32>,
{
    pub fn new(spi: SPI, cs: CS, ready: READY, reset: RESET, gpio0: GPIO0, timer: TIMER) -> Result<Self, E> {
        let mut this = Self {
            driver: Driver {
                spi,
            },
            cs,
            ready,
            reset,
            gpio0,
            timer,
        };

        this.reset().map(move |_| this)
    }

    pub fn reset(&mut self) -> Result<(), E> {
        self.gpio0.set_high().ok();
        self.cs.set_high().ok();
        self.reset.set_low().ok();

        // delay 10ms, reset
        self.timer.delay_ms(10);
        
        self.reset.set_high().ok();

        // delay 750ms, wait for it to boot up
        self.timer.delay_ms(750);

        Ok(())
    }

    fn wait_for_ready(&mut self) {
        let mut count = 0;

        loop {
            if let Ok(true) = self.ready.is_high() {
                break;
            }
            
            self.timer.delay_ms(1);
            count += 1;

            if count == 10_000 {
                panic!("esp32 not responding");
            }
        }
    }

    /// Adapted from https://github.com/arduino-libraries/WiFiNINA/blob/ea7748e0511d1d7a7d003548305d9d3bb0d71573/src/utility/wifi_drv.cpp#L375-L401.
    pub fn get_mac_address(&mut self) -> Result<[u8; 6], E> {
        self.wait_for_ready();

        self.driver.send_cmd(CmdCode::GetMACAddress, 1, |mut writer| {
            writer.write_bytes(&[0xff]) // dummy data
        })?;

        self.wait_for_ready();

        let mut mac = [0; 6];

        self.driver.receive_reply(CmdCode::GetMACAddress, 1, &mut mac)?;

        Ok(mac)
    }
}

