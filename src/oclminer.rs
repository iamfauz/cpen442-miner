use ocl;
use openssl;
use ocl_extras::full_device_info::FullDeviceInfo;
use rand::{RngCore, rngs::OsRng};
use crate::{
    error::Error,
    cpen442coin, cpen442coin::COIN_PREFIX_STR,
    miner::{Coin, Stats, Miner, MinerFunction, MinerThreadData, MinerSharedData},
    util::Timer
};
use std::mem::size_of;
use std::sync::{Arc, atomic::Ordering};
use std::time::{Instant, Duration};
use std::thread;

pub type OclMiner = Miner<OclMinerFunction>;

#[derive(Clone)]
pub struct OclMinerFunction {
    context : ocl::Context,
    program : ocl::Program,
    device : ocl::Device,
    throttle_of_100 : u32
}

const DEBUG_ENABLE : usize = 0; // 0 or 1

const OCL_WORD_LEN : usize = size_of::<u32>();
const OCL_MESSAGE_LEN : usize = cpen442coin::MD5_BLOCK_LEN * 4;
const OCL_BLOB_INDEX : usize = 5 * 4 + cpen442coin::MD5_HASH_HEX_LEN;
const OCL_BLOB_LEN_FAST : usize = 32 * 4;
// Message: ["CPEN 442..." | PREV_COIN | BLOB | TRACKER]
const OCL_BLOB_LEN : usize = OCL_MESSAGE_LEN - cpen442coin::MD5_HASH_HEX_LEN - OCL_BLOB_INDEX;
const OCL_COUNTER_INDEX : usize = cpen442coin::MD5_BLOCK_LEN * 3 + OCL_WORD_LEN;
const OCL_N_LOOPS : u32 = 64;
const OCL_N_LOOPS_2 : u32 = 256;
const MD5PROGRAM : &str = include_str!("cl/MD5.cl");

// Same transformation as happens on the GPU
fn message_for_id(message_base: &[u8], mod_start: usize, mod_end: usize,
    id: u32, idx : u32, idx2 : u32, r : &[u32; 4]) -> Vec<u8> {
    use slice_of_array::SliceArrayExt;
    const BLOB_INDEX : usize = OCL_BLOB_INDEX / OCL_WORD_LEN;
    const BLOB_LEN_FAST : usize = OCL_BLOB_LEN_FAST / OCL_WORD_LEN;
    const LAST_ROUND_COUNTER_INDEX : usize = OCL_COUNTER_INDEX / OCL_WORD_LEN;
    let mut message = Vec::from(&message_base[mod_start..mod_end]);
    if DEBUG_ENABLE > 0 {
        println!("id {}, idx {}, idx2 {}", id, idx, idx2);
        println!("LAST_ROUND_COUNTER_INDEX {}", LAST_ROUND_COUNTER_INDEX);
    }

    for i in (0..message_base.len()).step_by(OCL_WORD_LEN) {
        let wrd_idx = i / OCL_WORD_LEN;
        if mod_start <= i && i < mod_end {
            let mut val = u32::from_le_bytes(*message_base[i..i+OCL_WORD_LEN].as_array());

            if wrd_idx == (BLOB_INDEX + ((id + r[0]) as usize) % BLOB_LEN_FAST) {
                val += id + idx * 4;
            }

            if wrd_idx == (BLOB_INDEX + (((id + r[1]) as usize) + BLOB_LEN_FAST / 4) % BLOB_LEN_FAST) {
                val ^= (id << 16) | id;
            }

            if wrd_idx == (BLOB_INDEX + BLOB_LEN_FAST) {
                val += (id << 16) + idx - r[2];
            }

            if wrd_idx == LAST_ROUND_COUNTER_INDEX {
                val = val + (idx2 >> 2) + (idx2 << 24) + (idx << 12);
            }

            message[i - mod_start..i - mod_start + OCL_WORD_LEN]
                .copy_from_slice(&val.to_le_bytes());
        }
    }

    message
}

impl OclMinerFunction {
    pub fn new(platform : ocl::Platform, device : ocl::Device) -> Result<Self, Error> {
        let context = ocl::Context::builder()
            .platform(platform)
            .devices(&device)
            .build()?;

        let mut md5_program = format!("
#define MESSAGE_LEN ({message_len})
#define BLOB_INDEX ({blob_index})
#define BLOB_LEN_FAST ({blob_len_fast})
#define BLOB_LEN ({blob_len})
#define N_LOOPS ({n_loops})
#define N_LOOPS_2 ({n_loops_2})
#define LAST_ROUND_COUNTER_INDEX ({counter_index})
\n",
            message_len=OCL_MESSAGE_LEN / OCL_WORD_LEN,
            blob_index=OCL_BLOB_INDEX / OCL_WORD_LEN,
            blob_len_fast=OCL_BLOB_LEN_FAST / OCL_WORD_LEN,
            blob_len=OCL_BLOB_LEN / OCL_WORD_LEN,
            n_loops=OCL_N_LOOPS,
            n_loops_2=OCL_N_LOOPS_2,
            counter_index=OCL_COUNTER_INDEX / OCL_WORD_LEN);

        if DEBUG_ENABLE > 0 {
            md5_program += "\n\n#define __DEBUG_MODE__\n\n";
        }

        md5_program += MD5PROGRAM;

        let program = ocl::Program::builder()
            .devices(&device)
            .src(md5_program)
            .build(&context)?;

        Ok(Self {
            context,
            program,
            device,
            throttle_of_100: 0
        })
    }

