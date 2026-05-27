#![no_std]
#![no_main]

use cyw43::{JoinOptions, aligned_bytes};
use cyw43_pio::{PioSpi, RM2_CLOCK_DIVIDER};
use embassy_executor::Spawner;
use embassy_net::StackResources;
use embassy_net::dns::DnsSocket;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_rp::gpio::{Level, Output};
use embassy_rp::i2c::{Async, Config, I2c};
use embassy_rp::peripherals::{DMA_CH0, DMA_CH1, I2C1, PIO0, USB};
use embassy_rp::pio::Pio;
use embassy_rp::usb::Driver;
use embassy_rp::{bind_interrupts, dma};
use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex};
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use embedded_hal_async::i2c::I2c as _;
use embedded_io_async::Read;
use heapless::Vec;
use reqwless::client::HttpClient;
use reqwless::request::Method;
use static_cell::StaticCell;

use {defmt_rtt as _, panic_probe as _};

// byte: 0b12345678
//         ||||||||
//         |||||||+-- bit 8 (LSB)
//         ||||||+--- bit 7
//         |||||+---- bit 6
//         ||||+----- bit 5
//         |||+------ bit 4
//         ||+------- bit 3
//         |+-------- bit 2
//         +--------- bit 1 (MSB)

// Frame index (3 and 4|5 and 6|7 and 8|9 and 10) represent some
// segments of the (first|second|third|fourth) character on the
// display. The first number indicates whether it's the first or the
// second byte. The second number is which bit of that byte. Some bits
// do nothing.

//         1,4
//     ___________
//    |\ 2,3|    /|
//    | |   |   | |
//    |  \  |  /  |
// 2,2|   | | |   |1,2
//    | 1,1\|/    |
//    |     |     |
//  2,7----- -----2,8
//    |     |     |
//    | 2,4/|\    |
//    |   | | |   |
// 1,5|  /  |  \  |1,3
//    | |   |   | |
//    |/ 2,5|    \|
//     ‾‾‾‾‾‾‾‾‾‾‾
//         2,6

// Frame index 11 and 12 represent the remaining segments all over the
// four characters and the two middle dots. The first number indicates
// whether it's the first or the second byte, frame index 11 or 12. The
// second number is which bit of that byte. Some bits do nothing.

//
//     ___________        ___________         ___________        ___________
//    |\    |    /|      |\    |    /|       |\    |    /|      |\    |    /|
//    | |   |   | |      | |   |   | |       | |   |   | |      | |   |   | |
//    |  \  |  /  |      |  \  |  /  |  1,1  |  \  |  /  |      |  \  |  /  |
//    |   | | |   |      |   | | |   |   O   |   | | |   |      |   | | |   |
//    |    \|/1,4 |      |    \|/1,2 |       |    \|/1,3 |      |    \|/2,6 |
//    |     |     |      |     |     |       |     |     |      |     |     |
//     ----- -----        ----- -----         ----- -----        ----- -----
//    |     |     |      |     |     |       |     |     |      |     |     |
//    |    /|\1,5 |      |    /|\2,2 |       |    /|\2,7 |      |    /|\2,8 |
//    |   | | |   |      |   | | |   |   O   |   | | |   |      |   | | |   |
//    |  /  |  \  |      |  /  |  \  |  2,3  |  /  |  \  |      |  /  |  \  |
//    | |   |   | |      | |   |   | |       | |   |   | |      | |   |   | |
//    |/    |    \|      |/    |    \|       |/    |    \|      |/    |    \|
//     ‾‾‾‾‾‾‾‾‾‾‾        ‾‾‾‾‾‾‾‾‾‾‾         ‾‾‾‾‾‾‾‾‾‾‾        ‾‾‾‾‾‾‾‾‾‾‾
//

const ADDR: u8 = 0x71;

bind_interrupts!(struct Irqs {
    I2C1_IRQ => embassy_rp::i2c::InterruptHandler<I2C1>;
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<PIO0>;
    DMA_IRQ_0 => dma::InterruptHandler<DMA_CH0>, dma::InterruptHandler<DMA_CH1>;
    USBCTRL_IRQ => embassy_rp::usb::InterruptHandler<USB>;
});

#[derive(Clone, Copy, Debug)]
enum Segment {
    TopHorizontal,
    TopLeftVertical,
    TopLeftDiagonal,
    TopMiddleVertical,
    TopRightDiagonal,
    TopRightVertical,
    MiddleLeft,
    MiddleRight,
    BottomLeftVertical,
    BottomLeftDiagonal,
    BottomMiddleVertical,
    BottomRightDiagonal,
    BottomRightVertical,
    BottomHorizontal,
}

