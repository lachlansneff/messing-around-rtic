#![no_main]
#![no_std]

use embedded_hal::spi::{Mode, Phase, Polarity};
use messing_around_rtic::{
    wifi::Wifi,
}; // global logger + panicking-behavior + memory layout
use stm32f4xx_hal::{dwt::{Delay, DwtExt}, gpio::{AF6, Alternate, Floating, Input, Output, PullDown, PushPull, gpiob::{PB3, PB4, PB5, PB6, PB7, PB8, PB9}}, pac::{SPI3, TIM1}, prelude::*, spi::Spi, timer::Timer};
use rtic::{app, cyccnt::U32Ext as _};

const PERIOD: u32 = 1_000_000; // 10ms at 100Mhz

type WIFI = Wifi<
    Spi<SPI3, (PB3<Alternate<AF6>>, PB4<Alternate<AF6>>, PB5<Alternate<AF6>>)>,
    PB6<Output<PushPull>>,
    PB7<Input<PullDown>>,
    PB8<Output<PushPull>>,
    PB9<Output<PushPull>>,
    Delay,
>;

#[app(
    device = stm32f4xx_hal::pac,
    monotonic = rtic::cyccnt::CYCCNT,
    peripherals = true
)]
const APP: () = {
    struct Resources {
        // wifi: WIFI,
        cs: PB6<Output<PushPull>>,
        spi: Spi<SPI3, (PB3<Alternate<AF6>>, PB4<Alternate<AF6>>, PB5<Alternate<AF6>>)>,
    }

    #[init(
        spawn = [every_10_ms, once_after_init]
    )]
    fn init(mut cx: init::Context) -> init::LateResources {
        defmt::info!("Hello, world!");

        let dp = cx.device;

        // enable the cyccnt monotonic counter
        cx.core.DWT.enable_cycle_counter();

        let rcc = dp.RCC.constrain();
        let clocks = rcc.cfgr
            .sysclk(100.mhz())
            .freeze();

        // spi3_sck => pb3
        // spi3_miso => pb4
        // spi3_mosi => pb5
        // also pick a slave select

        let gpiob = dp.GPIOB.split();
        let sck = gpiob.pb3.into_alternate_af6();
        let miso = gpiob.pb4.into_alternate_af6();
        let mosi = gpiob.pb5.into_alternate_af6();
        let mut cs = gpiob.pb6.into_push_pull_output();
        let ready = gpiob.pb7.into_pull_down_input();
        let reset = gpiob.pb8.into_push_pull_output();
        let gpio0 = gpiob.pb9.into_push_pull_output();

        let mut spi = Spi::spi3(
            dp.SPI3,
            (sck, miso, mosi),
            Mode {
                polarity: Polarity::IdleLow,
                phase: Phase::CaptureOnFirstTransition,
            },
            8.mhz().into(),
            clocks.clone(),
        );

        let dwt = cx.core.DWT.constrain(cx.core.DCB, clocks);
        let delay = dwt.delay();

        // let timer = Timer::tim1(dp.TIM1, 1.hz(), clocks);

        cx.spawn.every_10_ms().unwrap();
        cx.spawn.once_after_init().unwrap();

        init::LateResources {
            // wifi: Wifi::new(spi, cs, ready, reset, gpio0, delay).unwrap(),
            cs,
            spi,
        }
    }

    #[idle]
    fn idle(_cx: idle::Context) -> ! {
        // If we don't have this idle loop, then `wfi` will disconnect
        // the device from the debugger.
        loop {}
    }

    #[task(resources = [])]
    fn once_after_init(cx: once_after_init::Context) {
        // let mac_addr = cx.resources.wifi.get_mac_address().unwrap();
        // defmt::info!("mac address: {:?}", mac_addr);
    }

    #[task(schedule = [every_10_ms], resources = [cs, spi])]
    fn every_10_ms(cx: every_10_ms::Context) {
        cx.schedule.every_10_ms(cx.scheduled + PERIOD.cycles()).unwrap();
        
        cx.resources.cs.set_low().ok();
        cx.resources.spi.write(&[0x42]).unwrap();
        cx.resources.cs.set_high().ok();

        // defmt::info!("supposed to be every 10ms");
    }

    extern {
        fn USART1();
    }
};
