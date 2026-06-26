#![no_std]
#![no_main]

mod animation;
mod display;
mod glyph;
mod weather;

use core::sync::atomic::{AtomicU8, Ordering};

use core::fmt::Write;
use cyw43::{Control, aligned_bytes};
use cyw43_pio::{PioSpi, RM2_CLOCK_DIVIDER};
use display::Display;
use embassy_executor::Spawner;
use embassy_net::dns::DnsSocket;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_net::{Stack, StackResources};
use embassy_rp::adc::{Adc, Channel, Config as AdcConfig, InterruptHandler as AdcInterruptHandler};
use embassy_rp::gpio::{Level, Output};
use embassy_rp::i2c::{Async, Config, I2c};
use embassy_rp::peripherals::{DMA_CH0, DMA_CH1, I2C1, PIO0, USB};
use embassy_rp::pio::Pio;
use embassy_rp::usb::Driver;
use embassy_rp::{bind_interrupts, dma};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Instant, Timer};
use embedded_hal_async::i2c::I2c as _;
use reqwless::client::HttpClient;
use static_cell::StaticCell;

use crate::display::segment_to_frame_byte;
use crate::weather::Weather;

use {defmt_rtt as _, panic_probe as _};

const ADDR: u8 = 0x71;

static OUTSIDE_TEMPERATURE_TASK_ERROR: Mutex<CriticalSectionRawMutex, Option<&'static str>> =
    Mutex::new(None);
static DISPLAY_BRIGHTNESS: AtomicU8 = AtomicU8::new(9);
static WEATHER: Mutex<CriticalSectionRawMutex, Option<Weather>> = Mutex::new(None);

bind_interrupts!(struct Irqs {
    I2C1_IRQ => embassy_rp::i2c::InterruptHandler<I2C1>;
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<PIO0>;
    DMA_IRQ_0 => dma::InterruptHandler<DMA_CH0>, dma::InterruptHandler<DMA_CH1>;
    USBCTRL_IRQ => embassy_rp::usb::InterruptHandler<USB>;
    ADC_IRQ_FIFO => AdcInterruptHandler;
});

#[embassy_executor::task]
async fn logger_task(driver: Driver<'static, USB>) {
    embassy_usb_logger::run!(1024, log::LevelFilter::Trace, driver);
}

#[embassy_executor::task]
async fn cyw43_task(
    runner: cyw43::Runner<'static, cyw43::SpiBus<Output<'static>, PioSpi<'static, PIO0, 0>>>,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn potentiometer_brightness_task(
    mut adc: Adc<'static, embassy_rp::adc::Async>,
    mut pin: Channel<'static>,
) {
    loop {
        match adc.read(&mut pin).await {
            Ok(raw) => {
                let brightness = (raw / 256) as u8;
                let last_brightness = DISPLAY_BRIGHTNESS.load(Ordering::Relaxed);

                if brightness != last_brightness {
                    log::debug!("Brightness value (0 - 15): {}", brightness);
                    DISPLAY_BRIGHTNESS.store(brightness, Ordering::Relaxed);
                }

                Timer::after(Duration::from_millis(100)).await;
            }
            Err(_) => {
                log::error!("Could not read value from brightness potentiometer");
                Timer::after(Duration::from_millis(5000)).await;
            }
        }
    }
}

