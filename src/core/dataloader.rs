use crate::engine::*;
use crate::market::*;
use crate::ZConfig;
use clickhouse::Client;
use log::{debug, error};
use sonic_rs::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub enum DataSource {
    #[default]
    Database,
    FilePath(String),
}
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct DatabaseAccount {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}

#[derive(Default)]
pub struct DataLoader {
    config: BtConfig,
    database: Option<DatabaseAccount>,
    source: DataSource,
    limit: usize,
    limit_ratio_depth: f32,
    limit_ratio_trade: f32,
    last_timestamp_depth: usize,
    last_timestamp_trade: usize,
}

impl DataLoader {
    pub async fn new(limit: usize, config: &BtConfig, zconfig: ZConfig) -> DataLoader {
        let config_clone = config.clone();
        let source = match config.source.clone() {
            Some(source) => source,
            _ => DataSource::Database,
        };
        let database = match source {
            DataSource::Database => Some(zconfig.database),
            _ => None,
        };
        // init the start time
        let mut start_time = config.start_time;
        let exchange_list: Vec<String> = config
            .exchanges
            .iter()
            .map(|ex| match ex {
                Exchange::BinanceSpot => "binance".to_string(),
                Exchange::CoinbaseSpot => "coinbase".to_string(),
                Exchange::OkxSpot => "okx".to_string(),
                Exchange::KrakenSpot => "kraken".to_string(),
                Exchange::OkxSwap => "okx_futures".to_string(),
                Exchange::BinanceSwap => "binance_futures".to_string(),
                Exchange::BybitSwap => "bybit".to_string(),
                Exchange::BitgetSwap => "bitget_futures".to_string(),
            })
            .collect();
        if let Some(conf) = database.clone() {
            let client = Client::default()
                .with_url(format!("{}:{}", conf.host, conf.port))
                .with_user(conf.username)
                .with_password(conf.password);
            for exchange in &exchange_list{
                let query = format!(
                    "SELECT  ?fields FROM {}.orderbook_{} ORDER BY local_timestamp LIMIT 1",
                    exchange,
                    &config.symbol.replace("_", "").to_lowercase()
                );
                let ret = client.query(&query).fetch_all::<Depth>().await;
                match ret {
                    Ok(data) => {
                        if let Some(first) = data.first() {
                            start_time = first.local_timestamp.max(start_time);
                        }
                    }
                    Err(e) => {
                        error!("query failed invailed time: {:?}", e);
                    }
                }

                let query_trade = format!(
                    "SELECT  ?fields FROM {}.trade_{} ORDER BY local_timestamp LIMIT 1",
                    exchange,
                    &config.symbol.replace("_", "").to_lowercase()
                );
                let ret = client.query
                (&query_trade).fetch_all::<Trade>().await;
                match ret {
                    Ok(data) => {
                        if let Some(first) = data.first() {
                            start_time = first.timestamp.max(start_time);
                        }
                    }
                    Err(e) => {
                        error!("query failed invailed time: {:?}", e);
                    }
                }
            }
        }
        
        // limit_ratio will adjust dymatically according to the data size, 10 for fast start
        DataLoader {
            config: config_clone,
            database,
            source,
            limit,
            limit_ratio_depth: 100.0,
            limit_ratio_trade: 100.0,
            last_timestamp_trade: start_time as usize,
            last_timestamp_depth: start_time as usize,
        }
    }
    pub async fn load_data(&mut self) -> Result<Vec<Depth>, std::io::Error> {
        match &self.source {
            DataSource::Database => self.load_from_database().await,
            DataSource::FilePath(path) => self.load_from_file(path).await,
        }
    }

    pub async fn load_trade(&mut self) -> Result<Vec<Trade>, std::io::Error> {
        match &self.source {
            DataSource::Database => self.load_trade_from_database().await,
            DataSource::FilePath(_path) => Ok(Vec::new()),
        }
    }

