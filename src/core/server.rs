use rustc_hash::FxHashMap;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::thread;

use super::engine::{BtConfig, ZileanV1};
pub struct ZileanServer {
    backtests: Arc<Mutex<FxHashMap<String, ZileanV1>>>,
}

impl ZileanServer {
    pub fn new() -> Self {
        Self {
            backtests: Arc::new(Mutex::new(FxHashMap::default())),
        }
    }

    pub fn launch_backtest(&mut self, config: BtConfig) -> String {
        let backtest_id = format!("bt-{}", uuid::Uuid::new_v4());

        let bid_clone = backtest_id.clone();
        let bt_clone = Arc::clone(&self.backtests);
        thread::spawn(move || {
            let zilean = ZileanV1::new(config);

            // zilean.launch();

            let mut bt = bt_clone.lock().unwrap();
            bt.insert(bid_clone.clone(), zilean);
        });

        backtest_id
    }
}

#[derive(Debug, Serialize)]
pub struct BacktestResponse {
    pub backtest_id: String,
    pub status: String,
    pub message: String,
}