#[derive(Debug)]
enum DisplayError {
    InvalidCharacterIndex(usize),
    UnknownGlyph(char),
}

struct Character {
    index: usize,
    segments: Vec<Segment, 14>,
}

struct Display {
    characters: [Character; 4],
}

impl Display {
    fn from_str(s: &str) -> Result<Self, DisplayError> {
        let mut characters = [
            Character {
                index: 1,
                segments: Vec::new(),
            },
            Character {
                index: 2,
                segments: Vec::new(),
            },
            Character {
                index: 3,
                segments: Vec::new(),
            },
            Character {
                index: 4,
                segments: Vec::new(),
            },
        ];

        for (i, c) in s.chars().enumerate().take(4) {
            let segments = glyph(c)?;
            characters[i] = Character {
                index: i + 1,
                segments: segments.iter().copied().collect(),
            };
        }
        Ok(Self { characters })
    }

    fn to_frame(&self) -> Result<[u8; 17], DisplayError> {
        let mut frame = [0u8; 17];
        for character in &self.characters {
            for &segment in &character.segments {
                let (index, mask) = segment_to_frame_byte(character.index, segment)?;
                frame[index] |= mask;
            }
        }
        Ok(frame)
    }
}

fn glyph(c: char) -> Result<&'static [Segment], DisplayError> {
    Ok(match c {
        'A' => &[
            Segment::TopHorizontal,
            Segment::TopLeftVertical,
            Segment::TopRightVertical,
            Segment::MiddleLeft,
            Segment::MiddleRight,
            Segment::BottomLeftVertical,
            Segment::BottomRightVertical,
        ],
        'B' => &[
            Segment::TopHorizontal,
            Segment::TopRightVertical,
            Segment::TopMiddleVertical,
            Segment::MiddleLeft,
            Segment::MiddleRight,
            Segment::BottomRightVertical,
            Segment::BottomMiddleVertical,
            Segment::BottomHorizontal,
        ],
        'C' => &[
            Segment::TopHorizontal,
            Segment::TopLeftVertical,
            Segment::BottomLeftVertical,
            Segment::BottomHorizontal,
        ],
        'D' => &[
            Segment::TopHorizontal,
            Segment::TopRightVertical,
            Segment::TopMiddleVertical,
            Segment::BottomRightVertical,
            Segment::BottomMiddleVertical,
            Segment::BottomHorizontal,
        ],
        'E' => &[
            Segment::TopHorizontal,
            Segment::TopLeftVertical,
            Segment::MiddleLeft,
            Segment::BottomLeftVertical,
            Segment::BottomHorizontal,
        ],
        'F' => &[
            Segment::TopHorizontal,
            Segment::TopLeftVertical,
            Segment::MiddleLeft,
            Segment::BottomLeftVertical,
        ],
        'G' => &[
            Segment::TopHorizontal,
            Segment::TopLeftVertical,
            Segment::MiddleRight,
            Segment::BottomLeftVertical,
            Segment::BottomRightVertical,
            Segment::BottomHorizontal,
        ],
        'H' => &[
            Segment::TopLeftVertical,
            Segment::TopRightVertical,
            Segment::MiddleLeft,
            Segment::MiddleRight,
            Segment::BottomLeftVertical,
            Segment::BottomRightVertical,
        ],
        'I' => &[
            Segment::TopHorizontal,
            Segment::TopMiddleVertical,
            Segment::BottomMiddleVertical,
            Segment::BottomHorizontal,
        ],
        'J' => &[
            Segment::TopRightVertical,
            Segment::BottomLeftVertical,
            Segment::BottomRightVertical,
            Segment::BottomHorizontal,
        ],
        'K' => &[
            Segment::TopLeftVertical,
            Segment::TopRightDiagonal,
            Segment::MiddleLeft,
            Segment::BottomLeftVertical,
            Segment::BottomRightDiagonal,
        ],
        'L' => &[
            Segment::TopLeftVertical,
            Segment::BottomLeftVertical,
            Segment::BottomHorizontal,
        ],
        'M' => &[
            Segment::TopLeftVertical,
            Segment::TopLeftDiagonal,
            Segment::TopRightDiagonal,
            Segment::TopRightVertical,
            Segment::BottomLeftVertical,
            Segment::BottomRightVertical,
        ],
        'N' => &[
            Segment::TopLeftVertical,
            Segment::TopLeftDiagonal,
            Segment::TopRightVertical,
            Segment::BottomLeftVertical,
            Segment::BottomRightVertical,
            Segment::BottomRightDiagonal,
        ],
        'O' => &[
            Segment::TopHorizontal,
            Segment::TopLeftVertical,
            Segment::TopRightVertical,
            Segment::BottomLeftVertical,
            Segment::BottomRightVertical,
            Segment::BottomHorizontal,
        ],
        'P' => &[
            Segment::TopHorizontal,
            Segment::TopLeftVertical,
            Segment::TopRightVertical,
            Segment::MiddleLeft,
            Segment::MiddleRight,
            Segment::BottomLeftVertical,
        ],
        'Q' => &[
            Segment::TopHorizontal,
            Segment::TopLeftVertical,
            Segment::TopRightVertical,
            Segment::BottomLeftVertical,
            Segment::BottomRightVertical,
            Segment::BottomHorizontal,
            Segment::BottomRightDiagonal,
        ],
        'R' => &[
            Segment::TopHorizontal,
            Segment::TopLeftVertical,
            Segment::TopRightVertical,
            Segment::MiddleLeft,
            Segment::MiddleRight,
            Segment::BottomLeftVertical,
            Segment::BottomRightDiagonal,
        ],
        'S' => &[
            Segment::TopHorizontal,
            Segment::TopLeftVertical,
            Segment::MiddleLeft,
            Segment::MiddleRight,
            Segment::BottomRightVertical,
            Segment::BottomHorizontal,
        ],
        'T' => &[
            Segment::TopHorizontal,
            Segment::TopMiddleVertical,
            Segment::BottomMiddleVertical,
        ],
        'U' => &[
            Segment::TopLeftVertical,
            Segment::TopRightVertical,
            Segment::BottomLeftVertical,
            Segment::BottomRightVertical,
            Segment::BottomHorizontal,
        ],
        'V' => &[
            Segment::TopLeftVertical,
            Segment::BottomLeftVertical,
            Segment::BottomLeftDiagonal,
            Segment::TopRightDiagonal,
        ],
        'W' => &[
            Segment::TopLeftVertical,
            Segment::TopRightVertical,
            Segment::BottomLeftVertical,
            Segment::BottomLeftDiagonal,
            Segment::BottomRightDiagonal,
            Segment::BottomRightVertical,
        ],
        'X' => &[
            Segment::TopLeftDiagonal,
            Segment::TopRightDiagonal,
            Segment::BottomLeftDiagonal,
            Segment::BottomRightDiagonal,
        ],
        'Y' => &[
            Segment::TopLeftDiagonal,
            Segment::TopRightDiagonal,
            Segment::BottomMiddleVertical,
        ],
        'Z' => &[
            Segment::TopHorizontal,
            Segment::TopRightDiagonal,
            Segment::BottomLeftDiagonal,
            Segment::BottomHorizontal,
        ],
        '0' => &[
            Segment::TopHorizontal,
            Segment::TopLeftVertical,
            Segment::TopRightVertical,
            Segment::TopRightDiagonal,
            Segment::BottomLeftVertical,
            Segment::BottomRightVertical,
            Segment::BottomLeftDiagonal,
            Segment::BottomHorizontal,
        ],
        '1' => &[
            Segment::TopRightDiagonal,
            Segment::TopRightVertical,
            Segment::BottomRightVertical,
        ],
        '2' => &[
            Segment::TopHorizontal,
            Segment::TopRightVertical,
            Segment::MiddleLeft,
            Segment::MiddleRight,
            Segment::BottomLeftVertical,
            Segment::BottomHorizontal,
        ],
        '3' => &[
            Segment::TopHorizontal,
            Segment::TopRightVertical,
            Segment::MiddleLeft,
            Segment::MiddleRight,
            Segment::BottomRightVertical,
            Segment::BottomHorizontal,
        ],
        '4' => &[
            Segment::TopLeftVertical,
            Segment::TopRightVertical,
            Segment::MiddleLeft,
            Segment::MiddleRight,
            Segment::BottomRightVertical,
        ],
        '5' => &[
            Segment::TopHorizontal,
            Segment::TopLeftVertical,
            Segment::MiddleLeft,
            Segment::MiddleRight,
            Segment::BottomRightVertical,
            Segment::BottomHorizontal,
        ],
        '6' => &[
            Segment::TopHorizontal,
            Segment::TopLeftVertical,
            Segment::MiddleLeft,
            Segment::MiddleRight,
            Segment::BottomLeftVertical,
            Segment::BottomRightVertical,
            Segment::BottomHorizontal,
        ],
        '7' => &[
            Segment::TopHorizontal,
            Segment::TopRightVertical,
            Segment::BottomRightVertical,
        ],
        '8' => &[
            Segment::TopHorizontal,
            Segment::TopLeftVertical,
            Segment::TopRightVertical,
            Segment::MiddleLeft,
            Segment::MiddleRight,
            Segment::BottomLeftVertical,
            Segment::BottomRightVertical,
            Segment::BottomHorizontal,
        ],
        '9' => &[
            Segment::TopHorizontal,
            Segment::TopLeftVertical,
            Segment::TopRightVertical,
            Segment::MiddleLeft,
            Segment::MiddleRight,
            Segment::BottomRightVertical,
            Segment::BottomHorizontal,
        ],
        '-' => &[Segment::MiddleLeft, Segment::MiddleRight],
        ' ' => &[],
        _ => return Err(DisplayError::UnknownGlyph(c)),
    })
}