#[embassy_executor::task]
async fn outside_temperature_task(
    wttr_url: &'static str,
    wifi_ssid: &'static str,
    wifi_password: &'static str,
    mut control: Control<'static>,
    stack: Stack<'static>,
) -> ! {
    static TCP_STATE: StaticCell<TcpClientState<2, 1024, 1024>> = StaticCell::new();
    let tcp_state = TCP_STATE.init(TcpClientState::new());
    let tcp_client = TcpClient::new(stack, tcp_state);
    let dns_socket = DnsSocket::new(stack);
    let mut client = HttpClient::new(&tcp_client, &dns_socket);

    loop {
        log::trace!("Setting wifi chip to no power management...");
        control
            .set_power_management(cyw43::PowerManagementMode::None)
            .await;

        match weather::fetch(
            &mut control,
            &stack,
            &mut client,
            wifi_ssid,
            wifi_password,
            wttr_url,
        )
        .await
        {
            Ok(w) => {
                let mut weather = WEATHER.lock().await;
                *weather = Some(w);
            }
            Err(e) => {
                log::error!("Fetching outside temperature failed: {:?}", e);
                {
                    let mut error = OUTSIDE_TEMPERATURE_TASK_ERROR.lock().await;
                    *error = Some(e.code());
                }
            }
        }

        log::trace!("Setting wifi chip to aggressive power management...");
        control
            .set_power_management(cyw43::PowerManagementMode::Aggressive)
            .await;

        log::info!("Waiting 15 minutes before fetching outside temperature again...");
        Timer::after(Duration::from_secs(15 * 60)).await;
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    const WIFI_SSID: &str = env!("WIFI_SSID");
    const WIFI_PASSWORD: &str = env!("WIFI_PASSWORD");
    const WTTR_LOCATION: &str = env!("WTTR_LOCATION");

    const MAX_URL_LEN: usize = 128;
    const UNPARAMETERIZED_URL: &str = "http://wttr.in/?format=2";

    const _: () = {
        assert!(!WIFI_SSID.is_empty(), "WIFI_SSID must not be empty");
        assert!(!WIFI_PASSWORD.is_empty(), "WIFI_PASSWORD must not be empty");
        assert!(!WTTR_LOCATION.is_empty(), "WTTR_LOCATION must not be empty");
        assert!(
            UNPARAMETERIZED_URL.len() + WTTR_LOCATION.len() <= MAX_URL_LEN,
            "WTTR_LOCATION too long, URL would exceed 128 chars"
        );
    };

    // Set up peripherals
    let p = embassy_rp::init(Default::default());

    // Set up usb logger
    let driver = Driver::new(p.USB, Irqs);
    spawner.spawn(logger_task(driver).unwrap());

    // Set up brightness potentiometer
    let adc = Adc::new(p.ADC, Irqs, AdcConfig::default());
    let pot_pin = Channel::new_pin(p.PIN_26, embassy_rp::gpio::Pull::None);
    spawner.spawn(potentiometer_brightness_task(adc, pot_pin).unwrap());

    // Set up i2c aka 14 segment display
    let mut cfg = Config::default();
    cfg.frequency = 100_000;
    let mut i2c = I2c::new_async(p.I2C1, p.PIN_15, p.PIN_14, Irqs, cfg);
    i2c.write(ADDR, &[0x21]).await.unwrap();
    i2c.write(ADDR, &[0x81]).await.unwrap();
    i2c.write(ADDR, &[0xE0]).await.unwrap();

    async fn show(i2c: &mut I2c<'_, I2C1, Async>, s: &str) {
        match Display::from_str(s).and_then(|d| d.to_frame()) {
            Ok(frame) => {
                log::debug!("Showing {}", s);
                let _ = i2c.write(ADDR, &frame).await;
            }
            Err(e) => {
                log::error!("Could not show {}: {:?}", s, e);
            }
        }
    }

    // Set up wifi
    let fw = aligned_bytes!("../firmware/43439A0.bin");
    let clm = aligned_bytes!("../firmware/43439A0_clm.bin");
    let nvram = aligned_bytes!("../firmware/nvram_rp2040.bin");
    let pwr = Output::new(p.PIN_23, Level::Low);
    let cs = Output::new(p.PIN_25, Level::High);
    let mut pio = Pio::new(p.PIO0, Irqs);
    let spi = PioSpi::new(
        &mut pio.common,
        pio.sm0,
        RM2_CLOCK_DIVIDER,
        pio.irq0,
        cs,
        p.PIN_24,
        p.PIN_29,
        dma::Channel::new(p.DMA_CH0, Irqs),
    );
    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init(cyw43::State::new());
    let (net_device, mut control, runner) = cyw43::new(state, pwr, spi, fw, nvram).await;
    static RESOURCES: StaticCell<StackResources<16>> = StaticCell::new();

    let mut seed = 0u64;
    for i in 0..64 {
        seed |= (embassy_rp::pac::ROSC.randombit().read().randombit() as u64) << i;
    }

    let (stack, runner_net) = embassy_net::new(
        net_device,
        embassy_net::Config::dhcpv4(Default::default()),
        RESOURCES.init(StackResources::new()),
        seed,
    );
    spawner.spawn(cyw43_task(runner).unwrap());
    spawner.spawn(net_task(runner_net).unwrap());
    control.init(clm).await;

    let mut url: heapless::String<MAX_URL_LEN> = heapless::String::new();
    let _ = write!(url, "http://wttr.in/{}?format=2", WTTR_LOCATION);
    static URL: StaticCell<heapless::String<MAX_URL_LEN>> = StaticCell::new();
    let url = URL.init(url);

    spawner.spawn(
        outside_temperature_task(url.as_str(), WIFI_SSID, WIFI_PASSWORD, control, stack).unwrap(),
    );

    loop {
        let current_weather = {
            let weather = WEATHER.lock().await;
            *weather
        };
        let display_brightness = DISPLAY_BRIGHTNESS.load(Ordering::Relaxed);
        let wifi_error = {
            let error = OUTSIDE_TEMPERATURE_TASK_ERROR.lock().await;
            *error
        };

        i2c.write(ADDR, &[0xE0 + display_brightness]).await.unwrap();

        match wifi_error {
            Some(code) => {
                show(&mut i2c, code).await;
                Timer::after(Duration::from_millis(100)).await;
            }
            None => match current_weather {
                Some(weather) => {
                    let display_loop_timestamp = Instant::now().as_millis() % 30_000;

                    if display_loop_timestamp < 20_000 {
                        let mut current_outside_temp_string: heapless::String<4> =
                            heapless::String::new();
                        let _ =
                            write!(current_outside_temp_string, "{:>3}C", weather.temperature_c);
                        show(&mut i2c, current_outside_temp_string.as_str()).await;
                        Timer::after(Duration::from_millis(100)).await;
                    } else if display_loop_timestamp < 25_000 {
                        let animation = weather.condition.animation();

                        let frame_index = (Instant::now().as_millis()
                            / (animation.duration / animation.frames.len() as u64))
                            as usize
                            % animation.frames.len();

                        match animation::build_frame_bytes(animation, frame_index) {
                            Ok(frame_bytes) => {
                                i2c.write(ADDR, &frame_bytes).await.unwrap();
                            }
                            Err(e) => {
                                log::error!(
                                    "Error building frame byte for animation of weather condition {}: {:?}",
                                    weather.condition,
                                    e
                                )
                            }
                        }

                        Timer::after(Duration::from_millis(10)).await;
                    } else {
                        let mut current_wind_string: heapless::String<4> = heapless::String::new();
                        let _ = write!(current_wind_string, "{}", weather.wind);
                        show(&mut i2c, current_wind_string.as_str()).await;
                        Timer::after(Duration::from_millis(100)).await;
                    }
                }
                None => {
                    let animation = animation::LOADING;

                    let frame_index = (Instant::now().as_millis()
                        / (animation.duration / animation.frames.len() as u64))
                        as usize
                        % animation.frames.len();
                    let mut frame_bytes = [0u8; 17];
                    for character_segment in animation.frames[frame_index].character_segments {
                        let (byte_index, byte) = segment_to_frame_byte(
                            character_segment.character,
                            character_segment.segment,
                        )
                        .unwrap();
                        frame_bytes[byte_index] |= byte;
                    }
                    i2c.write(ADDR, &frame_bytes).await.unwrap();
                    Timer::after(Duration::from_millis(10)).await;
                }
            },
        }
    }
}
