
use std::fs::{File, OpenOptions};
use std::io::{Write, Error};
use std::path::PathBuf;
use serde::Serialize;
use serde_json;
use chrono;

#[derive(Serialize)]
struct CoinRecord {
    id_of_miner : String,
    last_coin : String,
    coin_blob : String,
}

pub struct Wallet {
    id : String,
    file : File,
}

impl Wallet {
    pub fn new(filepath : PathBuf, id : String) -> Result<Self, Error> {
        let file = OpenOptions::new()
            .create(true)
            .read(false)
            .write(true)
            .truncate(false)
            .append(true)
            .open(filepath)?;

        Ok(Self { id, file })
    }

    pub fn store(&mut self, blob_str : String, last_coin : String) {
        let record = CoinRecord {
            id_of_miner: self.id.clone(),
            last_coin,
            coin_blob: blob_str
        };

        write!(&self.file, "\nMined on {}\n", 
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"))
            .unwrap();

        serde_json::to_writer_pretty(&self.file, &record)
            .expect("Failed to write record");

        self.file.write("\n".as_bytes()).unwrap();

        self.file.flush().unwrap();
    }
}
