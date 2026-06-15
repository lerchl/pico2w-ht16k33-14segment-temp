#![no_std]
#![no_main]

mod animation;
mod display;
mod glyph;

use core::sync::atomic::{AtomicI8, AtomicU8, Ordering};

use core::fmt::Write;
use cyw43::{Control, JoinOptions, aligned_bytes};
use cyw43_pio::{PioSpi, RM2_CLOCK_DIVIDER};
use display::Display;
use embassy_executor::Spawner;
use embassy_net::dns::DnsSocket;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_net::{Stack, StackResources};
use embassy_rp::adc::{Adc, Channel, Config as AdcConfig, InterruptHandler as AdcInterruptHandler};
use embassy_rp::gpio::{Level, Output};
use embassy_rp::i2c::{Config, I2c};
use embassy_rp::peripherals::{DMA_CH0, DMA_CH1, I2C1, PIO0, USB};
use embassy_rp::pio::Pio;
use embassy_rp::usb::Driver;
use embassy_rp::{bind_interrupts, dma};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Instant, Timer};
use embedded_hal_async::i2c::I2c as _;
use embedded_io_async::Read;
use reqwless::client::HttpClient;
use reqwless::request::Method;
use static_cell::StaticCell;

use crate::animation::LOADING;
use crate::display::segment_to_frame_byte;

use {defmt_rtt as _, panic_probe as _};

const ADDR: u8 = 0x71;

static WIFI_ERROR: Mutex<CriticalSectionRawMutex, Option<heapless::String<4>>> = Mutex::new(None);
static DISPLAY_BRIGHTNESS: AtomicU8 = AtomicU8::new(9);
static OUTSIDE_TEMP: AtomicI8 = AtomicI8::new(-128);

bind_interrupts!(struct Irqs {
    I2C1_IRQ => embassy_rp::i2c::InterruptHandler<I2C1>;
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<PIO0>;
    DMA_IRQ_0 => dma::InterruptHandler<DMA_CH0>, dma::InterruptHandler<DMA_CH1>;
    USBCTRL_IRQ => embassy_rp::usb::InterruptHandler<USB>;
    ADC_IRQ_FIFO => AdcInterruptHandler;
});

#[embassy_executor::task]
async fn logger_task(driver: Driver<'static, USB>) {
    embassy_usb_logger::run!(1024, log::LevelFilter::Info, driver);
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
        let raw = adc.read(&mut pin).await.unwrap();
        log::trace!("Raw brightness value (0 - 4095): {}", raw);
        let brightness = (raw / 256) as u8;
        log::debug!("Brightness value (0 - 15): {}", brightness);
        DISPLAY_BRIGHTNESS.store(brightness, Ordering::Relaxed);
        Timer::after(Duration::from_millis(100)).await;
    }
}

async fn join_wifi(
    control: &mut Control<'_>,
    stack: &Stack<'_>,
    wifi_ssid: &str,
    wifi_password: &str,
) {
    loop {
        match control
            .join(wifi_ssid, JoinOptions::new(wifi_password.as_bytes()))
            .await
        {
            Ok(_) => {
                log::info!("Wifi joined");
                break;
            }
            Err(e) => {
                log::warn!("Wifi join failed: {:?}, retrying...", e);
                {
                    let mut error = WIFI_ERROR.lock().await;
                    *error = Some(heapless::String::try_from("WE01").unwrap());
                }
                Timer::after(Duration::from_secs(2)).await;
            }
        }
    }

    while !stack.is_link_up() {
        log::trace!("Waiting for link up...");
        Timer::after(Duration::from_millis(500)).await;
    }

    log::trace!("Waiting for DHCP...");
    stack.wait_config_up().await;
    log::info!("Network up: {:?}", stack.config_v4());
}

async fn fetch_temperature(
    client: &mut HttpClient<'_, TcpClient<'_, 2>, DnsSocket<'_>>,
    wttr_url: &str,
) -> Result<i8, &'static str> {
    log::trace!("Creating http request...");
    let mut req = client.request(Method::GET, wttr_url).await.map_err(|e| {
        log::error!("HTTP request failed: {:?}", e);
        "request failed"
    })?;

    log::trace!("Sending http request...");
    let mut rx_buf = [0u8; 4096];
    let response = req.send(&mut rx_buf).await.map_err(|e| {
        log::error!("HTTP send failed: {:?}", e);
        "send failed"
    })?;

    log::trace!("Reading http response...");
    let mut body_buf = [0u8; 64];
    let n = response
        .body()
        .reader()
        .read(&mut body_buf)
        .await
        .map_err(|e| {
            log::error!("Body read error: {:?}", e);
            "read failed"
        })?;

    log::trace!("Parsing http response...");
    let s = core::str::from_utf8(&body_buf[..n])
        .map_err(|e| {
            log::error!("Body not valid UTF-8: {:?}", e);
            "utf8 error"
        })?
        .trim()
        .trim_start_matches('+')
        .trim_end_matches("°C");

    Ok(s.parse().unwrap_or(-128))
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

        if !stack.is_link_up() {
            log::info!("Link is down, (re-)joining wifi...");
            join_wifi(&mut control, &stack, wifi_ssid, wifi_password).await;
        }

        log::trace!("Fetching outside temperature...");
        match fetch_temperature(&mut client, wttr_url).await {
            Ok(temp) => OUTSIDE_TEMP.store(temp, Ordering::Relaxed),
            Err(e) => log::error!("Fetch failed: {}", e),
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
    const UNPARAMETERIZED_URL: &str = "http://wttr.in/?format=%t";

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

    macro_rules! show {
        ($i2c:expr, $s:expr) => {
            if let Ok(frame) = Display::from_str($s).and_then(|d| d.to_frame()) {
                let _ = $i2c.write(ADDR, &frame).await;
            }
        };
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
    let _ = write!(url, "http://wttr.in/{}?format=%t", WTTR_LOCATION);
    static URL: StaticCell<heapless::String<MAX_URL_LEN>> = StaticCell::new();
    let url = URL.init(url);

    spawner.spawn(
        outside_temperature_task(url.as_str(), WIFI_SSID, WIFI_PASSWORD, control, stack).unwrap(),
    );

    loop {
        let current_outside_temp = OUTSIDE_TEMP.load(Ordering::Relaxed);
        let display_brightness = DISPLAY_BRIGHTNESS.load(Ordering::Relaxed);
        i2c.write(ADDR, &[0xE0 + display_brightness]).await.unwrap();

        if current_outside_temp == -128 {
            let frame_index = (Instant::now().as_millis() / (500 / 12)) as usize % 12;
            let mut frame_bytes = [0u8; 17];
            let (byte_index, byte) =
                segment_to_frame_byte(LOADING[frame_index].0, LOADING[frame_index].1).unwrap();
            frame_bytes[byte_index] |= byte;
            i2c.write(ADDR, &frame_bytes).await.unwrap();
            Timer::after(Duration::from_millis(10)).await;
        } else {
            let mut current_outside_temp_string: heapless::String<4> = heapless::String::new();
            let _ = write!(current_outside_temp_string, "{:>3}C", current_outside_temp);
            show!(i2c, current_outside_temp_string.as_str());
            Timer::after(Duration::from_millis(100)).await;
        }
    }
}
