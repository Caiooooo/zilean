use engine::BtConfig;
use log::info;
use logger::init_logger;
use server::{BacktestResponse, ZileanServer};
use zilean::*;
use zmq::Context;

#[tokio::main(flavor = "multi_thread", worker_threads = 16)]
async fn main() {
    init_logger();
    let mut zilean_server = ZileanServer::new();
    let context = Context::new();
    let responder = context.socket(zmq::REP).unwrap();
    responder
        .bind("ipc:///tmp/zilean_backtest.ipc")
        .expect("Failed to bind socket");
    info!("zilean launched🚀 🚀 🚀");
    loop {
        let message = match responder.recv_string(0) {
            Ok(msg) => msg.unwrap(),
            Err(e) => {
                let response = sonic_rs::to_string(&BacktestResponse {
                    backtest_id: (-1).to_string(),
                    status: "error".to_string(),
                    message: "Backtest launched failed.".to_string() + e.to_string().as_str(),
                })
                .unwrap();
                let _ = responder.send(response.as_str(), 0);
                continue;
            }
        };

        if let Some(stripped) = message.strip_prefix("LAUNCH_BACKTEST") {
            let config: BtConfig = match sonic_rs::from_str(stripped) {
                Ok(config) => config,
                Err(e) => {
                    let response = sonic_rs::to_string(&BacktestResponse {
                        backtest_id: (-1).to_string(),
                        status: "error".to_string(),
                        message: "Backtest launched failed.".to_string() + e.to_string().as_str(),
                    })
                    .unwrap();
                    let _ = responder.send(response.as_str(), 0);
                    continue;
                }
            };
            let backtest_id = zilean_server.launch_backtest(config).await;
            let response = sonic_rs::to_string(&BacktestResponse {
                backtest_id,
                status: "ok".to_string(),
                message: "Backtest successfully launched.".to_string(),
            })
            .unwrap();
            let _ = responder.send(response.as_str(), 0);
        }
    }
}
