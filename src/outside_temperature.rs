use cyw43::{Control, JoinOptions};
use embassy_net::Stack;
use embassy_net::dns::DnsSocket;
use embassy_net::tcp::client::TcpClient;
use embassy_time::{Duration, Instant, Timer};
use embedded_io_async::Read;
use reqwless::{client::HttpClient, request::Method};

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
    ParsingResponse(heapless::String<64>),
}

impl RequestError {
    pub fn code(&self) -> &'static str {
        match self {
            RequestError::Request => "RE01",
            RequestError::Send => "RE02",
            RequestError::ReadingResponse => "RE03",
            RequestError::Utf8Error => "RE04",
            RequestError::ParsingResponse(_) => "RE05",
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
) -> Result<i8, RequestError> {
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
    let s = core::str::from_utf8(&body_buf[..n])
        .map_err(|_| RequestError::Utf8Error)?
        .trim()
        .trim_start_matches('+')
        .trim_end_matches("°C");

    s.parse::<i8>().map_err(|_| {
        RequestError::ParsingResponse(heapless::String::try_from(s).unwrap_or_default())
    })
}

pub async fn fetch(
    control: &mut Control<'_>,
    stack: &Stack<'_>,
    client: &mut HttpClient<'_, TcpClient<'_, 2>, DnsSocket<'_>>,
    wifi_ssid: &str,
    wifi_password: &str,
    wttr_url: &str,
) -> Result<i8, OutsideTemperatureTaskError> {
    if !stack.is_link_up() {
        join_wifi(control, stack, wifi_ssid, wifi_password).await?;
    }

    let temp = send_request(client, wttr_url).await?;
    Ok(temp)
}