fn segment_to_frame_byte(
    character_index: usize,
    segment: Segment,
) -> Result<(usize, u8), DisplayError> {
    if character_index < 1 || character_index > 4 {
        return Err(DisplayError::InvalidCharacterIndex(character_index));
    }

    let char_first_byte_index = 3 + (character_index - 1) * 2;

    let top_right_diagonal_bitmask = || match character_index {
        1 => 0b00010000,
        2 => 0b01000000,
        3 => 0b00100000,
        4 => 0b00000100,
        _ => unreachable!(),
    };

    let bottom_right_diagonal_bitmask = || match character_index {
        1 => 0b00001000,
        2 => 0b01000000,
        3 => 0b00000010,
        4 => 0b00000001,
        _ => unreachable!(),
    };

    Ok(match segment {
        Segment::TopHorizontal => (char_first_byte_index, 0b00010000),
        Segment::TopLeftVertical => (char_first_byte_index + 1, 0b01000000),
        Segment::TopLeftDiagonal => (char_first_byte_index, 0b10000000),
        Segment::TopMiddleVertical => (char_first_byte_index + 1, 0b00100000),
        // special
        Segment::TopRightDiagonal => (
            if character_index == 4 { 12 } else { 11 },
            top_right_diagonal_bitmask(),
        ),
        Segment::TopRightVertical => (char_first_byte_index, 0b01000000),
        Segment::MiddleLeft => (char_first_byte_index + 1, 0b00000010),
        Segment::MiddleRight => (char_first_byte_index + 1, 0b00000001),
        Segment::BottomLeftVertical => (char_first_byte_index, 0b00001000),
        Segment::BottomLeftDiagonal => (char_first_byte_index + 1, 0b00010000),
        Segment::BottomMiddleVertical => (char_first_byte_index + 1, 0b00001000),
        // special
        Segment::BottomRightDiagonal => (
            if character_index == 1 { 11 } else { 12 },
            bottom_right_diagonal_bitmask(),
        ),
        Segment::BottomRightVertical => (char_first_byte_index, 0b00100000),
        Segment::BottomHorizontal => (char_first_byte_index + 1, 0b00000100),
    })
}

