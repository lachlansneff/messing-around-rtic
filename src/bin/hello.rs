#![no_main]
#![no_std]

use messing_around_rtic as _; // global logger + panicking-behavior + memory layout

#[cortex_m_rt::entry]
fn main() -> ! {
    defmt::info!("Hello, world!");

    messing_around_rtic::exit()
}
