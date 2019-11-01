use reqwest;
use hex::FromHexError;
use openssl::error::ErrorStack;
use std::io;
use ocl;

#[derive(Debug)]
pub enum Error {
    Request(reqwest::Error),
    OpenSSL(ErrorStack),
    OpenCL(ocl::Error),
    Hex(FromHexError),
    Io(io::Error),
    BadCoin(String),
    ServerBusy,
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

impl From<ocl::Error> for Error {
    fn from(e : ocl::Error) -> Self {
        Error::OpenCL(e)
    }
}

impl From<io::Error> for Error {
    fn from(e : io::Error) -> Self {
        Error::Io(e)
    }
}

