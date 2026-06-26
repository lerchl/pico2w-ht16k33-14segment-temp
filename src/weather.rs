use core::cmp;

use cyw43::{Control, JoinOptions};
use embassy_net::Stack;
use embassy_net::dns::DnsSocket;
use embassy_net::tcp::client::TcpClient;
use embassy_time::{Duration, Instant, Timer};
use embedded_io_async::Read;
use reqwless::{client::HttpClient, request::Method};

use crate::animation::{
    Animation, CLOUDY, FOG, HEAVY_RAIN, HEAVY_SNOW, LIGHTNING, PARTIALLY_CLOUDY, QUESTION_MARKS,
    RAIN, SNOW, SUNSHINE,
};

#[derive(Debug)]
pub enum WifiError {
    MultipleFailedJoins,
    LinkTimeout,
    DhcpTimeout,
}

impl WifiError {
    pub fn code(&self) -> &'static str {
        match self {
            WifiError::MultipleFailedJoins => "WE01",
            WifiError::LinkTimeout => "WE02",
            WifiError::DhcpTimeout => "WE03",
        }
    }
}

impl From<WifiError> for OutsideTemperatureTaskError {
    fn from(e: WifiError) -> Self {
        OutsideTemperatureTaskError::Wifi(e)
    }
}

#[derive(Debug)]
pub enum RequestError {
    Request,
    Send,
    ReadingResponse,
    Utf8Error,
    ParsingResponse,
}

impl RequestError {
    pub fn code(&self) -> &'static str {
        match self {
            RequestError::Request => "RE01",
            RequestError::Send => "RE02",
            RequestError::ReadingResponse => "RE03",
            RequestError::Utf8Error => "RE04",
            RequestError::ParsingResponse => "RE05",
        }
    }
}

impl From<RequestError> for OutsideTemperatureTaskError {
    fn from(e: RequestError) -> Self {
        OutsideTemperatureTaskError::Request(e)
    }
}

#[derive(Debug)]
pub enum OutsideTemperatureTaskError {
    Wifi(WifiError),
    Request(RequestError),
}

impl OutsideTemperatureTaskError {
    pub fn code(&self) -> &'static str {
        match self {
            OutsideTemperatureTaskError::Wifi(e) => e.code(),
            OutsideTemperatureTaskError::Request(e) => e.code(),
        }
    }
}

#[derive(Clone, Copy)]
pub enum Condition {
    Unknown,
    Sunny,
    Cloudy,
    PartlyCloudy,
    Fog,
    LightRain,
    HeavyRain,
    LightSnow,
    HeavySnow,
    ThunderyRain,
    ThunderyHeavyRain,
}

impl Condition {
    pub fn animation(&self) -> &Animation {
        match self {
            Condition::Unknown => &QUESTION_MARKS,
            Condition::Sunny => &SUNSHINE,
            Condition::Cloudy => &CLOUDY,
            Condition::PartlyCloudy => &PARTIALLY_CLOUDY,
            Condition::Fog => &FOG,
            Condition::LightRain => &RAIN,
            Condition::HeavyRain => &HEAVY_RAIN,
            Condition::LightSnow => &SNOW,
            Condition::HeavySnow => &HEAVY_SNOW,
            Condition::ThunderyRain => &LIGHTNING,
            Condition::ThunderyHeavyRain => &LIGHTNING,
        }
    }
}

#[derive(Clone, Copy)]
pub enum Direction {
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    NorthWest,
}

impl core::fmt::Display for Direction {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            Direction::North => "N",
            Direction::NorthEast => "NE",
            Direction::East => "E",
            Direction::SouthEast => "SE",
            Direction::South => "S",
            Direction::SouthWest => "SW",
            Direction::West => "W",
            Direction::NorthWest => "NW",
        };
        write!(f, "{:>2}", s)
    }
}

#[derive(Clone, Copy)]
pub struct Wind {
    pub direction: Direction,
    pub speed_kmh: u8,
}

impl core::fmt::Display for Wind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:>2}{}", cmp::min(self.speed_kmh, 99), self.direction)
    }
}

#[derive(Clone, Copy)]
pub struct Weather {
    pub condition: Condition,
    pub temperature_c: i8,
    pub wind: Wind,
}

impl core::fmt::Display for Condition {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Condition::Unknown => write!(f, "Unknown"),
            Condition::Sunny => write!(f, "Sunny"),
            Condition::Cloudy => write!(f, "Cloudy"),
            Condition::PartlyCloudy => write!(f, "Partly Cloudy"),
            Condition::Fog => write!(f, "Fog"),
            Condition::LightRain => write!(f, "Light Rain"),
            Condition::HeavyRain => write!(f, "Heavy Rain"),
            Condition::LightSnow => write!(f, "Light Snow"),
            Condition::HeavySnow => write!(f, "Heavy Snow"),
            Condition::ThunderyRain => write!(f, "Thundery Rain"),
            Condition::ThunderyHeavyRain => write!(f, "Thundery Heavy Rain"),
        }
    }
}

