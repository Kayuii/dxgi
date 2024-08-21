
use winapi::{
    shared::{
        dxgi::*,
        dxgi1_2::*,
        dxgitype::*,
        minwindef::{DWORD, FALSE, TRUE, UINT},
        ntdef::LONG,
        windef::HMONITOR,
        winerror::*,
        // dxgiformat::{DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_420_OPAQUE},
    },
    um::{
        d3d11::*, d3dcommon::D3D_DRIVER_TYPE_UNKNOWN, unknwnbase::IUnknown, wingdi::*,
        winnt::HRESULT, winuser::*,
    },
};

use anyhow::anyhow;

// use crate::RotationMode::*;

// use crate::{AdapterDevice, Frame, PixelBuffer};
use std::{ffi::c_void, io, mem, ptr, slice};

use dxgi::{utils::convert_u16_to_string, AdapterDesc, Luid};

pub struct ComPtr<T>(*mut T);
impl<T> ComPtr<T> {
    fn is_null(&self) -> bool {
        self.0.is_null()
    }
}
impl<T> Drop for ComPtr<T> {
    fn drop(&mut self) {
        unsafe {
            if !self.is_null() {
                (*(self.0 as *mut IUnknown)).Release();
            }
        }
    }
}



pub struct Capturer {
    device: ComPtr<ID3D11Device>,
    context: ComPtr<ID3D11DeviceContext>,
    duplication: ComPtr<IDXGIOutputDuplication>,
    fastlane: bool,
    surface: ComPtr<IDXGISurface>,
    texture: ComPtr<ID3D11Texture2D>,
    width: usize,
    height: usize,
    output_texture: bool,
    adapter_desc1: DXGI_ADAPTER_DESC1,
    luid: i64,
}