    async fn load_trade_from_database(&mut self) -> Result<Vec<Trade>, std::io::Error> {
        let dbconf = self.database.clone().ok_or_else(|| std::io::Error::new(
            std::io::ErrorKind::Other,
            "Database configuration is missing",
        ))?;
        let client = Client::default()
            .with_url(format!("{}:{}", dbconf.host, dbconf.port))
            .with_user(dbconf.username)
            .with_password(dbconf.password);

        let exchange_list: Vec<String> = self
            .config
            .exchanges
            .iter()
            .map(|ex| match ex {
                Exchange::BinanceSpot => "binance".to_string(),
                Exchange::CoinbaseSpot => "coinbase".to_string(),
                Exchange::OkxSpot => "okx".to_string(),
                Exchange::KrakenSpot => "kraken".to_string(),
                Exchange::BinanceSwap => "binance_futures".to_string(),
                Exchange::BybitSwap => "bybit".to_string(),
                Exchange::BitgetSwap => "bitget_futures".to_string(),
                Exchange::OkxSwap => "okx_futures".to_string(),
            })
            .collect();

        if self.config.end_time < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "invalid end time, end time should be grater than 0.",
            ));
        }
        let end_time = (self.last_timestamp_trade + self.limit * self.limit_ratio_trade as usize)
            .min(self.config.end_time as usize);

        let mut query = String::from("WITH filtered_data AS (");
        for (i, exchange) in exchange_list.iter().enumerate() {
            if i > 0 {
                query.push_str(" UNION ALL ");
            }
            query.push_str(&format!(
                "SELECT * FROM {}.trade_{} WHERE local_timestamp >= {} AND local_timestamp < {}",
                exchange,
                self.config.symbol.replace("_", "").to_lowercase(),
                self.last_timestamp_trade,
                end_time,
            ));
        }
        query.push_str(") SELECT ?fields FROM filtered_data ORDER BY local_timestamp");
        let statement = client.query(&query);
        let ret = statement.fetch_all::<Trade>().await;
        match ret {
            Ok(mut data) => {
                debug!("query success");
                if !data.is_empty() {
                    if data.len() < self.limit {
                        self.limit_ratio_trade *= (self.limit as f32 / data.len() as f32).min(20.0);
                        // slow start
                    }
                    self.last_timestamp_trade = end_time + 1;
                }
                if exchange_list.contains(&"binance_futures_dec".to_string()) {
                    for trade in data.iter_mut() {
                        if trade.exchange.is_empty(){
                            trade.exchange = "binance_futures".to_string();
                        }
                    }
                }// bugs: binance_futures_dec is empty
                Ok(data)
            }
            Err(e) => {
                error!("query failed: {:?}", e);
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "query failed",
                ))
            }
        }
    }

    async fn load_from_database(&mut self) -> Result<Vec<Depth>, std::io::Error> {
        let dbconf = self.database.clone().ok_or_else(|| std::io::Error::new(
            std::io::ErrorKind::Other,
            "Database configuration is missing",
        ))?;
        let client = Client::default()
            .with_url(format!("{}:{}", dbconf.host, dbconf.port))
            .with_user(dbconf.username)
            .with_password(dbconf.password);

        let exchange_list: Vec<String> = self
            .config
            .exchanges
            .iter()
            .map(|ex| match ex {
                Exchange::BinanceSpot => "binance".to_string(),
                Exchange::CoinbaseSpot => "coinbase".to_string(),
                Exchange::OkxSpot => "okx".to_string(),
                Exchange::KrakenSpot => "kraken".to_string(),
                Exchange::BinanceSwap => "binance_futures".to_string(),
                Exchange::BybitSwap => "bybit".to_string(),
                Exchange::BitgetSwap => "bitget_futures".to_string(),
                Exchange::OkxSwap => "okx_futures".to_string(),
            })
            .collect();

        if self.config.end_time < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "invalid end time, end time should be grater than 0.",
            ));
        }
        let end_time = (self.last_timestamp_depth + self.limit * self.limit_ratio_depth as usize)
            .min(self.config.end_time as usize);

        let mut query = String::from("WITH filtered_data AS (");
        for (i, exchange) in exchange_list.iter().enumerate() {
            if i > 0 {
                query.push_str(" UNION ALL ");
            }
            query.push_str(&format!(
                "SELECT * FROM {}.orderbook_{} WHERE local_timestamp >= {} AND local_timestamp < {}",
                exchange,
                self.config.symbol.replace("_", "").to_lowercase(),
                self.last_timestamp_depth,
                end_time,
            ));
        }
        query.push_str(") SELECT ?fields FROM filtered_data ORDER BY local_timestamp");
        // 构建查询绑定参数

        // 执行查询
        let statement = client.query(&query);
        let ret = statement.fetch_all::<Depth>().await;
        match ret {
            Ok(data) => {
                debug!("query success");
                if !data.is_empty() {
                    if data.len() < self.limit {
                        self.limit_ratio_depth *= (self.limit as f32 / data.len() as f32).min(20.0);
                        // slow start
                    }
                    self.last_timestamp_depth = end_time  + 1;
                }
                Ok(data)
            }
            Err(e) => {
                error!("query {} failed: {:?}", query,  e);
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e,
                ))
            }
        }
    }
    async fn load_from_file(&self, _path: &str) -> Result<Vec<Depth>, std::io::Error> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use crate::ZConfig;

    use super::BtConfig;
    use super::DataLoader;

    #[tokio::test]
    async fn test_data_loader() {
        let config = BtConfig::default();
        let zconfig = ZConfig::parse("misc/config.tmol");
        DataLoader::new(10_000, &config, zconfig)
            .await
            .load_data()
            .await
            .unwrap();
    }
}