fn parse_condition(s: &str) -> Result<Condition, RequestError> {
    match s {
        s if s.contains('⛈') => Ok(Condition::ThunderyRain),
        s if s.contains('🌩') => Ok(Condition::ThunderyHeavyRain),
        s if s.contains('❄') => Ok(Condition::HeavySnow),
        s if s.contains('🌨') => Ok(Condition::LightSnow),
        s if s.contains('🌦') => Ok(Condition::LightRain),
        s if s.contains('🌧') => Ok(Condition::HeavyRain),
        s if s.contains('🌫') => Ok(Condition::Fog),
        s if s.contains('⛅') => Ok(Condition::PartlyCloudy),
        s if s.contains('☀') => Ok(Condition::Sunny),
        s if s.contains('☁') => Ok(Condition::Cloudy),
        s if s.contains('✨') => Ok(Condition::Unknown),
        _ => Err(RequestError::ParsingResponse),
    }
}

fn parse_direction(arrow: char) -> Result<Direction, RequestError> {
    match arrow {
        '↑' => Ok(Direction::North),
        '↗' => Ok(Direction::NorthEast),
        '→' => Ok(Direction::East),
        '↘' => Ok(Direction::SouthEast),
        '↓' => Ok(Direction::South),
        '↙' => Ok(Direction::SouthWest),
        '←' => Ok(Direction::West),
        '↖' => Ok(Direction::NorthWest),
        _ => Err(RequestError::ParsingResponse),
    }
}

fn parse_wind(s: &str) -> Result<Wind, RequestError> {
    // strip leading wind emoji (🌬️ is emoji + variation selector, 2 chars)
    let s = s.trim_start_matches(|c: char| {
        !c.is_ascii_digit()
            && c != '↑'
            && c != '↗'
            && c != '→'
            && c != '↘'
            && c != '↓'
            && c != '↙'
            && c != '←'
            && c != '↖'
    });

    let mut chars = s.chars();
    let arrow = chars.next().ok_or(RequestError::ParsingResponse)?;
    let direction = parse_direction(arrow)?;

    let rest: &str = chars.as_str();
    let speed_str = rest.trim_end_matches("km/h");
    let speed_kmh = speed_str
        .parse::<u8>()
        .map_err(|_| RequestError::ParsingResponse)?;

    Ok(Wind {
        direction,
        speed_kmh,
    })
}

async fn join_wifi(
    control: &mut Control<'_>,
    stack: &Stack<'_>,
    wifi_ssid: &str,
    wifi_password: &str,
) -> Result<(), WifiError> {
    let join_deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if Instant::now() > join_deadline {
            return Err(WifiError::MultipleFailedJoins);
        }

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
                Timer::after(Duration::from_secs(5)).await;
            }
        }
    }

    let link_deadline = Instant::now() + Duration::from_secs(30);
    while !stack.is_link_up() {
        if Instant::now() > link_deadline {
            return Err(WifiError::LinkTimeout);
        }
        log::trace!("Waiting for link up...");
        Timer::after(Duration::from_millis(500)).await;
    }

    log::trace!("Waiting for DHCP...");
    embassy_time::with_timeout(Duration::from_secs(30), stack.wait_config_up())
        .await
        .map_err(|_| WifiError::DhcpTimeout)?;

    log::info!("Network up: {:?}", stack.config_v4());
    Ok(())
}

async fn send_request(
    client: &mut HttpClient<'_, TcpClient<'_, 2>, DnsSocket<'_>>,
    wttr_url: &str,
) -> Result<Weather, RequestError> {
    log::trace!("Creating http request...");
    let mut req = client
        .request(Method::GET, wttr_url)
        .await
        .map_err(|_| RequestError::Request)?;

    log::trace!("Sending http request...");
    let mut rx_buf = [0u8; 4096];
    let response = req
        .send(&mut rx_buf)
        .await
        .map_err(|_| RequestError::Send)?;

    log::trace!("Reading http response...");
    let mut body_buf = [0u8; 64];
    let n = response
        .body()
        .reader()
        .read(&mut body_buf)
        .await
        .map_err(|_| RequestError::ReadingResponse)?;

    log::trace!("Parsing http response...");
    let s = core::str::from_utf8(&body_buf[..n]).map_err(|_| RequestError::Utf8Error)?;

    let mut parts = s.trim().split_whitespace();
    let condition_tok = parts.next().ok_or(RequestError::ParsingResponse)?;
    let temp_tok = parts.next().ok_or(RequestError::ParsingResponse)?;
    let wind_tok = parts.next().ok_or(RequestError::ParsingResponse)?;

    let condition = parse_condition(condition_tok)?;

    let temperature_c = temp_tok
        .trim_start_matches(|c: char| !c.is_ascii_digit() && c != '-')
        .trim_end_matches("°C")
        .parse::<i8>()
        .map_err(|_| RequestError::ParsingResponse)?;

    let wind = parse_wind(wind_tok)?;

    Ok(Weather {
        condition,
        temperature_c,
        wind,
    })
}

pub async fn fetch(
    control: &mut Control<'_>,
    stack: &Stack<'_>,
    client: &mut HttpClient<'_, TcpClient<'_, 2>, DnsSocket<'_>>,
    wifi_ssid: &str,
    wifi_password: &str,
    wttr_url: &str,
) -> Result<Weather, OutsideTemperatureTaskError> {
    if !stack.is_link_up() {
        join_wifi(control, stack, wifi_ssid, wifi_password).await?;
    }

    Ok(send_request(client, wttr_url).await?)
}
