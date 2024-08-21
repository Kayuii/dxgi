use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Once;
use windows::core::HSTRING;
use windows::Win32::Foundation::{LUID, TRUE};
use windows::Win32::Graphics::Direct3D::{
    D3D11_SRV_DIMENSION_TEXTURE2D, D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_0,
    D3D_FEATURE_LEVEL_11_1,
};

use windows::Win32::Graphics::Dxgi::{IDXGIFactory4, DXGI_CREATE_FACTORY_FLAGS};
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwareness, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, PROCESS_PER_MONITOR_DPI_AWARE,
};

use windows::Win32::UI::WindowsAndMessaging::SetProcessDPIAware;
use windows::{
    core::{Interface, Result},
    Graphics::DirectX::Direct3D11::IDirect3DDevice,
    Win32::{
        Graphics::{
            Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP},
            Direct3D11::{
                D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_FLAG, D3D11_SDK_VERSION,
            },
            Dxgi::{
                CreateDXGIFactory2, IDXGIAdapter1, IDXGIDevice, IDXGIFactory7,
                IDXGIOutput6, DXGI_ADAPTER_DESC1, DXGI_ADAPTER_FLAG, DXGI_ADAPTER_FLAG_NONE,
                DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_ERROR_UNSUPPORTED, DXGI_OUTDUPL_DESC,
                DXGI_OUTPUT_DESC,
            },
        },
        System::WinRT::Direct3D11::{
            CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
        },
    },
};

use crate::{AdapterDesc, Luid};

use crate::OutputDuplication;

pub fn init() {
    ro_initialize_once();
    become_dpi_aware();
}

fn ro_initialize_once() {
    static mut STATE: AtomicBool = AtomicBool::new(false);
    unsafe {
        let state = STATE.swap(true, Ordering::SeqCst);
        if !state {
            RoInitialize(RO_INIT_MULTITHREADED).ok();
        }
    };
}

fn set_dpi_aware() -> bool {
    unsafe {
        let _bool = SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE).is_ok();
        log::info!("SetProcessDpiAwareness [{}]", _bool);
        _bool
    }
}

fn set_process_dpi_awareness() -> bool {
    unsafe {
        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).is_err() {
            let _bool =
                SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE).is_ok();
            log::info!("SetProcessDpiAwarenessContext [{}]", _bool);
            _bool
        } else {
            true
        }
    }
}

fn become_dpi_aware() {
    static BECOME_AWARE: Once = Once::new();
    BECOME_AWARE.call_once(|| {
        if !set_dpi_aware() && !set_process_dpi_awareness() {
            let _bool = unsafe { SetProcessDPIAware().as_bool() };
            log::info!("SetProcessDPIAware [{}]", _bool);
        }
    });
}

fn _co_init() {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).unwrap();
    }
}

fn find_terminal_idx(content: &[u16]) -> usize {
    for (i, val) in content.iter().enumerate() {
        if *val == 0 {
            return i;
        }
    }
    content.len()
}
pub fn convert_u16_to_string(data: &[u16]) -> String {
    let terminal_idx = find_terminal_idx(data);
    HSTRING::from_wide(&data[0..terminal_idx])
        .expect("Strings are valid Unicode")
        .to_string_lossy()
}

pub(crate) fn create_d3d_device() -> Result<ID3D11Device> {
    for driver_type in [D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP] {
        let mut device = None;
        let result = unsafe {
            D3D11CreateDevice(
                None,
                driver_type,
                None,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device as *mut _),
                None,
                None,
            )
        };
        match result {
            Ok(_) => return Ok(device.unwrap()),
            Err(e) if e.code() == DXGI_ERROR_UNSUPPORTED => continue,
            Err(e) => return Err(e),
        };
    }
    // TODO result
    panic!("failed to create D3D device with any of the types");
}

pub(crate) fn create_direct3d_device(d3d_device: &ID3D11Device) -> Result<IDirect3DDevice> {
    let dxgi_device: IDXGIDevice = d3d_device.cast()?;
    let inspectable = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi_device)? };
    inspectable.cast()
}

pub(crate) fn get_d3d_interface_from_object<S: Interface, R: Interface>(object: &S) -> Result<R> {
    let access: IDirect3DDxgiInterfaceAccess = object.cast()?;
    let object = unsafe { access.GetInterface::<R>()? };
    Ok(object)
}

