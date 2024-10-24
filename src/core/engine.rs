use crate::dataloader::DataSource;
use crate::market::*;
use rand::Rng;
// use rustc_hash::FxHashMap;
use log::info;
use sonic_rs::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;
use zmq::Context;

use super::{dataloader::DataLoader, server::BacktestResponse};

#[derive(Debug, Default, PartialEq)]
pub enum BacktestState {
    #[default]
    Ready,
    Running,
    Paused,
    Finished,
}

pub enum LatencyModel {
    None,
    Fixed(i64),
    Random(i64, i64),
}

impl LatencyModel {
    pub fn order_with_latency(&self, order: Order) -> Order {
        match self {
            LatencyModel::None => order,
            LatencyModel::Fixed(latency) => Order {
                timestamp: order.timestamp + latency,
                ..order
            },
            LatencyModel::Random(a, b) => Order {
                timestamp: order.timestamp + rand::thread_rng().gen_range(*a..*b),
                ..order
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct BtConfig {
    pub exchanges: Vec<Exchange>,
    pub symbol: String,
    // v1, do not use trade data to match
    // trade_ex: Exchange,
    pub start_time: i64,
    pub end_time: i64,
    pub source: Option<DataSource>,
    pub balance: Balance,
    pub fee_rate: FeeRate,
}

impl BtConfig {
    pub fn parse(config: &str) -> BtConfig {
        sonic_rs::from_str(config).expect("Failed to parse config")
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct OrderList {
    inner: VecDeque<Order>,
}

impl OrderList {
    pub fn insert_order(&mut self, order: Order) {
        self.inner.push_back(order);
    }

    pub fn remove_order(&mut self, cid: String) -> Option<Order> {
        if let Some(pos) = self.inner.iter().position(|x| x.cid == cid) {
            return self.inner.remove(pos);
        }
        None
    }

    pub fn execute_orders(&mut self, depth: &Depth) -> Vec<FilledStack> {
        let mut filled = vec![];
        self.inner.retain_mut(|order| {
            if depth.timestamp < order.timestamp || order.exchange != depth.exchange {
                return true; // keep this order
            }
            let (filled_price, filled_amount) = order.execute(depth);
            if filled_price != 0.0 {
                filled.push(FilledStack {
                    exchange: order.exchange,
                    symbol: order.symbol.clone(),
                    filled_price,
                    filled_amount,
                });
            }

            order.state != OrderState::Filled // keep this order if not filled
        });

        filled
    }
}

#[derive(Debug)]
pub struct FilledStack {
    pub exchange: Exchange,
    pub symbol: String,
    pub filled_price: f64,
    pub filled_amount: f64,
}

#[derive(Serialize, Default)]
pub struct TickResponse {
    pub depth: Depth,
    pub account: Account,
}

// v1, do not support hedge backtest
pub struct ZileanV1 {
    config: BtConfig,
    // trade: Trade,
    order_list: OrderList,
    account: Account,
    data_loader: Arc<Mutex<DataLoader>>,
    data_cache: VecDeque<Depth>,
    latency: LatencyModel,
    state: BacktestState,
}

impl ZileanV1 {
    pub fn new(config: BtConfig) -> ZileanV1 {
        Self {
            config: config.clone(),
            // trade: Trade::default(),
            order_list: OrderList::default(),
            account: Account::default(),
            data_loader: Arc::new(Mutex::new(DataLoader::new(1_000_000, &config))),
            data_cache: VecDeque::new(),
            // TODO multiple latency model
            latency: LatencyModel::Fixed(20),
            state: BacktestState::default(),
        }
    }

    pub async fn launch(
        &mut self,
        backtest_id: String,
        tick_url: &str,
    ) -> Result<(), std::io::Error> {
        self.prepare_data().await?;
        self.account.backtest_id = backtest_id;
        self.account.balance = self.config.balance.clone();
        self.state = BacktestState::Running;
        self.start_listening(tick_url).await;
        Ok(())
    }

    async fn prepare_data(&mut self) -> Result<(), std::io::Error> {
        let data_loader = self.data_loader.clone();

        let handle = tokio::spawn(async move { data_loader.lock().await.load_data().await });

        // read the first block of data or the first file data into data_cache
        self.data_cache = VecDeque::from(handle.await??);
        Ok(())
    }

    pub async fn on_tick(&mut self) -> BacktestResponse {
        if self.state != BacktestState::Running {
            return BacktestResponse::bad_request("Backtest is not running".to_string());
        }

        if self.data_cache.is_empty() {
            let data_loader = self.data_loader.clone();
            // fill the cache with new data
            let handle = tokio::spawn(async move { data_loader.lock().await.load_data().await });
            self.data_cache = VecDeque::from(handle.await.unwrap().unwrap());
            // if no more data, return an end status
            if self.data_cache.is_empty() {
                self.state = BacktestState::Finished;
                return BacktestResponse::bad_request("No more data, backtestfinished".to_string());
            }
        }
        // println!("data_cache len: {}", self.data_cache.len());
        let depth = self.data_cache.pop_front().unwrap();
        self.match_orders(&depth);
        let tick_response = TickResponse {
            depth,
            account: self.account.clone(),
        };
        BacktestResponse::normal_response(sonic_rs::to_string(&tick_response).unwrap())
    }

    // return cid when success
    pub fn post_order(&mut self, order: Order) -> BacktestResponse {
        info!("{:?}", order);
        let amount = order.amount;
        let post_value = order.price * amount;
        // check account balance, if not enough, return an error status
        if self.account.balance.get_available() < post_value && order.side == OrderSide::Buy {
            return BacktestResponse::bad_request("Insufficient balance.".to_string());
        }
        // bugs should add freezed amount
        if self.account.position.amount_available < order.amount && order.side == OrderSide::Sell {
            return BacktestResponse::bad_request("Insufficient amount.".to_string());
        }
        let cid = order.cid.clone();
        self.order_list
            .insert_order(self.latency.order_with_latency(order));
        // update account
        self.account.position.add_freezed(amount);
        self.account.balance.add_freezed(post_value);

        BacktestResponse::normal_response(format!("cid: {} order posted.", cid))
    }

    pub fn cancel_order(&mut self, cid: String) -> BacktestResponse {
        // remove order from order_list
        let order = self.order_list.remove_order(cid);

        if let Some(order) = order {
            let amount = order.amount - order.filled_amount;
            let value = order.price * amount;
            self.account.balance.sub_freezed(value);
            self.account.position.sub_freezed(amount);
            // return an ok status response
            BacktestResponse::normal_response(format!("cid: {} order canceled.", order.cid))
        } else {
            // cancel order not found
            // return an error status response
            BacktestResponse::bad_request("Order not found".to_string())
        }
    }

    fn match_orders(&mut self, depth: &Depth) {
        // when tick update, try to match orders
        let filled_stack = self.order_list.execute_orders(depth);
        if !filled_stack.is_empty() {
            info!("{:?}", filled_stack);
        }
        for filled in filled_stack {
            self.account
                .position
                .update_pos(filled.filled_price, filled.filled_amount);
            self.account
                .balance
                .fill_freezed(filled.filled_price * filled.filled_amount);
        }
    }

    // controller for backtest server
    async fn start_listening(&mut self, tick_url: &str) {
        // start zmq server here, listen on ipc:///tmp/zilean_backtest/{backtest_id}.ipc
        let context = Context::new();
        let responder = context.socket(zmq::REP).unwrap();
        let url = format!("{}{}.ipc", tick_url, self.account.backtest_id);

        //timeout setting
        responder
            .set_heartbeat_ivl(100) // timeout check interval 1 s
            .expect("Failed to set heartbeat interval");
        responder
            .set_heartbeat_timeout(300) // timeout check wait time 3 s
            .expect("Failed to set heartbeat timeout");
        responder
            .set_heartbeat_ttl(6000) // timeout time 60 s
            .expect("Failed to set heartbeat TTL");
        responder
            .set_rcvtimeo(10000) // receive timeout 10 s
            .expect("Failed to set receive timeout");
        // TODO let dir = url[6..url.len() - 4].to_string() + ".ipc";
        // delete the repete file

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
                let tick = self.on_tick().await;
                let tick_response =
                    sonic_rs::to_string(&tick).expect("Failed to serialize tick data");
                responder
                    .send(tick_response.as_str(), 0)
                    .expect("Failed to send tick data.");
            } else if let Some(stripped) = message.strip_prefix("POST_ORDER") {
                let order: Order = match sonic_rs::from_str(stripped) {
                    Ok(order) => order,
                    Err(e) => {
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
            } else if let Some(stripped) = message.strip_prefix("CANCEL_ORDER") {
                let cid = stripped.to_string();
                let response = self.cancel_order(cid);
                let response = sonic_rs::to_string(&response).unwrap();
                let _ = responder.send(response.as_str(), 0);
            } else if message.starts_with("CLOSE") {
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

    async fn close_bt(&mut self, responder: zmq::Socket, tick_url: &str) {
        info!(
            "Closing backtest server for id: {}.",
            self.account.backtest_id
        );
        responder
            .disconnect(format!("{}{}.ipc", tick_url, self.account.backtest_id).as_str())
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::BtConfig;

    #[test]
    fn test_de_btconfig_from_str() {
        let config_str = r#"{
            "exchanges": ["BinanceSpot"],
            "symbol": "BTC_USDT",
            "start_time": 0,
            "end_time": 0,
            "balance": {
                "total": 0,
                "available": 0,
                "freezed": 0
            },
            "source": {"FilePath": "./data/BTCUSDT.csv"},
            "fee_rate": {
                "maker_fee": 0,
                "taker_fee": 0
            }
        }"#;
        println!("{:?}", config_str);
        let config = BtConfig::parse(config_str);
        println!("{:?}", config);
    }
    use super::ZileanV1;

    #[tokio::test]
    async fn test_on_tick() {
        let mut zilean = ZileanV1::new(BtConfig::default());
        println!("{:?}", BtConfig::default());
        zilean.state = super::BacktestState::Running;
        zilean.prepare_data().await.unwrap();
        zilean.on_tick().await;
    }
}
