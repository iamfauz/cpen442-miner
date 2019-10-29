use ocl;
use crate::error::Error;

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

pub struct OclMiner {
    program : ocl::Program,
    context : ocl::Context,
    device : ocl::Device,
    queue : ocl::Queue
}
