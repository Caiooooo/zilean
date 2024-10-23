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
#[derive(Deserialize, Serialize, Clone)]
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
    offset: usize,
}

impl DataLoader {
    pub fn new(limit: usize, config: &BtConfig) -> DataLoader {
        let config_clone = config.clone();
        let source = match config.source.clone() {
            Some(source) => source,
            _ => DataSource::Database,
        };
        let database = match source {
            DataSource::Database => Some(config::dbConfig::new().database),
            _ => None,
        };
        DataLoader {
            config: config_clone,
            database,
            source,
            limit,
            offset: 0,
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

        let ret = 
            // TODO 支持期货查询
            client
                .query(
                    "SELECT ?fields FROM ? WHERE exch_timestamp >= ? AND exch_timestamp < ? LIMIT ? OFFSET ?",
                )
                .bind(Identifier(
                    format!("{}_spot", &self.config.symbol.split("_").next().unwrap())
                        .to_lowercase()
                        .as_str(),
                ))
                .bind(self.config.start_time)
                .bind(self.config.end_time)
                .bind(self.limit)
                .bind(self.offset)
                .fetch_all::<Depth>()
                .await;
        match ret {
            Ok(data) => {
                debug!("query success");
                self.offset += self.limit;
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
        DataLoader::new(10_000, &config).load_data().await.unwrap();
    }
}
