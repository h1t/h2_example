use h2::client;
use h2server::calc_stat;
use http::Request;
use std::{
    error::Error,
    fmt::Display,
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
    fn add(&mut self, duration: Duration) {
        self.times.push(duration);
    }

    fn print_state(&self) {
        if !self.times.is_empty() {
            let (min, max, total) = calc_stat(&self.times);
            let avg_time = total / self.times.len() as f64;
            println!("Total stat:");
            println!("  Processed request count: {}", self.times.len());
            println!("  Max, min, avg request time in millis: {max:.4} {min:.4} {avg_time:.4}");
            println!("  Total time in millis: {total:.4}");
        } else {
            println!("There are no requests to the server");
        }
    }
}

struct ParamRangeError(RangeInclusive<u64>);

impl Error for ParamRangeError {}

impl Display for ParamRangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Request count param must be in {:?}", self.0)
    }
}

impl std::fmt::Debug for ParamRangeError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, fmt)
    }
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn Error>> {
    let request_count = std::env::args()
        .nth(1)
        .expect("There is no parameter for number of requests");
    let request_count = request_count.parse::<u64>()?;

    if !REQUEST_COUNT_RANGE.contains(&request_count) {
        return Err(ParamRangeError(REQUEST_COUNT_RANGE).into());
    }

    let mut stats: Stat = Default::default();
    let duration = Duration::from_secs(CONNECTION_TIMEOUT);
    let tcp: Result<TcpStream, Box<dyn Error>> =
        match time::timeout(duration, TcpStream::connect(URL)).await {
            Ok(Ok(tcp)) => Ok(tcp),
            Ok(Err(e)) => {
                println!("Can't connect to server {e:?}");
                Err(e.into())
            }
            Err(e) => {
                println!("Timeout expired during connecting to server");
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
            let request = Request::builder()
                .body(())
                .expect("could not create empty request");
            let (response, _stream) = client.lock().await.send_request(request, true)?;

            let _response = response.await?;
            Ok(instant.elapsed())
        };
        let tx = tx.clone();
        tokio::spawn(async move { _ = tx.send(future.await).await });
    }

    drop(tx);
    while let Some(res) = rx.recv().await {
        match res {
            Ok(duration) => stats.add(duration),
            Err(e) => println!("There is error in response: {e:?}"),
        }
    }

    stats.print_state();

    Ok(())
}
