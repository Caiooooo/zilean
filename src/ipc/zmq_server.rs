use zmq::Context;
use log::info;

use crate::{engine::ZileanV1, market::Order, server::BacktestResponse};

impl ZileanV1{
    // controller for backtest server
    pub async fn start_listening(&mut self, tick_url: &str) {
        // start zmq server here, listen on ipc:///tmp/zilean_backtest/{backtest_id}.ipc
        let context = Context::new();
        let responder = context.socket(zmq::REP).unwrap();
        let url = format!("{}{}.ipc", tick_url, self.account.backtest_id);

        //timeout setting
        responder
            .set_heartbeat_ivl(10000) // timeout check interval 100 s
            .expect("Failed to set heartbeat interval");
        responder
            .set_heartbeat_timeout(30000) // timeout check wait time 300 s
            .expect("Failed to set heartbeat timeout");
        responder
            .set_heartbeat_ttl(600000) // timeout time 6000 s
            .expect("Failed to set heartbeat TTL");
        responder
            .set_rcvtimeo(100000) // receive timeout 100 s
            .expect("Failed to set receive timeout");
        // TODO let dir = url[6..url.len() - 4].to_string() + ".ipc";
        // delete the repete file

        // 检查并创建目录
        if let Some(dir_path) = url.strip_prefix("ipc://") {
            if let Some(parent_dir) = std::path::Path::new(dir_path).parent() {
                if !parent_dir.exists() {
                    std::fs::create_dir_all(parent_dir).expect("Failed to create directory");
                }
            }
        }
        responder.bind(&url).expect("Failed to bind socket");
        info!("bt-server {} connected.", self.account.backtest_id);
        loop {
            let message = match responder.recv_string(0) {
                Ok(msg) => msg.unwrap(),
                Err(e) => {
                    // connection interrupted
                    if e.to_string().contains("Resource temporarily unavailable") {
                        info!("Connection {} recv timeout.", self.account.backtest_id);
                        break;
                    }
                    let response = sonic_rs::to_string(&BacktestResponse::bad_request(format!(
                        "Error prasing string, {}.",
                        e
                    )))
                    .unwrap();
                    let _ = responder.send(response.as_str(), 0);
                    continue;
                }
            };
            if message.starts_with("TICK") {
                // execute on_tick and send response
                responder
                    .send(self.next_tick.as_str(), 0)
                    .expect("Failed to send tick data.");
                let tick = self.on_tick().await;
                let tick_response =
                    sonic_rs::to_string(&tick).expect("Failed to serialize tick data");
                self.next_tick = tick_response;
            } else if let Some(stripped) = message.strip_prefix("POST_ORDER") {
                let order: Order = match sonic_rs::from_str(stripped) {
                    Ok(order) => order,
                    Err(e) => {
                        log::error!("Error parsing order: {}", e);
                        let response = sonic_rs::to_string(&BacktestResponse::bad_request(
                            format!("Error parsing order: {}", e),
                        ))
                        .unwrap();
                        let _ = responder.send(response.as_str(), 0);
                        continue;
                    }
                };
                let response = self.post_order(order);
                let response = sonic_rs::to_string(&response).unwrap();
                let _ = responder.send(response.as_str(), 0);
            } else if let Some(stripped) = message.strip_prefix("CLOSE_POSITION") {
                let symbol = stripped.to_string();
                let response = self.cancel_order(symbol);
                let response = sonic_rs::to_string(&response).unwrap();
                let _ = responder.send(response.as_str(), 0);
            } else if let Some(stripped) = message.strip_prefix("CANCEL_ORDER") {
                let cid = stripped.to_string();
                let response = self.cancel_order(cid);
                let response = sonic_rs::to_string(&response).unwrap();
                let _ = responder.send(response.as_str(), 0);
            } else if message.starts_with("CLOSE") {
                info!("Server closed.");
                let response = sonic_rs::to_string(&BacktestResponse::normal_response(
                    "Server closed.".to_string(),
                ))
                .unwrap();
                responder
                    .send(response.as_str(), 0)
                    .expect("Failed to send unknown command response.");
                break;
            } else {
                let response = sonic_rs::to_string(&BacktestResponse::bad_request(
                    "Unknown command.".to_string(),
                ))
                .unwrap();
                responder
                    .send(response.as_str(), 0)
                    .expect("Failed to send unknown command response.");
            }
        }
        self.close_bt(responder, tick_url).await;
    }

}