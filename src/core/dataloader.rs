#[derive(Debug)]
pub enum DataSource {
    Database,
    FilePath(String),
}

#[derive(Default)]
pub struct DataLoader;

impl DataLoader {
    pub fn load_data(source: DataSource) -> Result<Vec<String>, std::io::Error> {

    }

    fn load_from_database();
    fn load_from_directory();
    fn load_from_file();
}