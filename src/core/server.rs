use core::fmt;
use serde::Serialize;
// use rustc_hash::FxHashMap;
// use std::sync::{Arc, Mutex};

use crate::ZConfig;

use super::engine::{BtConfig, ZileanV1};

#[derive(Default)]
pub struct ZileanServer {
    // backtests: Arc<Mutex<FxHashMap<String, ZileanV1>>>,
}

// #[tokio::main]
async fn start_zilean(bid_clone:String, config: BtConfig, zconfig: ZConfig){
    let url = zconfig.tick_url.clone();
    let mut zilean = ZileanV1::new(config, zconfig).await;
    zilean.launch(bid_clone.clone(), &url).await.unwrap();
}

impl ZileanServer {
    pub fn new() -> Self {
        Self {
            // backtests: Arc::new(Mutex::new(FxHashMap::default())),
        }
    }

    pub async fn launch_backtest(&mut self, config: BtConfig, zconfig: ZConfig) -> String {
        let backtest_id = format!("bt-{}", nanoid::nanoid!());
        let bid_clone = backtest_id.clone();
        // let bt_clone = Arc::clone(&self.backtests);
        tokio::spawn(async move {
            start_zilean(bid_clone, config, zconfig).await;
        });

        backtest_id
    }
}

#[derive(Debug, Serialize)]
pub struct BacktestResponse {
    pub status: String,
    pub message: String,
}

impl BacktestResponse {
    pub fn bad_request(message: String) -> Self {
        Self {
            status: "error".to_string(),
            message,
        }
    }

    pub fn normal_response(message: String) -> Self {
        Self {
            status: "ok".to_string(),
            message,
        }
    }
}

impl fmt::Display for BacktestResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", sonic_rs::to_string(self).unwrap())
    }
}
