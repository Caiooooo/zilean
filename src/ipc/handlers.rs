use zmq::Socket;
use crate::{engine::ZileanV1, server::BacktestResponse};
use super::traits::{TradingEngine, MessageResponder, Command, CommandParser};
use crate::market::Order;
use serde::Serialize;
use log::{info, error};

/// ZMQ消息响应器实现
pub struct ZmqResponder {
    pub socket: Socket,  // 公开socket字段
}

impl ZmqResponder {
    pub fn new(socket: Socket) -> Self {
        Self { socket }
    }
}

impl MessageResponder for ZmqResponder {
    fn send_response<T: Serialize>(&self, response: &T) -> Result<(), String> {
        let serialized = sonic_rs::to_string(response)
            .map_err(|e| format!("Serialization error: {}", e))?;
        
        self.socket.send(serialized.as_str(), 0)
            .map_err(|e| format!("Failed to send response: {}", e))?;
        
        Ok(())
    }
    
    fn send_error(&self, error_message: &str) -> Result<(), String> {
        let error_response = BacktestResponse::bad_request(error_message.to_string());
        self.send_response(&error_response)
    }
}

/// 为ZileanV1实现TradingEngine trait
impl TradingEngine for ZileanV1 {
    async fn handle_tick(&mut self) -> BacktestResponse {
        self.on_tick().await
    }
    
    fn handle_post_order(&mut self, order: Order) -> BacktestResponse {
        self.post_order(order)
    }
    
    fn handle_cancel_order(&mut self, cid: String) -> BacktestResponse {
        self.cancel_order(cid)
    }
    
    fn handle_close_position(&mut self, symbol: String) -> BacktestResponse {
        // 注意：原始代码中CLOSE_POSITION调用的是cancel_order，这里保持一致
        // 如果需要不同的逻辑，可以在这里实现
        self.cancel_order(symbol)
    }
}

/// 统一的命令处理器
pub struct ZileanCommandHandler {
    next_tick: String,
}

impl ZileanCommandHandler {
    pub fn new() -> Self {
        Self {
            next_tick: String::new(),
        }
    }
    
    /// 处理命令，需要传入engine的可变引用
    pub fn handle_command_with_engine(&mut self, engine: &mut ZileanV1, message: &str) -> Result<crate::server::BacktestResponse, String> {
        let command = CommandParser::parse(message)?;
        
        match command {
            Command::Tick => {
                // 对于TICK命令，返回缓存的tick数据字符串
                if self.next_tick.is_empty() {
                    Err("No tick data available".to_string())
                }
                else {
                    // 直接返回成功响应，包含缓存的tick数据
                    Ok(crate::server::BacktestResponse::normal_response(self.next_tick.clone()))
                }
            },
            Command::PostOrder(order) => {
                Ok(engine.handle_post_order(order))
            },
            Command::CancelOrder(cid) => {
                Ok(engine.handle_cancel_order(cid))
            },
            Command::ClosePosition(symbol) => {
                Ok(engine.handle_close_position(symbol))
            },
            Command::Close => {
                info!("Server close command received.");
                Ok(crate::server::BacktestResponse::normal_response("Server closed.".to_string()))
            },
            Command::Unknown(msg) => {
                error!("Unknown command received: {}", msg);
                Ok(crate::server::BacktestResponse::bad_request("Unknown command.".to_string()))
            }
        }
    }
    
    /// 更新tick数据缓存
    pub async fn update_tick_cache(&mut self, engine: &mut ZileanV1) -> Result<(), String> {
        self.next_tick = engine.handle_tick().await.message;
        // self.next_tick = sonic_rs::to_string(&tick_response)
            // .map_err(|e| format!("Failed to serialize tick data: {}", e))?;
        Ok(())
    }
}
