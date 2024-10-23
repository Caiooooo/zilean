use sonic_rs::{Deserialize, Serialize};

use crate::dataloader::DatabaseAccount;
#[derive(Deserialize, Serialize)]
#[allow(non_camel_case_types)]
pub struct dbConfig {
    pub database: DatabaseAccount,
}

impl dbConfig {
    pub fn new() -> dbConfig {
        let s = std::fs::read_to_string("misc/config.toml").unwrap();
        toml::de::from_str(&s).unwrap()
    }
}