impl Capturer {
    pub fn new(display: u32) -> anyhow::Result<Capturer> {
        let mut device = ptr::null_mut();
        let mut context = ptr::null_mut();
        let mut duplication = ptr::null_mut();
        let mut output_desc = unsafe { mem::MaybeUninit::uninit().assume_init() };
        let mut duplicator_desc = unsafe { mem::MaybeUninit::uninit().assume_init() };
        
        let mut adapter_desc1 = unsafe { mem::MaybeUninit::uninit().assume_init() };
        let mut adapter_desc = AdapterDesc::default();

        
        let mut factory = ptr::null_mut();
        wrap_hresult(unsafe { CreateDXGIFactory1(&IID_IDXGIFactory1, &mut factory) })?;
        let factory = factory as *mut IDXGIFactory1;
        let mut adapter = ptr::null_mut();
        unsafe {
            // On error, our adapter is null, so it's fine.
            (*factory).EnumAdapters1(0, &mut adapter);
        };

        if adapter.is_null() {
            return Err(anyhow!("No adapter found"));
        }

        let mut res = wrap_hresult(unsafe {
            D3D11CreateDevice(
                adapter as *mut _,
                D3D_DRIVER_TYPE_UNKNOWN,
                ptr::null_mut(), // No software rasterizer.
                0,               // No device flags.
                ptr::null_mut(), // Feature levels.
                0,               // Feature levels' length.
                D3D11_SDK_VERSION,
                &mut device,
                ptr::null_mut(),
                &mut context,
            )
        });

        let device = ComPtr(device);
        let context = ComPtr(context);

        if res.is_err() {
            return Err(anyhow!("Failed to create D3D11 device"));
        } else {
            
            wrap_hresult(unsafe { (*adapter).GetDesc1(&mut adapter_desc1) });

            adapter_desc = {
                let adapter_flag = adapter_desc1.Flags;

                let adapter_name = convert_u16_to_string(adapter_desc1.Description.as_ref());
                // let _adapter_dedicated_video_memory = adapter_desc.DedicatedVideoMemory;
                let luid = adapter_desc1.AdapterLuid;
                // let high_part = adapter_desc.AdapterLuid.HighPart;
                // let low_part = adapter_desc.AdapterLuid.LowPart;
                let vendor_id = adapter_desc1.VendorId;
                let device_id = adapter_desc1.DeviceId;

                let is_integrated = vendor_id == 0x8086 && (device_id == 0x163c || device_id == 0x1616);
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
                let luid = luid.HighPart as i64 | ((luid.LowPart as i64) << 32);

                let adapter_desc = AdapterDesc {
                    index: 0,
                    description: adapter_name.to_string(),
                    luid: Luid(luid),
                    device_id,
                    vendor_id,
                    is_default,
                    is_software,
                    is_hardware,
                    is_discrete,
                    is_integrated,
                };

                adapter_desc
            };



            let output = unsafe {
                let mut output = ptr::null_mut();
                (*adapter).EnumOutputs(0, &mut output);
                ComPtr(output)
            };

            if output.is_null() {
                return Err(anyhow!("No output found"));
            }

            wrap_hresult(unsafe { (*output.0).GetDesc(&mut output_desc) });


            let output = unsafe {
                let mut inner: *mut IDXGIOutput1 = ptr::null_mut();
                (*output.0).QueryInterface(&IID_IDXGIOutput1, &mut inner as *mut *mut _ as *mut *mut _);
                inner
            };

            res = wrap_hresult(unsafe {
                let hres = (*output).DuplicateOutput(device.0 as *mut _, &mut duplication);
                hres
            });
        }

        res?;

        if duplication.is_null() {
            return Err(anyhow!("Failed to create duplication"));
        }
        
        unsafe { (*duplication).GetDesc(&mut duplicator_desc) };

        let width = output_desc.DesktopCoordinates.right - output_desc.DesktopCoordinates.left;
        let height = output_desc.DesktopCoordinates.bottom - output_desc.DesktopCoordinates.top;
        let luid = *adapter_desc.luid;

        Ok(Capturer {
            device,
            context,
            duplication: ComPtr(duplication),
            fastlane: duplicator_desc.DesktopImageInSystemMemory == TRUE,
            surface: ComPtr(ptr::null_mut()),
            texture: ComPtr(ptr::null_mut()),
            width: width as usize,
            height: height as usize,
            output_texture: false,
            adapter_desc1,
            luid,
        })
    }


    // #[cfg(feature = "vram")]
    pub fn set_output_texture(&mut self, texture: bool) {
        self.output_texture = texture;
    }

    unsafe fn load_frame(&mut self, timeout: UINT) -> io::Result<(*const u8, i32)> {
        let mut frame = ptr::null_mut();
        #[allow(invalid_value)]
        let mut info = mem::MaybeUninit::uninit().assume_init();

        wrap_hresult((*self.duplication.0).AcquireNextFrame(timeout, &mut info, &mut frame))?;
        let frame = ComPtr(frame);

        if *info.LastPresentTime.QuadPart() == 0 {
            return Err(std::io::ErrorKind::WouldBlock.into());
        }

        #[allow(invalid_value)]
        let mut rect = mem::MaybeUninit::uninit().assume_init();
        if self.fastlane {
            wrap_hresult((*self.duplication.0).MapDesktopSurface(&mut rect))?;
        } else {
            self.surface = ComPtr(self.ohgodwhat(frame.0)?);
            wrap_hresult((*self.surface.0).Map(&mut rect, DXGI_MAP_READ))?;
        }
        Ok((rect.pBits, rect.Pitch))
    }