pub fn get_hardware_adapters_desc() -> Option<Vec<AdapterDesc>> {
    let factory = unsafe {
        match CreateDXGIFactory2::<IDXGIFactory7>(DXGI_CREATE_FACTORY_FLAGS(0u32)) {
            Ok(factory) => factory,
            Err(e) => {
                log::debug!("factory2 init fail: {:?}", e);
                return None;
            }
        }
    };

    // --- Enumerate adapters
    let mut adapters = Vec::new();
    unsafe {
        let mut i = 0;
        while let Ok(adapter) = factory.EnumAdapters1(i) {
            adapters.push(adapter);
            i += 1;
        }
    };

    let mut adapters_desc = Vec::new();
    let mut i = 0;
    for adapter in adapters {
        unsafe {
            match adapter.GetDesc1() {
                Ok(adapter_desc) => {
                    let adapter_flag = DXGI_ADAPTER_FLAG(adapter_desc.Flags as _);

                    let adapter_name = convert_u16_to_string(adapter_desc.Description.as_ref());
                    // let _adapter_dedicated_video_memory = adapter_desc.DedicatedVideoMemory;
                    let luid = adapter_desc.AdapterLuid;
                    // let high_part = adapter_desc.AdapterLuid.HighPart;
                    // let low_part = adapter_desc.AdapterLuid.LowPart;
                    let vendor_id = adapter_desc.VendorId;
                    let device_id = adapter_desc.DeviceId;

                    let is_integrated =
                        vendor_id == 0x8086 && (device_id == 0x163c || device_id == 0x1616);
                    let is_discrete = !is_integrated;
                    let is_default = false;
                    let mut is_software = false;

                    // desc.VendorId == 0x10DE  NVIDIA
                    // desc.VendorId == 0x1002 || 0x1022  AMD
                    // desc.VendorId == 0x8086) 0x163C, 0x8087 Intel
                    // Skip the software adaptor.
                    if (vendor_id == 0x1414) && (device_id == 0x8c) {
                        // log::debug!("is Microsoft Basic Render Driver");
                        is_software = true;
                    }
                    if (adapter_flag & DXGI_ADAPTER_FLAG_SOFTWARE) != DXGI_ADAPTER_FLAG_NONE {
                        // log::debug!("Skip Software Adapter");
                        is_software = true;
                    }

                    let is_hardware = !is_software;

                    // Print some info about the adapter.
                    // log::debug!(
                    //     "===> DXGI Adapter: [{}], LUID=[{:?}] -> VendorId[{:x}] DeviceId[{:x}] with {} memory",
                    //     adapter_name,
                    //     luid,
                    //     vendor_id,
                    //     device_id,
                    //     adapter_dedicated_video_memory
                    // );

                    let adapter_desc = AdapterDesc {
                        index: i,
                        description: adapter_name.to_string(),
                        luid: luid.into(),
                        device_id,
                        vendor_id,
                        is_default,
                        is_software,
                        is_hardware,
                        is_discrete,
                        is_integrated,
                    };

                    // log::debug!(
                    //     "===> DXGI {} with {} memory",
                    //     adapter_desc,
                    //     adapter_dedicated_video_memory
                    // );

                    adapters_desc.push(adapter_desc);
                }
                Err(e) => {
                    log::debug!("adapters1 GetDesc1 fail: {:?}", e);
                    return None;
                }
            }
        };
        i += 1;
    }
    if !adapters_desc.is_empty() {
        return Some(adapters_desc);
    }
    None
}

pub fn get_hardware_adapter_desc(adapter: &IDXGIAdapter1) -> Option<AdapterDesc> {
    unsafe {
        match adapter.GetDesc1() {
            Ok(adapter_desc) => {
                let adapter_flag = DXGI_ADAPTER_FLAG(adapter_desc.Flags as _);

                let adapter_name = convert_u16_to_string(adapter_desc.Description.as_ref());
                // let _adapter_dedicated_video_memory = adapter_desc.DedicatedVideoMemory;
                let luid = adapter_desc.AdapterLuid;
                // let high_part = adapter_desc.AdapterLuid.HighPart;
                // let low_part = adapter_desc.AdapterLuid.LowPart;
                let vendor_id = adapter_desc.VendorId;
                let device_id = adapter_desc.DeviceId;

                let is_integrated =
                    vendor_id == 0x8086 && (device_id == 0x163c || device_id == 0x1616);
                let is_discrete = !is_integrated;
                let is_default = false;
                let mut is_software = false;

                // desc.VendorId == 0x10DE  NVIDIA
                // desc.VendorId == 0x1002 || 0x1022  AMD
                // desc.VendorId == 0x8086) 0x163C, 0x8087 Intel
                // Skip the software adaptor.
                if (vendor_id == 0x1414) && (device_id == 0x8c) {
                    // log::debug!("is Microsoft Basic Render Driver");
                    is_software = true;
                }
                if (adapter_flag & DXGI_ADAPTER_FLAG_SOFTWARE) != DXGI_ADAPTER_FLAG_NONE {
                    // log::debug!("Skip Software Adapter");
                    is_software = true;
                }

                let is_hardware = !is_software;

                let adapter_desc = AdapterDesc {
                    index: 0,
                    description: adapter_name.to_string(),
                    luid: luid.into(),
                    device_id,
                    vendor_id,
                    is_default,
                    is_software,
                    is_hardware,
                    is_discrete,
                    is_integrated,
                };

                Some(adapter_desc)
            }
            Err(e) => {
                log::debug!("adapters1 GetDesc1 fail: {:?}", e);
                return None;
            }
        }
    }
}

