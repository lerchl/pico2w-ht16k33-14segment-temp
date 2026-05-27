#![no_std]
#![no_main]

mod display;
mod glyph;

use cyw43::{JoinOptions, aligned_bytes};
use cyw43_pio::{PioSpi, RM2_CLOCK_DIVIDER};
use display::Display;
use embassy_executor::Spawner;
use embassy_net::StackResources;
use embassy_net::dns::DnsSocket;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_rp::gpio::{Level, Output};
use embassy_rp::i2c::{Config, I2c};
use embassy_rp::peripherals::{DMA_CH0, DMA_CH1, I2C1, PIO0, USB};
use embassy_rp::pio::Pio;
use embassy_rp::usb::Driver;
use embassy_rp::{bind_interrupts, dma};
use embassy_time::{Duration, Timer};
use embedded_hal_async::i2c::I2c as _;
use embedded_io_async::Read;
use reqwless::client::HttpClient;
use reqwless::request::Method;
use static_cell::StaticCell;

use {defmt_rtt as _, panic_probe as _};

const ADDR: u8 = 0x71;

bind_interrupts!(struct Irqs {
    I2C1_IRQ => embassy_rp::i2c::InterruptHandler<I2C1>;
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<PIO0>;
    DMA_IRQ_0 => dma::InterruptHandler<DMA_CH0>, dma::InterruptHandler<DMA_CH1>;
    USBCTRL_IRQ => embassy_rp::usb::InterruptHandler<USB>;
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

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    const WIFI_SSID: &str = env!("WIFI_SSID");
    const WIFI_PASSWORD: &str = env!("WIFI_PASSWORD");

    // Set up peripherals
    let p = embassy_rp::init(Default::default());

    // Set up usb logger
    let driver = Driver::new(p.USB, Irqs);
    spawner.spawn(logger_task(driver).unwrap());

    // Set up i2c aka 14 segment display
    let mut cfg = Config::default();
    cfg.frequency = 100_000;
    let mut i2c = I2c::new_async(p.I2C1, p.PIN_15, p.PIN_14, Irqs, cfg);
    i2c.write(ADDR, &[0x21]).await.unwrap();
    i2c.write(ADDR, &[0x81]).await.unwrap();
    i2c.write(ADDR, &[0xEF]).await.unwrap();

    macro_rules! show {
        ($i2c:expr, $s:expr) => {
            if let Ok(frame) = Display::from_str($s).and_then(|d| d.to_frame()) {
                let _ = $i2c.write(ADDR, &frame).await;
            }
        };
    }

    show!(i2c, "INIT");

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
    control
        .set_power_management(cyw43::PowerManagementMode::None)
        .await;

    show!(i2c, "WIFI");
    log::info!("Joining wifi network '{}'", WIFI_SSID);

    loop {
        match control
            .join(WIFI_SSID, JoinOptions::new(WIFI_PASSWORD.as_bytes()))
            .await
        {
            Ok(_) => {
                log::info!("WiFi joined");
                break;
            }
            Err(e) => {
                log::warn!("WiFi join failed: {:?}, retrying...", e);
                show!(i2c, "WERR");
                Timer::after(Duration::from_secs(2)).await;
                show!(i2c, "WIFI");
            }
        }
    }

    show!(i2c, "LINK");
    while !stack.is_link_up() {
        log::info!("Waiting for link up...");
        Timer::after(Duration::from_millis(500)).await;
    }

    show!(i2c, "DHCP");
    log::info!("Waiting for DHCP...");
    stack.wait_config_up().await;
    log::info!("Network up: {:?}", stack.config_v4());

    static TCP_STATE: StaticCell<TcpClientState<2, 1024, 1024>> = StaticCell::new();
    let tcp_state = TCP_STATE.init(TcpClientState::new());
    let tcp_client = TcpClient::new(stack, tcp_state);
    let dns_socket = DnsSocket::new(stack);
    let mut client = HttpClient::new(&tcp_client, &dns_socket);

    let url = "http://wttr.in/Vienna?format=%t";

    loop {
        show!(i2c, "HTTP");
        log::info!("Fetching weather...");

        let success = 'fetch: {
            let mut req = match client.request(Method::GET, url).await {
                Ok(r) => r,
                Err(e) => {
                    log::error!("HTTP request failed: {:?}", e);
                    break 'fetch false;
                }
            };

            let mut rx_buf = [0u8; 4096];
            let response = match req.send(&mut rx_buf).await {
                Ok(r) => r,
                Err(e) => {
                    log::error!("HTTP send failed: {:?}", e);
                    break 'fetch false;
                }
            };

            let mut body_buf = [0u8; 64];
            let n = match response.body().reader().read(&mut body_buf).await {
                Ok(n) => n,
                Err(e) => {
                    log::error!("Body read error: {:?}", e);
                    break 'fetch false;
                }
            };

            let s = match core::str::from_utf8(&body_buf[..n]) {
                Ok(s) => s.trim().trim_start_matches('+').trim_end_matches("°C"),
                Err(e) => {
                    log::error!("Body not valid UTF-8: {:?}", e);
                    break 'fetch false;
                }
            };
            log::info!("s: {}", s);

            let mut aligned: heapless::String<4> = heapless::String::new();
            use core::fmt::Write;
            let _ = write!(aligned, "{:>3}C", s);

            log::info!("Showing: {}", aligned.as_str());

            show!(i2c, aligned.as_str());

            true
        };

        if success {
            Timer::after(Duration::from_secs(60)).await;
        } else {
            show!(i2c, "ERR ");
            Timer::after(Duration::from_secs(5)).await;
        }
    }
}
