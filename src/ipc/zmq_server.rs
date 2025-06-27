use zmq::Context;
use log::info;

use crate::engine::ZileanV1;
use super::traits::MessageResponder;
use super::handlers::{ZmqResponder, ZileanCommandHandler};

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
        
        // 创建响应器和命令处理器
        let message_responder = ZmqResponder::new(responder);
        let mut command_handler = ZileanCommandHandler::new();
        let _ = command_handler.update_tick_cache(self).await;
        loop {
            let message = match message_responder.socket.recv_string(0) {
                Ok(msg) => msg.unwrap(),
                Err(e) => {
                    // connection interrupted
                    if e.to_string().contains("Resource temporarily unavailable") {
                        info!("Connection {} recv timeout.", self.account.backtest_id);
                        break;
                    }
                    let _ = message_responder.send_error(&format!("Error parsing string: {}", e));
                    continue;
                }
            };
            
            // 使用命令处理器处理其他命令
            match command_handler.handle_command_with_engine(self, &message) {
                Ok(response) => {
                    // 特殊处理CLOSE/TICK命令
                    if message.starts_with("CLOSE") {
                        let _ = message_responder.send_response(&response);
                        break;
                    }
                    
                    if let Err(e) = message_responder.send_response(&response) {
                        log::error!("Failed to send response: {}", e);
                    }

                    if message.starts_with("TICK") {
                        // 更新缓存的tick数据
                        if let Err(e) = command_handler.update_tick_cache(self).await {
                            log::error!("Failed to update tick cache: {}", e);
                        }
                    }
                },
                Err(e) => {
                    log::error!("Command handling error: {}", e);
                    let _ = message_responder.send_error(&e);
                }
            }
        }
        
        self.close_bt(message_responder.socket, tick_url).await;
    }
}