pub(crate) fn init_adaptor() -> Option<(IDXGIAdapter1, ID3D11Device, ID3D11DeviceContext)> {
    let factory = unsafe {
        match CreateDXGIFactory2::<IDXGIFactory7>(DXGI_CREATE_FACTORY_FLAGS(0u32)) {
            Ok(factory) => factory,
            Err(e) => {
                log::debug!("factory2 init fail: {:?}", e);
                return None;
            }
        }
    };

    // --- Enumerate adapters
    let mut adapters = Vec::new();
    unsafe {
        let mut i = 0;
        while let Ok(adapter) = factory.EnumAdapters1(i) {
            adapters.push(adapter);
            i += 1;
        }
    };

    for adapter in adapters {
        unsafe {
            match adapter.GetDesc1() {
                Ok(adapter_desc) => {
                    let adapter_flag = DXGI_ADAPTER_FLAG(adapter_desc.Flags as _);

                    let adapter_name = convert_u16_to_string(adapter_desc.Description.as_ref());
                    let adapter_dedicated_video_memory = adapter_desc.DedicatedVideoMemory;
                    let high_part = adapter_desc.AdapterLuid.HighPart;
                    let low_part = adapter_desc.AdapterLuid.LowPart;
                    let vendor_id = adapter_desc.VendorId;
                    let device_id = adapter_desc.DeviceId;

                    // desc.VendorId == 0x10DE  NVIDIA
                    // desc.VendorId == 0x1002 || 0x1022  AMD
                    // desc.VendorId == 0x8086) 0x163C, 0x8087 Intel
                    // Skip the software adaptor.
                    if (vendor_id == 0x1414) && (device_id == 0x8c) {
                        log::debug!("Skip Microsoft Basic Render Driver");
                        continue;
                    }
                    if (adapter_flag & DXGI_ADAPTER_FLAG_SOFTWARE) != DXGI_ADAPTER_FLAG_NONE {
                        log::debug!("Skip Software Adapter");
                        continue;
                    }

                    // Print some info about the adapter.
                    log::debug!(
                        "===> DXGI Adapter: [{}], LUID=[{:08x}-{:08x}] -> VendorId[{:x}] DeviceId[{:x}] with {} memory",
                        adapter_name,
                        high_part,
                        low_part,
                        vendor_id,
                        device_id,
                        adapter_dedicated_video_memory
                    );
                }
                Err(e) => {
                    log::debug!("adapters1 GetDesc1 fail: {:?}", e);
                    return None;
                }
            }
        };

        let mut level_used = D3D_FEATURE_LEVEL_11_0;
        let feature_levels = [D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0];
        let device_types = D3D_DRIVER_TYPE_UNKNOWN;

        let mut device_flags = D3D11_CREATE_DEVICE_FLAG::default();
        device_flags |= D3D11_CREATE_DEVICE_BGRA_SUPPORT;

        let mut d3d11_device: Option<ID3D11Device> = None;
        let mut d3d11_device_ctx: Option<ID3D11DeviceContext> = None;

        match unsafe {
            D3D11CreateDevice(
                &adapter,
                device_types,
                None,
                device_flags,
                Some(&feature_levels),
                D3D11_SDK_VERSION,
                Some(&mut d3d11_device),
                Some(&mut level_used),
                Some(&mut d3d11_device_ctx),
            )
        } {
            Ok(_) => {
                log::debug!("D3D11 device ok for {:?} ", device_types);
                log::debug!(
                    "D3D11 {:?} {:?} {:?}",
                    d3d11_device,
                    level_used,
                    d3d11_device_ctx
                );
                let d3d11_device = d3d11_device.unwrap();
                let d3d11_device_ctx = d3d11_device_ctx.unwrap();
                return Some((adapter, d3d11_device, d3d11_device_ctx));
            }
            Err(err) => {
                log::debug!("D3D11 device fail: {:?}", err);
            }
        }
    }
    None
}

