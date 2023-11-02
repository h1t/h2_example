use bytes::Bytes;
use h2::server::{self, SendResponse};
use h2::RecvStream;
use h2server::calc_stat;
use http::Response;
use http::{Request, StatusCode};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::error::Error;
use std::ops::RangeInclusive;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::{signal, time};

const URL: &str = "127.0.0.1:8080";
const MAX_CONNECTIONS: usize = 5;
const TIMEOUT_RANGE: RangeInclusive<u64> = 100..=500;

#[derive(Default, Debug)]
struct Stat {
    wait_conn_count: u64,
    times: Vec<Duration>,
}

impl Stat {
    fn print_state(&self) {
        if !self.times.is_empty() {
            let (min, max, total) = calc_stat(&self.times);
            let avg_time = total / self.times.len() as f64;
            println!("Total stat:");
            println!("  Processed connection count: {}", self.times.len());
            println!(
                "  Out of service connection count: {}",
                self.wait_conn_count
            );
            println!("  Max, min, avg connection time in millis: {max:.4} {min:.4} {avg_time:.4}");
        } else {
            println!("There are no connections to the server");
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let is_running = Arc::new(Mutex::new(true));
    let stats: Arc<Mutex<Stat>> = Default::default();

    let listener = TcpListener::bind(URL).await?;

    println!("listening on {:?}", listener.local_addr());

    let (tx, mut rx) = mpsc::channel::<TcpStream>(MAX_CONNECTIONS);
    let is_server_running = Arc::clone(&is_running);
    let server_stats = Arc::clone(&stats);

    let handle = tokio::spawn(async move {
        while let Some(socket) = rx.recv().await {
            let is_server_running = { *(is_server_running.lock().await) };
            if is_server_running {
                let server_stats = Arc::clone(&server_stats);
                tokio::spawn(async move {
                    match serve(socket).await {
                        Ok(duration) => {
                            if duration != Duration::ZERO {
                                let mut lock = server_stats.lock().await;
                                lock.times.push(duration);
                            }
                        }
                        Err(e) => println!("ERROR: {e:?}"),
                    }
                });
            } else {
                let mut lock = server_stats.lock().await;
                lock.wait_conn_count += 1;
            }
        }
    });

    loop {
        tokio::select! {
            _ = signal::ctrl_c() => {
                let mut is_running = is_running.lock().await;
                *is_running = false;

                drop(tx);
                break
            },

            res = listener.accept() => {
                if let Ok((socket, _peer_addr)) = res {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        if (tx.send(socket).await).is_err() {
                            println!("ERROR: tcp stream recivier dropped");
                        }
                    });
                }
            }
        }
    }

    println!("--------- stop ----------");

    handle.await?;

    let stats = stats.lock().await;
    stats.print_state();

    Ok(())
}

async fn serve(socket: TcpStream) -> Result<Duration, Box<dyn Error + Send + Sync>> {
    let mut connection = server::handshake(socket).await?;
    let mut rng = StdRng::from_rng(rand::thread_rng())?;
    let times: Arc<Mutex<Vec<Duration>>> = Default::default();

    while let Some(result) = connection.accept().await {
        let (request, respond) = result?;
        let duration = Duration::from_millis(rng.gen_range(TIMEOUT_RANGE));
        let times = Arc::clone(&times);

        tokio::spawn(async move {
            match time::timeout(duration, handle_request(request)).await {
                Ok(Ok((response, time))) => {
                    let res = send_response(respond, response);
                    {
                        let mut lock = times.lock().await;
                        lock.push(time);
                    }
                    res
                }
                Ok(Err(_)) => send_error_response(respond, StatusCode::INTERNAL_SERVER_ERROR),
                Err(_) => send_error_response(respond, StatusCode::REQUEST_TIMEOUT),
            }
        });
    }

    let total_time = {
        let lock = times.lock().await;
        if !lock.is_empty() {
            let request_count = lock.len();
            let (min, max, total) = calc_stat(&lock);
            let avg_time = total / request_count as f64;

            println!(
                "Connection stat(count, max, min, avg) in millis: {request_count} {max:.4} {min:.4} {avg_time:.4}"
            );
            lock.iter().sum()
        } else {
            println!("Connection stat: empty");
            Duration::ZERO
        }
    };

    Ok(total_time)
}

fn send_error_response(
    mut respond: SendResponse<Bytes>,
    status_code: StatusCode,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let response = Response::builder().status(status_code).body(())?;

    respond.send_response(response, true)?;
    Ok(())
}

fn send_response(
    mut respond: SendResponse<Bytes>,
    response: Response<()>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    respond.send_response(response, true)?;
    Ok(())
}

async fn handle_request(
    _request: Request<RecvStream>,
) -> Result<(Response<()>, Duration), Box<dyn Error + Send + Sync>> {
    let instant = Instant::now();

    // NOTE: for test purpose only!!!
    let millis = rand::thread_rng().gen_range(400..=600);
    time::sleep(Duration::from_millis(millis)).await;

    // get data from request
    // ...

    // prepare response data
    // ...

    // for tests we use an empty data
    let response = Response::builder().status(StatusCode::OK).body(())?;

    Ok((response, instant.elapsed()))
}
