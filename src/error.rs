use reqwest;
use hex::FromHexError;
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