pub(crate) fn init_adaptor_by_luid(
    luid: LUID,
) -> Option<(IDXGIAdapter1, ID3D11Device, ID3D11DeviceContext)> {
    let factory = unsafe {
        match CreateDXGIFactory2::<IDXGIFactory7>(DXGI_CREATE_FACTORY_FLAGS(0u32)) {
            Ok(factory) => factory,
            Err(e) => {
                log::debug!("factory2 init fail: {:?}", e);
                return None;
            }
        }
    };

    log::debug!("init_adaptor_by_luid {:?}", luid);

    unsafe {
        match factory.EnumAdapterByLuid::<IDXGIFactory4>(luid) {
            Ok(adapter) => {
                let adapter1 = adapter.cast::<IDXGIAdapter1>().unwrap();
                let mut level_used = D3D_FEATURE_LEVEL_11_0;
                let feature_levels = [D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0];
                let device_types = D3D_DRIVER_TYPE_UNKNOWN;

                let mut device_flags = D3D11_CREATE_DEVICE_FLAG::default();
                device_flags |= D3D11_CREATE_DEVICE_BGRA_SUPPORT;

                let mut d3d11_device: Option<ID3D11Device> = None;
                let mut d3d11_device_ctx: Option<ID3D11DeviceContext> = None;

                match D3D11CreateDevice(
                    &adapter1,
                    device_types,
                    None,
                    device_flags,
                    Some(&feature_levels),
                    D3D11_SDK_VERSION,
                    Some(&mut d3d11_device),
                    Some(&mut level_used),
                    Some(&mut d3d11_device_ctx),
                ) {
                    Ok(_) => {
                        log::debug!("D3D11 device ok for {:?} ", device_types);
                        log::debug!(
                            "D3D11 {:?} {:?} {:?}",
                            d3d11_device,
                            level_used,
                            d3d11_device_ctx
                        );
                        let d3d11_device = d3d11_device.unwrap();
                        let d3d11_device_ctx = d3d11_device_ctx.unwrap();
                        return Some((adapter1, d3d11_device, d3d11_device_ctx));
                    }
                    Err(err) => {
                        log::debug!("D3D11 device fail: {:?}", err);
                    }
                }
            }
            Err(e) => {
                log::debug!("adapters1 GetDesc1 fail: {:?}", e);
                return None;
            }
        }
    }
    None
}

pub(crate) fn acquire_duplication(
    adapter: &IDXGIAdapter1,
    d3d11_device: &ID3D11Device,
    capture_monitor_index: u32,
) -> Option<OutputDuplication> {
    let index = capture_monitor_index + 1;
    for output_index in 0..index {
        match unsafe { adapter.EnumOutputs(output_index) } {
            Ok(output) => {
                let output = match output.cast::<IDXGIOutput6>() {
                    Ok(output1) => output1,
                    Err(e) => {
                        log::error!("Failed to IDXGIOutput1 cast: {:?}", e);
                        return None;
                    }
                };
                match unsafe { output.GetDesc() } {
                    Ok(output_desc) => {
                        let _name = convert_u16_to_string(output_desc.DeviceName.as_ref());
                        let _monitor = output_desc.Monitor;
                        let _attached_to_desktop = output_desc.AttachedToDesktop == TRUE;

                        let width = output_desc.DesktopCoordinates.right
                            - output_desc.DesktopCoordinates.left;
                        let height = output_desc.DesktopCoordinates.bottom
                            - output_desc.DesktopCoordinates.top;
                        match unsafe { output.DuplicateOutput(d3d11_device) } {
                            Ok(duplicator) => {
                                // let duplicator_desc =  unsafe { duplicator.GetDesc() };
                                let adapter_desc = get_hardware_adapter_desc(adapter)?;
                                return Some(OutputDuplication {
                                    duplication: duplicator,
                                    output_dimensions: (width as _, height as _),
                                    adapter_desc,
                                });
                            }
                            Err(e) => {
                                log::error!(
                                    "Failed to duplicate output[{}]: {:?}",
                                    output_index,
                                    e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to get output[{}] desc: {:?}", output_index, e);
                    }
                }
            }
            Err(err) => {
                log::error!("Failed to get output[{}]: {:?}", output_index, err);
            }
        }
    }
    None
}
