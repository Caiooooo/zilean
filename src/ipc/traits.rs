use serde::Serialize;
use crate::{market::Order, server::BacktestResponse};

/// 定义交易引擎的核心操作接口
/// 这个trait抽象了交易引擎的主要功能，隐藏了内部实现细节
pub trait TradingEngine {
    /// 处理tick数据更新
    /// 返回当前市场状态的响应
    async fn handle_tick(&mut self) -> BacktestResponse;
    
    /// 处理下单请求
    /// 参数: order - 订单对象
    /// 返回: 下单结果响应
    fn handle_post_order(&mut self, order: Order) -> BacktestResponse;
    
    /// 处理撤单请求
    /// 参数: cid - 订单客户端ID
    /// 返回: 撤单结果响应
    fn handle_cancel_order(&mut self, cid: String) -> BacktestResponse;
    
    /// 处理平仓请求
    /// 参数: symbol - 交易对符号
    /// 返回: 平仓结果响应
    fn handle_close_position(&mut self, symbol: String) -> BacktestResponse;
    
}

/// 引擎状态信息
#[derive(Debug, Serialize, Clone)]
pub struct EngineStatus {
    pub backtest_id: String,
    pub is_running: bool,
    pub current_timestamp: i64,
}

/// 消息响应器trait
/// 负责将响应序列化并发送
pub trait MessageResponder {
    /// 发送响应消息
    /// 参数: response - 响应数据
    /// 返回: 发送是否成功
    fn send_response<T: Serialize>(&self, response: &T) -> Result<(), String>;
    
    /// 发送错误响应
    /// 参数: error_message - 错误信息
    /// 返回: 发送是否成功
    fn send_error(&self, error_message: &str) -> Result<(), String>;
}

/// 命令类型枚举
#[derive(Debug, Clone)]
pub enum Command {
    Tick,
    PostOrder(Order),
    CancelOrder(String),
    ClosePosition(String),
    Close,
    Unknown(String),
}

/// 命令解析器
pub struct CommandParser;

impl CommandParser {
    /// 解析命令字符串为命令枚举
    pub fn parse(message: &str) -> Result<Command, String> {
        if message.starts_with("TICK") {
            Ok(Command::Tick)
        } else if let Some(stripped) = message.strip_prefix("POST_ORDER") {
            match sonic_rs::from_str::<Order>(stripped) {
                Ok(order) => Ok(Command::PostOrder(order)),
                Err(e) => Err(format!("Error parsing order: {}", e)),
            }
        } else if let Some(stripped) = message.strip_prefix("CANCEL_ORDER") {
            Ok(Command::CancelOrder(stripped.to_string()))
        } else if let Some(stripped) = message.strip_prefix("CLOSE_POSITION") {
            Ok(Command::ClosePosition(stripped.to_string()))
        } else if message.starts_with("CLOSE") {
            Ok(Command::Close)
        } else {
            Ok(Command::Unknown(message.to_string()))
        }
    }
}
