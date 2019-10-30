use ocl;
use crate::{
    error::Error,
    cpen442coin,
    miner::{Coin, Stats, Miner, MinerFunction, MinerThreadData, MinerSharedData}
};
use std::thread;
use atomic_option::AtomicOption;
use std::sync::{
    atomic::{AtomicBool,Ordering},
    mpsc,
    Arc,
};

pub fn list_cl_devices() -> Result<(), Error> {
    for (i, platform) in ocl::Platform::list().iter().enumerate() {
        println!("Platform {}: {} {}", i,
            platform.name()?,
            platform.version()?);
        for (i, device) in ocl::Device::list_all(platform)?.iter().enumerate() {
            use ocl::enums::{DeviceInfo, DeviceInfoResult};
            use ocl::flags::{DEVICE_TYPE_CPU, DEVICE_TYPE_GPU, DEVICE_TYPE_ACCELERATOR};
            println!("  Device {}: {} {}", i,
                device.vendor()?,
                device.name()?);

            let dev_info = [
                DeviceInfo::Type,
                DeviceInfo::MaxComputeUnits,
                DeviceInfo::MaxWorkGroupSize,
                DeviceInfo::GlobalMemSize];
            for info in &dev_info {
                match device.info(*info)? {
                    DeviceInfoResult::Type(t) => {
                        let mut t_str = String::new();
                        if t.contains(DEVICE_TYPE_CPU) {
                            t_str += " CPU";
                        }
                        if t.contains(DEVICE_TYPE_GPU) {
                            t_str += " GPU";
                        }

                        if t.contains(DEVICE_TYPE_ACCELERATOR) {
                            t_str += " ACCELERATOR";
                        }
                        println!("    Device Type:{}", t_str);
                    },
                    DeviceInfoResult::MaxComputeUnits(n) => {
                        println!("    Compute Units: {}", n);
                    },
                    DeviceInfoResult::MaxWorkGroupSize(n) => {
                        println!("    Workgroup Size: {}", n);
                    },
                    DeviceInfoResult::GlobalMemSize(m) => {
                        println!("    Memory Size: {} MB", m / 1024 / 1024);
                    },
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

pub type OclMiner = Miner<OclMinerFunction>;

pub struct OclMinerFunction {
    context : ocl::Context,
    program : ocl::Program,
    device : ocl::Device,
    queue : ocl::Queue,
}

const OCL_MESSAGE_LEN : usize = cpen442coin::MD5_BLOCK_LEN * 4;

const MD5PROGRAM_DEFS : &str = "
//#define N_PARALLEL ({n_parallel})
";

const MD5PROGRAM : &str = include_str!("cl/MD5_2.cl");

impl OclMinerFunction {
    pub fn new(platform : ocl::Platform, device : ocl::Device) -> Result<Self, Error> {
        let context = ocl::Context::builder()
            .platform(platform)
            .devices(&device)
            .build()?;

        use std::mem::size_of;

        let mut md5_program = format!("
#define MESSAGE_LEN ({message_len})
#define BLOB_INDEX ({blob_index})
#define BLOB_LEN ({blob_len})
\n",
            message_len=OCL_MESSAGE_LEN / size_of::<u32>(),
            blob_index=(5 + 8),
            blob_len=32);

        md5_program += MD5PROGRAM;

        let program = ocl::Program::builder()
            .devices(&device)
            .src(md5_program)
            .build(&context)?;

        let queue = ocl::Queue::new(&context, device.clone(), None)?;

        Ok(Self {
            context,
            program,
            device,
            queue,
        })
    }
}

impl MinerFunction for OclMinerFunction {
    fn run(self, tdata : MinerThreadData, tsdata: Arc<MinerSharedData>) -> Result<(), Error> {

        Ok(())
    }
}
