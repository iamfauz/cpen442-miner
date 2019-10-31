use ocl;
use openssl;
use ocl_extras::full_device_info::FullDeviceInfo;
use rand::RngCore;
use crate::{
    error::Error,
    cpen442coin, cpen442coin::COIN_PREFIX_STR,
    miner::{Coin, Stats, Miner, MinerFunction, MinerThreadData, MinerSharedData},
    util::Timer
};
use std::sync::{Arc, atomic::Ordering};
use std::time::{Instant, Duration};

pub fn list_cl_devices() -> Result<(), Error> {
    for (i, platform) in ocl::Platform::list().iter().enumerate() {
        println!("Platform {}: {} {}", i,
            platform.name()?,
            platform.version()?);
        for (i, device) in ocl::Device::list_all(platform)?.iter().enumerate() {
            println!("  Device {}:", i);
            print_device(device)?;
        }
    }

    Ok(())
}

pub fn print_device(device : &ocl::Device) -> Result<(), Error>{
    use ocl::enums::{DeviceInfo, DeviceInfoResult};
    use ocl::flags::{DEVICE_TYPE_CPU, DEVICE_TYPE_GPU, DEVICE_TYPE_ACCELERATOR};
    println!("    {} {}",
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

    Ok(())
}


pub type OclMiner = Miner<OclMinerFunction>;

#[derive(Clone)]
pub struct OclMinerFunction {
    #[allow(dead_code)]
    pub context : ocl::Context,
    pub program : ocl::Program,
    pub device : ocl::Device,
}

const OCL_BLOB_INDEX : usize = (5 + 8) * 4;
const OCL_BLOB_LEN : usize = 32 * 4;
const OCL_MESSAGE_LEN : usize = cpen442coin::MD5_BLOCK_LEN * 4;
const OCL_N_LOOPS : u32 = 4096;
const MD5PROGRAM : &str = include_str!("cl/MD5.cl");

// Same transformation as happens on the GPU
fn message_for_id(message_base: &[u8], mod_start: usize, mod_end: usize, id: u32, idx : u32) -> Vec<u8> {
    use std::mem::size_of;
    use slice_of_array::SliceArrayExt;
    const BLOB_INDEX : usize = OCL_BLOB_INDEX / 4;
    const BLOB_LEN : usize = OCL_BLOB_LEN / 4;
    let mut message = Vec::from(&message_base[mod_start..mod_end]);

    for i in (0..message_base.len()).step_by(size_of::<u32>()) {
        let wrd_idx = i / size_of::<u32>();
        if BLOB_INDEX <= wrd_idx && wrd_idx <= BLOB_INDEX + BLOB_LEN {
            let mut val = u32::from_le_bytes(*message_base[i..i+size_of::<u32>()].as_array());

            if wrd_idx == (BLOB_INDEX + (id as usize) % BLOB_LEN) {
                val += id + idx;
            }

            if wrd_idx == (BLOB_INDEX + ((id as usize) + BLOB_LEN / 4) % BLOB_LEN) {
                val ^= (id << 16) | id;
            }

            if wrd_idx == (BLOB_INDEX + BLOB_LEN) {
                val += (id << 16) + idx;
            }

            message[i - mod_start..i - mod_start + size_of::<u32>()]
                .copy_from_slice(&val.to_le_bytes());
        }
    }

    message
}

impl OclMinerFunction {
    pub fn default() -> Result<Self, Error> {
        Ok(Self::new(ocl::Platform::first()?,
        ocl::Device::first(ocl::Platform::first()?)?)?)
    }

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
#define N_LOOPS ({n_loops})
\n",
            message_len=OCL_MESSAGE_LEN / size_of::<u32>(),
            blob_index=OCL_BLOB_INDEX / size_of::<u32>(),
            blob_len=OCL_BLOB_LEN / size_of::<u32>(),
            n_loops=OCL_N_LOOPS);

        md5_program += MD5PROGRAM;

        let program = ocl::Program::builder()
            .devices(&device)
            .src(md5_program)
            .build(&context)?;

        Ok(Self {
            context,
            program,
            device,
        })
    }
}

