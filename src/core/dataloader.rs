use crate::config;
use crate::engine::*;
use crate::market::*;
use clickhouse::{sql::Identifier, Client};
use log::{debug, error};
use sonic_rs::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub enum DataSource {
    #[default]
    Database,
    FilePath(String),
}
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DatabaseAccount {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
}

#[derive(Default)]
pub struct DataLoader {
    config: BtConfig,
    database: Option<DatabaseAccount>,
    source: DataSource,
    limit: usize,
    limit_ratio: f32,
    last_timestamp: usize,
}

impl DataLoader {
    pub async fn new(limit: usize, config: &BtConfig) -> DataLoader {
        let config_clone = config.clone();
        let source = match config.source.clone() {
            Some(source) => source,
            _ => DataSource::Database,
        };
        let database = match source {
            DataSource::Database => Some(config::ZConfig::parse("config.toml").database),
            _ => None,
        };
        // init the start time
        let mut start_time = config.start_time;
        if let Some(conf) = database.clone() {
            let client = Client::default()
                .with_url(format!("{}:{}", conf.host, conf.port))
                .with_user(conf.username)
                .with_password(conf.password)
                .with_database(conf.database);
            let ret = client
                .query("SELECT ?fields FROM ? LIMIT 1")
                .bind(Identifier(
                    format!("{}_spot", &config.symbol.split("_").next().unwrap())
                        .to_lowercase()
                        .as_str(),
                ))
                .fetch_all::<Depth>().await;
            // println!("start time: {:?}", ret);
            match ret {
                Ok(data) => {
                    if !data.is_empty() {
                        start_time = data.first().unwrap().local_timestamp.max(start_time);
                    }
                }
                Err(e) => {
                    error!("query failed invailed time: {:?}", e);
                }
            }
        }
        // limit_ratio will adjust dymatically according to the data size, 10 for fast start
        DataLoader {
            config: config_clone,
            database,
            source,
            limit,
            limit_ratio: 1000.0,
            last_timestamp: start_time as usize,
        }
    }
    pub async fn load_data(&mut self) -> Result<Vec<Depth>, std::io::Error> {
        match &self.source {
            DataSource::Database => self.load_from_database().await,
            DataSource::FilePath(path) => self.load_from_file(path).await,
        }
    }

    async fn load_from_database(&mut self) -> Result<Vec<Depth>, std::io::Error> {
        let dbconf = self.database.clone().unwrap();
        let client = Client::default()
            .with_url(format!("{}:{}", dbconf.host, dbconf.port))
            .with_user(dbconf.username)
            .with_password(dbconf.password)
            .with_database(dbconf.database);

        let mut exchange_list: Vec<String> = self.config.exchanges.iter().map(|ex| match ex {
            Exchange::BinanceSpot => "binance".to_string(),
            Exchange::CoinbaseSpot => "coinbase".to_string(),
            Exchange::OkxSpot => "okx".to_string(),
            Exchange::KrakenSpot => "kraken".to_string(),
        }).collect();
        if exchange_list.contains(&"okx".to_string()){
            exchange_list.push("okex".to_string());
        }

        if self.config.end_time < 0{
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "invalid end time, end time should be grater than 0.",
            ));
        }
        let end_time = (self.last_timestamp + self.limit * self.limit_ratio as usize).min(self.config.end_time as usize);
        let ret = 
            // TODO 支持期货查询
            client
                .query(
                    "SELECT ?fields FROM ? WHERE exchange IN (?) AND local_timestamp >= ? AND local_timestamp < ? ORDER BY local_timestamp LIMIT ?",
                )
                .bind(Identifier(
                    format!("{}_spot", &self.config.symbol.split("_").next().unwrap())
                        .to_lowercase()
                        .as_str(),
                ))
                .bind(exchange_list)
                .bind(self.last_timestamp)
                .bind(end_time)
                .bind(self.limit)
                .fetch_all::<Depth>()
                .await;
        match ret {
            Ok(data) => {
                debug!("query success");
                if !data.is_empty() {
                    if data.len() < self.limit{
                        self.limit_ratio *= (self.limit as f32/ data.len() as f32).min(20.0) ; // slow start
                    }
                    self.last_timestamp = data.last().unwrap().local_timestamp as usize + 1;
                }
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
    async fn load_from_file(&self, _path: &str) -> Result<Vec<Depth>, std::io::Error> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::BtConfig;
    use super::DataLoader;

    #[tokio::test]
    async fn test_data_loader() {
        let config = BtConfig::default();
        DataLoader::new(10_000, &config).await.load_data().await.unwrap();
    }
}
