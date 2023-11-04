use h2::client;
use h2server::calc_stat;
use http::Request;
use std::{
    error::Error,
    fmt::Display,
    num::ParseIntError,
    ops::RangeInclusive,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    net::TcpStream,
    sync::{mpsc, Mutex},
    time,
};

const CONNECTION_TIMEOUT: u64 = 2;
const URL: &str = "127.0.0.1:8080";
const REQUEST_COUNT_RANGE: RangeInclusive<u64> = 1..=100;

#[derive(Default, Debug)]
struct Stat {
    times: Vec<Duration>,
}

impl Stat {
    fn new() -> Self {
        Default::default()
    }

    fn add(&mut self, duration: Duration) {
        self.times.push(duration);
    }

    fn print_state(&self) {
        if !self.times.is_empty() {
            let (min, max, total, avg) = calc_stat(&self.times);
            println!("Total stat:");
            println!("  Number of processed requests: {}", self.times.len());
            println!("  Requests time in millis(max, min, avg): {max:.4} {min:.4} {avg:.4}");
            println!("  Total time in millis: {total:.4}");
        } else {
            println!("There are no requests to the server");
        }
    }
}

enum AppParamError {
    Missed,
    NotInteger(ParseIntError),
    NotInRange,
}

impl Display for AppParamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missed => write!(f, "missed requests count param"),
            Self::NotInteger(e) => write!(f, "bad format of requests count param: {e}"),
            Self::NotInRange => {
                write!(
                    f,
                    "requests count param must be in {:?}",
                    REQUEST_COUNT_RANGE
                )
            }
        }
    }
}

impl std::fmt::Debug for AppParamError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, fmt)
    }
}

impl From<ParseIntError> for AppParamError {
    fn from(value: ParseIntError) -> Self {
        AppParamError::NotInteger(value)
    }
}

impl Error for AppParamError {}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn Error>> {
    let request_count = std::env::args()
        .nth(1)
        .ok_or(AppParamError::Missed)
        .and_then(|str| str.parse::<u64>().map_err(|e| e.into()))?;

    if !REQUEST_COUNT_RANGE.contains(&request_count) {
        return Err(AppParamError::NotInRange.into());
    }

    let mut stats = Stat::new();
    let duration = Duration::from_secs(CONNECTION_TIMEOUT);
    let tcp: Result<TcpStream, Box<dyn Error>> =
        match time::timeout(duration, TcpStream::connect(URL)).await {
            Ok(Ok(tcp)) => Ok(tcp),
            Ok(Err(e)) => {
                eprintln!("Can't connect to server {e:?}");
                Err(e.into())
            }
            Err(e) => {
                eprintln!("Timeout expired during connecting to server");
                Err(e.into())
            }
        };
    let tcp = tcp?;
    let (client, conn) = client::handshake(tcp).await?;
    tokio::spawn(conn);

    println!("sending request");
    let (tx, mut rx) = mpsc::channel::<Result<Duration, h2::Error>>(1);
    let client = Arc::new(Mutex::new(client));

    for _ in 0..request_count {
        let client = Arc::clone(&client);
        let future = async move {
            let instant = Instant::now();

            // NOTE: for tests we use an empty data
            let request = Request::builder()
                .body(())
                .expect("could not create empty request");

            let (response, _stream) = client.lock().await.send_request(request, true)?;

            //NOTE: for tests we don't use response data
            let _response = response.await?;

            Ok(instant.elapsed())
        };
        let tx = tx.clone();
        tokio::spawn(async move {
            if tx.send(future.await).await.is_err() {
                eprintln!("response receiver is dropped");
            }
        });
    }

    drop(tx);

    while let Some(res) = rx.recv().await {
        match res {
            Ok(duration) => stats.add(duration),
            Err(e) => eprintln!("There is error in response: {e:?}"),
        }
    }

    stats.print_state();

    Ok(())
}