impl MinerFunction for OclMinerFunction {
    fn run(self, tdata : MinerThreadData, tsdata: Arc<MinerSharedData>) -> Result<(), Error> {
        use std::mem::size_of;

        let mut rng = rand::thread_rng();

        let start = Instant::now();
        let mut counter = 0;
        let mut total_ms = 0;
        let mut loop_iterations = 0;
        let mut last_report_timer = Timer::new(Duration::from_millis(2000));
        let mut previous_coin = tsdata.previous_coin.take(Ordering::Relaxed).unwrap();

        let mut message = [0u8; OCL_MESSAGE_LEN];
        let modifiable_start = COIN_PREFIX_STR.len() + previous_coin.len();
        let modifiable_end = message.len() - tdata.miner_id.len();

        message[0..COIN_PREFIX_STR.len()].copy_from_slice(COIN_PREFIX_STR.as_bytes());
        message[COIN_PREFIX_STR.len()..modifiable_start].copy_from_slice(previous_coin.as_bytes());
        message[modifiable_end..OCL_MESSAGE_LEN]
            .copy_from_slice(tdata.miner_id.as_bytes());

        if ! self.device.endian_little().expect("Failed to get Endianess") {
            to_big_endian_u32(&mut message);
            panic!("Big Endian!");
        }

        let parallel = self.device.max_wg_size()?;

        let queue = ocl::Queue::new(&self.context, self.device.clone(), None)?;

        let kernel = ocl::Kernel::builder()
            .program(&self.program)
            .name("md5")
            .queue(queue.clone())
            .global_work_size(parallel)
            .arg_named("base_message", None::<&ocl::Buffer<u32>>)
            .arg_named("md5_prefix_out", None::<&ocl::Buffer<u32>>)
            .arg_named("message_out", None::<&ocl::Buffer<u8>>)
            .build()?;

        while ! tsdata.should_stop.load(Ordering::Relaxed) {
            let loop_start = Instant::now();

            if let Some(new_coin) = tsdata.previous_coin.take(Ordering::Relaxed) {
                previous_coin = new_coin;
                message[COIN_PREFIX_STR.len()..modifiable_start].copy_from_slice(previous_coin.as_bytes());
            }

            // Generate a random base message
            {
                let mut i = modifiable_start;
                // Add timestamp
                let elapsed = start.elapsed().as_nanos().to_ne_bytes();
                message[i..i+elapsed[..].len()].copy_from_slice(&elapsed);
                i += elapsed[..].len();

                // openssl RNG
                openssl::rand::rand_bytes(&mut message[i..i + 16]).unwrap();
                i += 16;

                // rand RNG
                rng.fill_bytes(&mut message[i..modifiable_end]);
            }

            let message_words : &[u32; OCL_MESSAGE_LEN / size_of::<u32>()] = 
                unsafe { std::mem::transmute(&message) };

            let msg_buf = ocl::Buffer::<u32>::builder()
                .queue(queue.clone())
                .flags(ocl::flags::MEM_READ_ONLY)
                .len(OCL_MESSAGE_LEN / size_of::<u32>())
                .copy_host_slice(message_words)
                .build()?;

            let mut id_out = vec![0xFFFFFFFFu32; 6];

            let id_out_buf = ocl::Buffer::<u32>::builder()
                .queue(queue.clone())
                .flags(ocl::flags::MEM_READ_WRITE)
                .len(6)
                .copy_host_slice(&id_out[..])
                .build()?;

            let mut msg_out = vec![0u8; OCL_MESSAGE_LEN];

            let msg_out_buf = ocl::Buffer::<u8>::builder()
                .queue(queue.clone())
                .flags(ocl::flags::MEM_READ_WRITE)
                .len(OCL_MESSAGE_LEN)
                .copy_host_slice(&msg_out[..])
                .build()?;

            kernel.set_arg("base_message", &msg_buf)?;
            kernel.set_arg("md5_prefix_out", &id_out_buf)?;
            kernel.set_arg("message_out", &msg_out_buf)?;

            unsafe { kernel.cmd().queue(&queue).enq()?; }

            id_out_buf.cmd()
                .queue(&queue)
                .offset(0)
                .read(&mut id_out)
                .enq()?;

            msg_out_buf.cmd()
                .queue(&queue)
                .offset(0)
                .read(&mut msg_out)
                .enq()?;

            queue.finish()?;

            if id_out[0] != 0xFFFFFFFF {
                let coin = Coin {
                    previous_coin : (*previous_coin).clone(),
                    //blob : message_for_id(message_words, i as u32)
                    blob : message_for_id(&message, modifiable_start, modifiable_end,
                        id_out[0], id_out[1])
                };

                match tdata.coin_schan.send(coin) {
                    Ok(_) => {},
                    Err(e) => return Err(Error::new(
                            format!("Miner Mpsc Send Failed: {:?}", e)))
                };
            }

            total_ms += loop_start.elapsed().as_millis() as u64;
            loop_iterations += 1;
            counter += OCL_N_LOOPS as u64 * parallel as u64;
            if last_report_timer.check_and_reset() {
                //println!("Loop Time {}ms", total_ms as f64 / loop_iterations as f64);
                tdata.stats_schan.send(Stats{
                    nhash: counter,
                    loopms: Some(total_ms / loop_iterations)
                }).unwrap();
                counter = 0;
            }
        }

        Ok(())
    }
}

fn to_big_endian_u32(buf : &mut [u8]) {
    use std::mem::size_of;
    use slice_of_array::SliceArrayExt;
    assert_eq!(buf.len() % size_of::<u32>(), 0);

    for i in (0..buf.len()).step_by(size_of::<u32>()) {
        let val = u32::from_le_bytes(*buf[i..i+size_of::<u32>()].as_array());

        buf[i..i+size_of::<u32>()].copy_from_slice(&val.to_be_bytes());
    }
}
