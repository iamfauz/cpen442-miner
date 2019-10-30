//! Miner

use structopt;
use structopt::StructOpt;
use hex;
use num_cpus;
use std::path::PathBuf;

mod error;
mod cpen442coin;
mod miner;
mod oclminer;
mod cpuminer;
mod cryptowallet;
mod util;

use error::Error;

#[derive(Debug, StructOpt)]
struct MinerOclOpts {
    /// List OpenCL Devices
    #[structopt(long = "cl-devices")]
    cl_devices : bool,

    /// Number of threads to feed OpenCL
    #[structopt(long = "cl-threads")]
    cl_threads : Option<usize>,

    /// Enable OpenCL
    #[structopt(long = "cl")]
    cl : bool,
}

#[derive(Debug, StructOpt)]
struct MinerOpts {
    /// Use at most this many cores to mine
    #[structopt(short = "j", long)]
    ncpu : Option<usize>,

    /// Miner Identity String
    #[structopt(short = "i", long)]
    identity : Option<String>,

    /// MD5 the Identity String
    #[structopt(long)]
    md5identity : bool,

    /// Use a fake coin tracker (Don't contact cpen442coin.ece.ubc.ca)
    #[structopt(long)]
    fake : bool,
    
    /// File to output mined coins to
    #[structopt(short = "o", long = "output", parse(from_os_str))]
    wallet : Option<PathBuf>,

    #[structopt(flatten)]
    ocl : MinerOclOpts,
}

fn main() -> Result<(), Error> {
    let opt = MinerOpts::from_args();

    if opt.ocl.cl_devices {
        oclminer::list_cl_devices()?;
        return Ok(());
    }

    if opt.identity.is_none() {
        return Err(Error::Msg("Identity is not given! Specify it with --identity.".into()));
    }

    let identity;
    if opt.md5identity {
        use openssl::hash;
        let md5 = hash::hash(hash::MessageDigest::md5(),
            opt.identity.unwrap().as_bytes()).unwrap();
        identity = hex::encode(&md5[..]);
    } else {
        identity = opt.identity.unwrap();
    }

    println!("Mining with Identity: {}", identity);

    let ncpu = opt.ncpu.unwrap_or(num_cpus::get());

    println!("Using {} cpu cores to mine", ncpu);

    let tracker;
    let mut wallet = None;
    if opt.fake {
        println!("WARNING: Using Fake Tracker, Coins Not Recorded!");
        tracker = cpen442coin::Tracker::new_fake(identity.clone());
    } else {
        tracker = cpen442coin::Tracker::new(identity.clone());

        if let Some(wallet_path) = opt.wallet {
            println!("Wallet Path: {:?}", wallet_path);
            wallet = Some(cryptowallet::Wallet::new(wallet_path, identity.clone())?);
        }
    }

    let mut oclf = None;
    let clthreads = opt.ocl.cl_threads.unwrap_or(4);
    if opt.ocl.cl {
        oclf = Some(oclminer::OclMinerFunction::default()?);
        println!("Using OpenCL Device:");
        oclminer::print_device(&oclf.as_ref().unwrap().device)?;

        println!("Using {} threads for OpenCL", clthreads);
    }

    let mut mm = miner::MiningManager::new(tracker, ncpu, clthreads, oclf);

    mm.run(&mut wallet)?;

    Ok(())
}
