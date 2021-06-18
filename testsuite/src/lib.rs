#![no_std]
#![cfg_attr(test, no_main)]

use messing_around_rtic as _; // memory layout + panic handler

#[defmt_test::tests]
mod tests {}
