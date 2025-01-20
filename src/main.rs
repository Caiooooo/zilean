use engine::BtConfig;
use log::info;
use logger::init_logger;
use server::{BacktestResponse, ZileanServer};
use zilean::*;
use zmq::Context;



#[tokio::main(flavor = "multi_thread", worker_threads = 192)] // num_cpus::get()
async fn main() {
    init_logger();
    let zconfig = ZConfig::parse("./config.toml");
    let mut zilean_server = ZileanServer::new();
    let context = Context::new();
    let responder = context.socket(zmq::REP).unwrap();
    responder
        .bind(&zconfig.start_url)
        .expect("Failed to bind socket");
    let ascii_art = r#"
     ___                       ___       ___           ___           ___     
    /\  \          ___        /\__\     /\  \         /\  \         /\__\    
    \:\  \        /\  \      /:/  /    /::\  \       /::\  \       /::|  |   
     \:\  \       \:\  \    /:/  /    /:/\:\  \     /:/\:\  \     /:|:|  |   
      \:\  \      /::\__\  /:/  /    /::\~\:\  \   /::\~\:\  \   /:/|:|  |__ 
 _______\:\__\  __/:/\/__/ /:/__/    /:/\:\ \:\__\ /:/\:\ \:\__\ /:/ |:| /\__\
 \::::::::/__/ /\/:/  /    \:\  \    \:\~\:\ \/__/ \/__\:\/:/  / \/__|:|/:/  /
  \:\~~\~~     \::/__/      \:\  \    \:\ \:\__\        \::/  /      |:/:/  / 
   \:\  \       \:\__\       \:\  \    \:\ \/__/        /:/  /       |::/  /  
    \:\__\       \/__/        \:\__\    \:\__\         /:/  /        /:/  /   
     \/__/                     \/__/     \/__/         \/__/         \/__/    
    "#;

    info!("{}", ascii_art);
    info!("zilean launched🚀 🚀 🚀");
    loop {
        let message = match responder.recv_string(0) {
            Ok(msg) => msg.unwrap(),
            Err(e) => {
                let response = sonic_rs::to_string(&BacktestResponse::bad_request(
                    "back test launch failed.".to_string() + e.to_string().as_str(),
                ))
                .unwrap();
                let _ = responder.send(response.as_str(), 0);
                continue;
            }
        };

        if let Some(stripped) = message.strip_prefix("LAUNCH_BACKTEST") {
            let bt_config: BtConfig = match sonic_rs::from_str(stripped) {
                Ok(config) => config,
                Err(e) => {
                    let response = sonic_rs::to_string(&BacktestResponse::bad_request(
                        "Backtest launched failed.".to_string() + e.to_string().as_str(),
                    ))
                    .unwrap();
                    let _ = responder.send(response.as_str(), 0);
                    continue;
                }
            };
            let backtest_id = zilean_server
                .launch_backtest(bt_config, zconfig.tick_url.clone())
                .await;
            let response =
                sonic_rs::to_string(&BacktestResponse::normal_response(backtest_id)).unwrap();
            let _ = responder.send(response.as_str(), 0);
        }
    }
}
