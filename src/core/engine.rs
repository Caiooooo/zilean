use std::{collections::VecDeque, default, vec};

use crate::market::*;

use crate::dataloader::DataSource;
use rand::Rng;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use super::{dataloader::DataLoader, server::BacktestResponse};

#[derive(Debug, Default)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct BtConfig {
    exchanges: Vec<Exchange>,
    symbol: String,
    // v1, do not use trade data to match
    // trade_ex: Exchange,
    start_time: i64,
    end_time: i64,
    source: Option<DataSource>,
    balance: Balance,
    fee_rate: FeeRate,
}

impl BtConfig {
    pub fn parse(config: &str) -> BtConfig {
        serde_json::from_str(config).expect("Failed to parse config")
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

    pub fn execute_orders(&mut self, orderbook: &FxHashMap<Exchange, Depth>) -> Vec<FilledStack> {
        let mut filled = vec![];
        for order in self.inner.iter_mut() {
            let depth = orderbook.get(&order.exchange).unwrap();

            if depth.timestamp > order.timestamp {
                continue;
            }

            let (filled_price, filled_amount) = order.execute(depth);
            filled.push(FilledStack {
                exchange: order.exchange,
                symbol: order.symbol.clone(),
                filled_price,
                filled_amount,
            });
        }
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

// v1, do not support hedge backtest
pub struct ZileanV1 {
    config: BtConfig,
    orderbook: FxHashMap<Exchange, Depth>,
    // trade: Trade,
    order_list: OrderList,
    account: Account,
    data_loader: DataLoader,
    data_cache: Vec<Depth>,
    latency: LatencyModel,
    state: BacktestState,
}

impl ZileanV1 {
    pub fn new(config: BtConfig) -> ZileanV1 {
        Self {
            config,
            orderbook: FxHashMap::default(),
            // trade: Trade::default(),
            order_list: OrderList::default(),
            account: Account::default(),
            data_loader: DataLoader::default(),
            data_cache: Vec::new(),
            latency: LatencyModel::Fixed(20),
            state: BacktestState::default(),
        }
    }

    pub fn launch(&mut self, backtest_id: String) -> Result<(), std::io::Error> {
        self.prepare_data()?;

        self.state = BacktestState::Running;
        Ok(())
    }

    fn prepare_data(&mut self) -> Result<(), std::io::Error> {
        self.data_loader = match &self.config.source {
            Some(source) => DataLoader::new(source.clone(), 10000, 0),
            _ => DataLoader::new(DataSource::Database, 10000, 0),
        };
        // read the first block of data or the first file data into data_cache

        Ok(())
    }

    pub fn on_tick(&mut self) {}

    pub fn post_order(&mut self, order: Order) -> BacktestResponse {
        let post_value = order.price * order.amount;
        // check account balance, if not enough, return an error statu

        self.order_list
            .insert_order(self.latency.order_with_latency(order));
        // update account
        self.account.balance.add_freezed(post_value);
    }

    pub fn cancel_order(&mut self, cid: String) -> BacktestResponse {
        // remove order from order_list
        let order = self.order_list.remove_order(cid);

        if let Some(order) = order {
            let value = order.price * order.amount;
            self.account.balance.sub_freezed(value);

            // return an ok status response
        } else {
            // cancel order not found
            // return an error status response
        }
    }

    fn match_orders(&mut self) {
        // when tick update, try to match orders
        let filled_stack = self.order_list.execute_orders(&self.orderbook);
        for filled in filled_stack {
            self.account
                .position
                .update_pos(filled.filled_price, filled.filled_amount);
        }
    }

    fn start_listening(&mut self) {
        // start zmq server here, listen on ipc:///tmp/zilean_backtest/backtest_id/{}.ipc
    }
}

#[cfg(test)]
mod tests {
    use super::BtConfig;

    #[test]
    fn test_de_btconfig_from_str() {
        let config_str = r#"{
            "exchanges": ["BinanceSpot"],
            "symbol": "BTCUSDT",
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
}