    pub fn throttle(&mut self, utilization : f32) -> Result<(), Error> {
        if 0.0 <= utilization && utilization <= 1.0 {
            self.throttle_of_100 = (100.0 * (1.0 - utilization)) as u32;

            println!("GPU Utilization: {} / 100", 100 - self.throttle_of_100);

            Ok(())
        } else {
            Err(Error::Msg("Utilization should be between 0 and 1".into()))
        }
    }
}

impl MinerFunction for OclMinerFunction {
    fn run(self, tdata : MinerThreadData, tsdata: Arc<MinerSharedData>) -> Result<(), Error> {
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
            .arg_named("params_in", None::<&ocl::Buffer<u32>>)
            .arg_named("params_out", None::<&ocl::Buffer<u32>>)
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
                let elapsed = (start.elapsed().as_nanos() + (OsRng.next_u64() as u128) << 64).to_ne_bytes();
                message[i..i+elapsed[..].len()].copy_from_slice(&elapsed);
                i += elapsed[..].len();

                // openssl RNG
                openssl::rand::rand_bytes(&mut message[i..i + 16]).unwrap();
                i += 16;

                // OS RNG
                OsRng.fill_bytes(&mut message[i..i + 16]);
                i += 16;

                // rand RNG
                rng.fill_bytes(&mut message[i..modifiable_end]);
            }

            let message_words : &[u32; OCL_MESSAGE_LEN / OCL_WORD_LEN] = 
                unsafe { std::mem::transmute(&message) };

            let msg_buf = ocl::Buffer::<u32>::builder()
                .queue(queue.clone())
                .flags(ocl::flags::MEM_READ_ONLY)
                .len(OCL_MESSAGE_LEN / OCL_WORD_LEN)
                .copy_host_slice(message_words)
                .build()?;

            let params_in = [
                OsRng.next_u32(),
                rng.next_u32(),
                OsRng.next_u32(),
                rng.next_u32(),
            ];

            let params_in_buf = ocl::Buffer::<u32>::builder()
                .queue(queue.clone())
                .flags(ocl::flags::MEM_READ_ONLY)
                .len(4)
                .copy_host_slice(&params_in)
                .build()?;

            const OCL_PARAMS_LEN : usize = 4 +
                DEBUG_ENABLE * (OCL_MESSAGE_LEN + cpen442coin::MD5_HASH_LEN);

            let mut params_out = [0xFFFFFFFFu32; OCL_PARAMS_LEN];

            let params_out_buf = ocl::Buffer::<u32>::builder()
                .queue(queue.clone())
                .flags(ocl::flags::MEM_READ_WRITE)
                .len(OCL_PARAMS_LEN)
                .copy_host_slice(&params_out[..])
                .build()?;

            kernel.set_arg("base_message", &msg_buf)?;
            kernel.set_arg("params_in", &params_in_buf)?;
            kernel.set_arg("params_out", &params_out_buf)?;

            unsafe { kernel.cmd().queue(&queue).enq()?; }

            params_out_buf.cmd()
                .queue(&queue)
                .offset(0)
                .read(&mut params_out[..])
                .enq()?;

            queue.finish()?;

            if params_out[0] != 0xFFFFFFFF {
                if DEBUG_ENABLE > 0 {
                    let mut hash = Vec::new();

                    for i in 3..=6 {
                        hash.extend_from_slice(&params_out[i].to_le_bytes());
                    }

                    println!("\nDEBUG GPU Hash: {}\n", hex::encode(hash));

                    let mut gpu_message = Vec::new();

                    for i in 7..7+(OCL_MESSAGE_LEN / OCL_WORD_LEN) {
                        gpu_message.extend_from_slice(&params_out[i].to_le_bytes());
                    }

                    println!("\nDEBUG GPU Message: {}\n", hex::encode(gpu_message));
                    println!("\nDEBUG Base Message: {}\n", hex::encode(&message[..]));
                }

                let coin = Coin {
                    previous_coin : (*previous_coin).clone(),
                    //blob : message_for_id(message_words, i as u32)
                    blob : message_for_id(&message, modifiable_start, modifiable_end,
                        params_out[0], params_out[1], params_out[2], &params_in)
                };

                match tdata.coin_schan.send(coin) {
                    Ok(_) => {},
                    Err(e) => return Err(Error::new(
                            format!("Miner Mpsc Send Failed: {:?}", e)))
                };
            }

            total_ms += loop_start.elapsed().as_millis() as u64;
            loop_iterations += 1;
            counter += (OCL_N_LOOPS as u64) * (OCL_N_LOOPS_2 as u64) * parallel as u64;

            if last_report_timer.check_and_reset() {
                //println!("\nLoop Time {}ms", total_ms as f64 / loop_iterations as f64);
                tdata.stats_schan.send(Stats{
                    nhash: counter,
                    loopms: Some(total_ms / loop_iterations)
                }).unwrap();
                counter = 0;
            }

            if loop_iterations % 100 < self.throttle_of_100 as u64 {
                thread::sleep(Duration::from_millis(2 * total_ms / loop_iterations));
            }
        }

        Ok(())
    }
}

fn to_big_endian_u32(buf : &mut [u8]) {
    use slice_of_array::SliceArrayExt;
    assert_eq!(buf.len() % OCL_WORD_LEN, 0);

    for i in (0..buf.len()).step_by(OCL_WORD_LEN) {
        let val = u32::from_le_bytes(*buf[i..i+OCL_WORD_LEN].as_array());

        buf[i..i+OCL_WORD_LEN].copy_from_slice(&val.to_be_bytes());
    }
}
