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
mod ocldevice;
mod cryptowallet;
mod util;
mod proxy;

use error::Error;

#[derive(Debug, StructOpt)]
struct MinerOclOpts {
    /// List OpenCL Devices
    #[structopt(long = "list-cl-devices")]
    cl_devices : bool,

    /// The indexes of the OpenCL devices to use.
    /// --list-cl-devices to list the devices
    #[structopt(long = "cl-device")]
    cl_device_idxs : Option<Vec<usize>>,

    /// Attempt to throttle OpenCL GPUs usage to this ratio [0 to 1]
    #[structopt(long = "cl-max-utilize")]
    cl_utilization : Option<f32>,

    /// Don't allow OpenCL to take longer than this number of milliseconds
    #[structopt(long = "cl-max-ms")]
    cl_max_ms : Option<u32>,
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

    /// HTTP Proxies to use
    #[structopt(long = "proxy-file", parse(from_os_str))]
    http_proxies : Option<PathBuf>,

    /// How often to poll last_coin in milliseconds
    #[structopt(long = "poll-ms", default_value = "6000")]
    poll_ms : u32,

    #[structopt(flatten)]
    ocl : MinerOclOpts,
}

fn main() -> Result<(), Error> {
    let opt = MinerOpts::from_args();

    if opt.ocl.cl_devices {
        for (i, p) in ocldevice::get_cl_devices()?.iter().enumerate() {
            println!("Device #{}", i);
            ocldevice::print_plat_dev_pair(&p)?;
        }
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

    if identity.len() != cpen442coin::MD5_HASH_HEX_LEN {
        return Err(Error::Msg(format!("Identity should be of length {}",
                    cpen442coin::MD5_HASH_HEX_LEN)));
    }

    println!("Mining with Identity: {}", identity);

    let ncpu = opt.ncpu.unwrap_or(num_cpus::get());

    println!("Using {} cpu cores to mine", ncpu);

    let tracker;
    let mut wallet = None;
    if opt.fake {
        println!("WARNING: Using Fake Tracker, Coins Not Recorded!");
        tracker = cpen442coin::Tracker::new_fake(identity.clone())?;
    } else {
        tracker = cpen442coin::Tracker::new(identity.clone(),
            opt.http_proxies.unwrap_or(PathBuf::new()))?;

        if let Some(wallet_path) = opt.wallet {
            println!("Wallet Path: {:?}", wallet_path);
            wallet = Some(cryptowallet::Wallet::new(wallet_path, identity.clone())?);
        }
    }

    let mut all_oclfs = Vec::new();
    if let Some(cl_device_idxs) = opt.ocl.cl_device_idxs {
        for idx in cl_device_idxs {
            let devices = ocldevice::get_cl_devices()?;

            let mut oclf;
            if let Some(p) = devices.get(idx) {
                println!("Using OpenCL Device:");
                ocldevice::print_plat_dev_pair(p)?;

                oclf = oclminer::OclMinerFunction::new(p.0, p.1)?;
            } else {
                return Err(Error::Msg(format!("Bad OpenCL device Index: {}", idx)));
            }

            if let Some(th) = opt.ocl.cl_utilization {
                oclf.throttle(th)?;
            }

            if let Some(ms) = opt.ocl.cl_max_ms {
                oclf.set_max_loop_ms(ms);
            }

            all_oclfs.push(oclf);
        }
    }

    let mut mm = miner::MiningManager::new(tracker, ncpu, all_oclfs, opt.poll_ms);

    mm.run(&mut wallet)?;

    Ok(())
}
