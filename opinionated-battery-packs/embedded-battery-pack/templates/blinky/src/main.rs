#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
{%- if panic_handler == "panic-halt" %}
use panic_halt as _;
{%- elif panic_handler == "panic-probe" %}
use panic_probe as _;
{%- elif panic_handler == "panic-rtt" %}
use panic_rtt_target as _;
{%- elif panic_handler == "panic-semihosting" %}
use cortex_m_semihosting as _;
{%- endif %}

{%- if concurrency == "embassy" %}
use embassy_executor::Spawner;
use embassy_time::Timer;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    // TODO: Initialize your HAL and configure the LED GPIO pin here.
    info!("{{ project_name }} started!");

    loop {
        // TODO: Toggle LED pin
        info!("blink!");
        Timer::after_millis(500).await;
    }
}
{%- elif concurrency == "rtic" %}
#[rtic::app(device = todo!("replace with your PAC crate"))]
mod app {
    use defmt::info;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        // TODO: Add your LED pin here
    }

    #[init]
    fn init(_cx: init::Context) -> (Shared, Local) {
        info!("{{ project_name }} started!");
        // TODO: Configure clocks and GPIO
        (Shared {}, Local {})
    }

    #[idle]
    fn idle(_cx: idle::Context) -> ! {
        loop {
            // TODO: Toggle LED with a timer or software task
            cortex_m::asm::wfi();
        }
    }
}
{%- endif %}
