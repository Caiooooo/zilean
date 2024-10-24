use sonic_rs::{Deserialize, Serialize};

use crate::dataloader::DatabaseAccount;

#[derive(Debug, Deserialize, Serialize)]
pub struct ZConfig {
    pub start_url: String,
    pub tick_url: String,
    pub debug: bool,
    pub database: DatabaseAccount,
}

impl ZConfig {
    pub fn parse(path: &str) -> ZConfig {
        let s = std::fs::read_to_string(path).unwrap();
        toml::de::from_str(&s).unwrap()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn parse_config() {
        let config = ZConfig::parse("./misc/config.toml");
        println!("{:?}", config);
    }
}
