use std::default;

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub enum DataSource {
    #[default]
    Database,
    FilePath(String),
}

#[derive(Default)]
pub struct DataLoader {
    source: DataSource,
    limit: usize,
    offset: usize,
}

impl DataLoader {
    pub fn new(source: DataSource, limit: usize, offset: usize) -> DataLoader {
        Self {
            source,
            limit,
            offset,
        }
    }
    pub fn load_data() -> Result<Vec<String>, std::io::Error> {
        todo!()
    }

    fn load_from_database() {
        todo!()
    }
    fn load_from_directory() {
        todo!()
    }
    fn load_from_file() {
        todo!()
    }
}
