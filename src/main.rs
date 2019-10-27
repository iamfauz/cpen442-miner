//! Miner

use hex::FromHexError;
use reqwest;
use openssl::error::ErrorStack;

#[derive(Debug)]
pub enum Error {
    Request(reqwest::Error),
    OpenSSL(ErrorStack),
    Hex(FromHexError),
    Msg(String)
}

impl Error {
    pub fn new(msg : String) -> Self {
        Error::Msg(msg)
    }
}

impl From<FromHexError> for Error {
    fn from(e : FromHexError) -> Self {
        Error::Hex(e)
    }
}

impl From<ErrorStack> for Error {
    fn from(e : ErrorStack) -> Self {
        Error::OpenSSL(e)
    }
}

impl From<reqwest::Error> for Error {
    fn from(e : reqwest::Error) -> Self {
        Error::Request(e)
    }
}

mod cpen442coin;
mod miner;

fn main() {
    let tracker = cpen442coin::Tracker::new("b33df5e6d48aa36ef96896b695223ce8".into());

    let mut mm = miner::MiningManager::new(tracker);

    mm.run(Some("2e3a8e88a060cedcd9ac7b74fadd58e0".into()));
}
