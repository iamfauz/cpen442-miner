use ocl;
use crate::Error;

pub type OclDeviceList = Vec<(ocl::Platform, ocl::Device)>;

pub fn get_cl_devices() -> Result<OclDeviceList, Error> {
    let mut list = OclDeviceList::new();

    for platform in ocl::Platform::list() {
        for device in ocl::Device::list_all(platform)? {
            list.push((platform.clone(), device));
        }
    }

    Ok(list)
}

pub fn print_plat_dev_pair(p : &(ocl::Platform, ocl::Device)) -> Result<(), Error> {
    use ocl::enums::{DeviceInfo, DeviceInfoResult};
    use ocl::flags::{DEVICE_TYPE_CPU, DEVICE_TYPE_GPU, DEVICE_TYPE_ACCELERATOR};
    println!("  {} {}", p.0.name()?, p.0.version()?);
    println!("  {} {}",
        p.1.vendor()?,
        p.1.name()?);

    let dev_info = [
        DeviceInfo::Type,
        DeviceInfo::MaxComputeUnits,
        DeviceInfo::MaxWorkGroupSize,
        DeviceInfo::GlobalMemSize];
    for info in &dev_info {
        match p.1.info(*info)? {
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
                println!("  Device Type:{}", t_str);
            },
            DeviceInfoResult::MaxComputeUnits(n) => {
                println!("  Compute Units: {}", n);
            },
            DeviceInfoResult::MaxWorkGroupSize(n) => {
                println!("  Workgroup Size: {}", n);
            },
            DeviceInfoResult::GlobalMemSize(m) => {
                println!("  Memory Size: {} MB", m / 1024 / 1024);
            },
            _ => {}
        }
    }

    Ok(())
}

