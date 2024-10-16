use crate::market::*;
use crate::account::Account;

use crate::dataloader::DataSource;
use rustc_hash::FxHashMap;

use super::dataloader::DataLoader;

#[derive(Debug, Eq, PartialEq, Hash)]
pub enum Exchange {
    BinanceSpot,
    CoinbaseSpot,
    OkxSpot,
    KrakenSpot,
}

impl Exchange {
    pub fn from_str(s: &str) -> Option<Exchange> {
        match s {
            "BinanceSpot" => Some(Exchange::BinanceSpot),
            "CoinbaseSpot" => Some(Exchange::CoinbaseSpot),
            "OkxSpot" => Some(Exchange::OkxSpot),
            "KrakenSpot" => Some(Exchange::KrakenSpot),
            _ => None, 
        }
    }
}

#[derive(Debug)]
pub struct BtConfig {
    exchanges: Vec<Exchange>,
    symbol: String,
    // v1, do not use trade data to match
    // trade_ex: Exchange,
    start_time: i64,
    end_time: i64,
    source: DataSource,
}

impl BtConfig {
    pub fn new(exchanges: Vec<String>, symbol: String, start_time: i64, end_time: i64, source: Option<String>) -> BtConfig {
        let exchange_vec: Vec<Exchange> = exchanges
            .iter()
            .filter_map(|s| Exchange::from_str(s))
            .collect();

        let data_source = match source {
            Some(path) => DataSource::FilePath(path),
            None => DataSource::Database,
        };

        Self {
            exchanges: exchange_vec,
            symbol,
            source: data_source,
            start_time,
            end_time,
        }
    }
}

// v1, do not support hedge backtest
pub struct ZileanV1 {
    config: BtConfig,
    orderbook: FxHashMap<Exchange, Depth>,
    trade: Trade,
    order_list: Vec<Order>, 
    account: Account,
    data_loader: DataLoader,
    data_cache: Vec<Depth>,
    is_ready: bool,
}

impl ZileanV1 {
    pub fn new(config: BtConfig) -> ZileanV1 {
        Self {
            config,
            orderbook: FxHashMap::default(),
            trade: Trade::default(),
            order_list: Vec::new(),
            account: Account::default(),
            data_loader: DataLoader::default(),
            data_cache: Vec::new(),
            is_ready: false,
        }
    }

    pub fn prepare(&mut self) {

    }

    pub fn tick() {

    }
}