static STOP_ANIM: Signal<CriticalSectionRawMutex, ()> = Signal::new();

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
async fn anim_task(mut i2c: I2c<'static, I2C1, Async>) {
    let frames = [
        (1, Segment::TopHorizontal),
        (2, Segment::TopHorizontal),
        (3, Segment::TopHorizontal),
        (4, Segment::TopHorizontal),
        (4, Segment::TopRightVertical),
        (4, Segment::BottomRightVertical),
        (4, Segment::BottomHorizontal),
        (3, Segment::BottomHorizontal),
        (2, Segment::BottomHorizontal),
        (1, Segment::BottomHorizontal),
        (1, Segment::BottomLeftVertical),
        (1, Segment::TopLeftVertical),
    ];

    let mut i = 0;

    loop {
        embassy_futures::select::select(Timer::after(Duration::from_millis(150)), STOP_ANIM.wait())
            .await;

        if STOP_ANIM.try_take().is_some() {
            break;
        }

        let mut frame = [0u8; 17];
        let (character_index, segment) = frames[i % frames.len()];
        let (index, mask) = segment_to_frame_byte(character_index, segment).unwrap();
        frame[index] = mask;

        i2c.write(ADDR, &frame).await;
        i += 1;
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    const WIFI_SSID: &str = env!("WIFI_SSID");
    const WIFI_PASSWORD: &str = env!("WIFI_PASSWORD");
    let p = embassy_rp::init(Default::default());
    let driver = Driver::new(p.USB, Irqs);
    spawner.spawn(logger_task(driver).unwrap());

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
    log::info!("Joining WiFi network '{}'", WIFI_SSID);
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
        let _ = spawner.spawn(anim_task(i2c).unwrap());
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