    // copy from GPU memory to system memory
    unsafe fn ohgodwhat(&mut self, frame: *mut IDXGIResource) -> io::Result<*mut IDXGISurface> {
        let mut texture: *mut ID3D11Texture2D = ptr::null_mut();
        (*frame).QueryInterface(
            &IID_ID3D11Texture2D,
            &mut texture as *mut *mut _ as *mut *mut _,
        );
        let texture = ComPtr(texture);

        #[allow(invalid_value)]
        let mut texture_desc = mem::MaybeUninit::uninit().assume_init();
        (*texture.0).GetDesc(&mut texture_desc);

        texture_desc.Usage = D3D11_USAGE_STAGING;
        texture_desc.BindFlags = 0;
        texture_desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
        texture_desc.MiscFlags = 0;

        let mut readable = ptr::null_mut();
        wrap_hresult((*self.device.0).CreateTexture2D(
            &mut texture_desc,
            ptr::null(),
            &mut readable,
        ))?;
        (*readable).SetEvictionPriority(DXGI_RESOURCE_PRIORITY_MAXIMUM);
        let readable = ComPtr(readable);

        let mut surface = ptr::null_mut();
        (*readable.0).QueryInterface(
            &IID_IDXGISurface,
            &mut surface as *mut *mut _ as *mut *mut _,
        );

        (*self.context.0).CopyResource(readable.0 as *mut _, texture.0 as *mut _);

        Ok(surface)
    }

    pub fn frame<'a>(&'a mut self, timeout: UINT) -> io::Result<Frame<'a>> {
        if self.output_texture {
            Ok(Frame::Texture(self.get_texture(timeout)?))
        } else {
            let width = self.width;
            let height = self.height;
            Ok(Frame::PixelBuffer(PixelBuffer::new(
                self.get_pixelbuffer(timeout)?,
                width,
                height,
            )))
        }
    }

    fn get_pixelbuffer<'a>(&'a mut self, timeout: UINT) -> io::Result<&'a [u8]> {
        unsafe {
            let result = {
                self.unmap();
                let r = self.load_frame(timeout)?;
                slice::from_raw_parts(r.0, r.1 as usize * self.height)
            };
            Ok(result)
        }
    }

    fn get_texture(&mut self, timeout: UINT) -> io::Result<*mut c_void> {
        unsafe {
            if self.duplication.0.is_null() {
                return Err(std::io::ErrorKind::AddrNotAvailable.into());
            }
            (*self.duplication.0).ReleaseFrame();
            let mut frame = ptr::null_mut();
            #[allow(invalid_value)]
            let mut info = mem::MaybeUninit::uninit().assume_init();

            wrap_hresult((*self.duplication.0).AcquireNextFrame(timeout, &mut info, &mut frame))?;
            let frame = ComPtr(frame);

            if info.AccumulatedFrames == 0 || *info.LastPresentTime.QuadPart() == 0 {
                return Err(std::io::ErrorKind::WouldBlock.into());
            }

            let mut texture: *mut ID3D11Texture2D = ptr::null_mut();
            (*frame.0).QueryInterface(
                &IID_ID3D11Texture2D,
                &mut texture as *mut *mut _ as *mut *mut _,
            );

            let texture = ComPtr(texture);
            self.texture = texture;
            Ok(self.texture.0 as *mut c_void)
        }
    }

    fn unmap(&self) {
        unsafe {
            (*self.duplication.0).ReleaseFrame();
            if self.fastlane {
                (*self.duplication.0).UnMapDesktopSurface();
            } else {
                if !self.surface.is_null() {
                    (*self.surface.0).Unmap();
                }
            }
        }
    }


    pub fn get_device(&self) -> *mut std::ffi::c_void {
        self.device.0 as _
    }

    pub fn get_luid(&self) -> i64 {
        self.luid
    }

    pub fn width(&self) -> i32 {
        self.width as _
    }
    pub fn height(&self) -> i32 {
        self.height as _
    }


}

impl Drop for Capturer {
    fn drop(&mut self) {
        if !self.duplication.is_null() {
            self.unmap();
        }
    }
}


fn wrap_hresult(x: HRESULT) -> io::Result<()> {
    use std::io::ErrorKind::*;
    Err((match x {
        S_OK => return Ok(()),
        DXGI_ERROR_ACCESS_LOST => ConnectionReset,
        DXGI_ERROR_WAIT_TIMEOUT => TimedOut,
        DXGI_ERROR_INVALID_CALL => InvalidData,
        E_ACCESSDENIED => PermissionDenied,
        DXGI_ERROR_UNSUPPORTED => ConnectionRefused,
        DXGI_ERROR_NOT_CURRENTLY_AVAILABLE => Interrupted,
        DXGI_ERROR_SESSION_DISCONNECTED => ConnectionAborted,
        E_INVALIDARG => InvalidInput,
        _ => {
            // 0x8000ffff https://www.auslogics.com/en/articles/windows-10-update-error-0x8000ffff-fixed/
            return Err(io::Error::new(Other, format!("Error code: {:#X}", x)));
        }
    })
    .into())
}

#[cfg(not(any(target_os = "ios")))]
pub enum Frame<'a> {
    PixelBuffer(PixelBuffer<'a>),
    Texture(*mut c_void),
}

#[cfg(not(any(target_os = "ios")))]
impl Frame<'_> {
    pub fn valid<'a>(&'a self) -> bool {
        match self {
            Frame::PixelBuffer(pixelbuffer) => !pixelbuffer.data().is_empty(),
            Frame::Texture(texture) => !texture.is_null(),
        }
    }

    pub fn to<'a>(&'a self) -> anyhow::Result<*mut c_void> {
        match self {
            Frame::PixelBuffer(pixelbuffer) => {
                Err(anyhow!("PixelBuffer is not supported"))
            }
            Frame::Texture(texture) => Ok(*texture),
        }
    }


    // pub fn to<'a>(
    //     &'a self,
    //     yuvfmt: EncodeYuvFormat,
    //     yuv: &'a mut Vec<u8>,
    //     mid_data: &mut Vec<u8>,
    // ) -> ResultType<EncodeInput> {
    //     match self {
    //         Frame::PixelBuffer(pixelbuffer) => {
    //             convert_to_yuv(&pixelbuffer, yuvfmt, yuv, mid_data)?;
    //             Ok(EncodeInput::YUV(yuv))
    //         }
    //         Frame::Texture(texture) => Ok(EncodeInput::Texture(*texture)),
    //     }
    // }
}


pub struct PixelBuffer<'a> {
    data: &'a [u8],
    width: usize,
    height: usize,
    stride: Vec<usize>,
}

impl<'a> PixelBuffer<'a> {
    pub fn new(data: &'a [u8], width: usize, height: usize) -> Self {
        let stride0 = data.len() / height;
        let mut stride = Vec::new();
        stride.push(stride0);
        PixelBuffer {
            data,
            width,
            height,
            stride,
        }
    }
}

impl<'a> TraitPixelBuffer for PixelBuffer<'a> {
    fn data(&self) -> &[u8] {
        self.data
    }

    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

    fn stride(&self) -> Vec<usize> {
        self.stride.clone()
    }

    fn pixfmt(&self) -> Pixfmt {
        Pixfmt::BGRA
    }
}


pub trait TraitPixelBuffer {
    fn data(&self) -> &[u8];

    fn width(&self) -> usize;

    fn height(&self) -> usize;

    fn stride(&self) -> Vec<usize>;

    fn pixfmt(&self) -> Pixfmt;
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Pixfmt {
    BGRA,
    RGBA,
    RGB565LE,
    I420,
    NV12,
    I444,
}

impl Pixfmt {
    pub fn bpp(&self) -> usize {
        match self {
            Pixfmt::BGRA | Pixfmt::RGBA => 32,
            Pixfmt::RGB565LE => 16,
            Pixfmt::I420 | Pixfmt::NV12 => 12,
            Pixfmt::I444 => 24,
        }
    }

    pub fn bytes_per_pixel(&self) -> usize {
        (self.bpp() + 7) / 8
    }
}

fn main() {
}