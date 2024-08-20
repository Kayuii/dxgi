use windows;

use windows::Win32::Foundation::{LUID, S_FALSE};
use windows::{core::Result, core::*, Win32::Graphics::Direct3D11::*, Win32::Graphics::Dxgi::*};

use crate::staging_texture::StagingTexture;
use crate::OtherFrame;

use crate::utils::{acquire_duplication, init_adaptor, init_adaptor_by_luid};
use crate::OutputDuplication;


pub struct CaptureDXGI {
    device: ID3D11Device,
    device_context: ID3D11DeviceContext,
    duplicator: Option<OutputDuplication>,
    staging_texture: Option<StagingTexture>,
    capture_monitor_index: u32,
    width: u32,
    height: u32,
}

impl Drop for CaptureDXGI {
    fn drop(&mut self) {}
}

impl CaptureDXGI {
    pub fn new(capture_monitor_index: u32) -> Option<CaptureDXGI> {
        match init_adaptor() {
            Some((a, d, ctx)) => {
                match acquire_duplication(&a, &d, capture_monitor_index) {
                    Some(duplication) => {
                        return Some(Self {
                            duplicator: Some(duplication),
                            device: d,
                            device_context: ctx,
                            staging_texture: None,
                            // resources,
                            capture_monitor_index,
                            width: 0,
                            height: 0,
                        });
                    }
                    None => {
                        log::debug!("acquire_duplication is None.");
                        None
                    }
                }
            }
            None => {
                log::debug!("Should have an adaptor and d3d11 device now.");
                None
            }
        }
    }

    pub fn new_by_luid(luid: LUID, capture_monitor_index: u32) -> Option<CaptureDXGI> {
        match init_adaptor_by_luid(luid) {
            Some((a, d, ctx)) => {
                match acquire_duplication(&a, &d, capture_monitor_index) {
                    Some(duplication) => {
                        return Some(Self {
                            duplicator: Some(duplication),
                            device: d,
                            device_context: ctx,
                            staging_texture: None,
                            capture_monitor_index,
                            width: 0,
                            height: 0,
                        });
                    }
                    None => {
                        log::debug!("acquire_duplication is None.");
                        None
                    }
                }
            }
            None => {
                log::debug!("Should have an adaptor and d3d11 device now.");
                None
            }
        }
    }

    fn capture_to_texture(&self, _timeout: u32, skip: bool) -> Result<ID3D11Texture2D> {
        let duplication = &self.duplicator.as_ref().unwrap().duplication;
        let mut pp_desktop_resource = None;
        unsafe {
            let timeout_in_ms: u32 = 3;
            let mut frame_info: DXGI_OUTDUPL_FRAME_INFO = Default::default();
            duplication.AcquireNextFrame(
                timeout_in_ms,
                &mut frame_info,
                &mut pp_desktop_resource,
            )?;

            if skip && frame_info.LastPresentTime == 0 {
                let res = duplication.ReleaseFrame();
                if let Err(e) = res {
                    // log::error!("Could not release frame: {:?}", e);
                    return Err(e);
                }
                return Err(Error::new(
                    DXGI_ERROR_WAIT_TIMEOUT,
                    "acquire_duplication is None.",
                ));
            }
            Ok(pp_desktop_resource.unwrap().cast().unwrap())
        }
    }

    pub fn capture_next(&mut self, timeout: u32, skip: bool) -> Result<bool> {
        if self.duplicator.is_none() {
            if let Some((dupl, d3d11_device, ctx)) = match init_adaptor() {
                Some((a, d, ctx)) => {
                    match acquire_duplication(&a, &d, self.capture_monitor_index) {
                        Some(duplication) => Some((Some(duplication), d, ctx)),
                        None => {
                            // log::debug!("acquire_duplication is None.");
                            return Err(Error::new(S_FALSE, "acquire_duplication is None."));
                        }
                    }
                }
                None => {
                    // log::debug!("init_adaptor failed.");
                    return self.capture_next(timeout, skip);
                }
            } {
                self.duplicator = dupl;
                self.device = d3d11_device;
                self.device_context = ctx;
                self.staging_texture = None;
            };
        }

        // #[cfg(debug_assertions)]
        // let now = std::time::Instant::now();
        // Now, we can acquire the next frame.
        match self.capture_to_texture(timeout, skip) {
            Ok(texture) => {
                let (width, height) = {
                    let (full_w, full_h) = self.duplicator.as_ref().unwrap().output_dimensions;
                    (full_w, full_h)
                };
                self.width = width.clone();
                self.height = height.clone();

                let mut tex_desc: D3D11_TEXTURE2D_DESC = Default::default();
                unsafe { texture.GetDesc(&mut tex_desc) };

                // Here, we create an texture that will be mapped.
                if self.staging_texture.is_none()
                    || width != tex_desc.Width
                    || height != tex_desc.Height
                {
                    let new_staging_texture = StagingTexture::new(
                        &self.device,
                        tex_desc.Width,
                        tex_desc.Height,
                        tex_desc.Format,
                    )?;
                    self.staging_texture = Some(new_staging_texture);
                }

                unsafe {
                    let copy_dest = self.staging_texture.as_ref().unwrap().as_resource()?;
                    let copy_src = texture.cast()?;
                    self.device_context.CopySubresourceRegion(
                        Some(&copy_dest),
                        0,
                        0,
                        0,
                        0,
                        Some(&copy_src),
                        0,
                        None,
                    );

                    let res = self.duplicator.as_ref().unwrap().duplication.ReleaseFrame();
                    if let Err(e) = res {
                        log::error!("Could not release frame: {:?}", e);
                        return Err(e);
                    }
                }

                // #[cfg(debug_assertions)]
                // let elapsed = now.elapsed();
                // #[cfg(debug_assertions)]
                // log::debug!("===>0< capture: elapsed {:?} ", elapsed);
                return Ok(true);
            }
            Err(hr) => {
                match hr.code() {
                    DXGI_ERROR_ACCESS_LOST
                    | DXGI_ERROR_DEVICE_REMOVED
                    | DXGI_ERROR_INVALID_CALL => {
                        log::debug!("Reacquiring duplication: {:?}", hr);
                        self.duplicator = None;
                        return self.capture_next(timeout, skip);
                    }
                    DXGI_ERROR_WAIT_TIMEOUT => {
                        // Timeout may happen if no changes occured from the last frame.
                        // This means it is perfectly ok to return the current image.
                        if self.staging_texture.is_some() {
                            //likely no draw events since last frame, return ok since we have a frame to show.
                            return Ok(!skip);
                        }
                        log::debug!("===> DXGI_ERROR_WAIT_TIMEOUT: {:?}", hr);
                        Err(hr)
                    }
                    _ => {
                        log::error!("Unhandled error!: {:?}", hr);
                        let res =
                            unsafe { self.duplicator.as_ref().unwrap().duplication.ReleaseFrame() };
                        if let Err(e) = res {
                            log::error!("Could not release frame: {:?}", e);
                            return Err(e);
                        }
                        Err(hr)
                    }
                }
            }
        }
    }

    pub fn capture(&mut self, timeout: u32, skip: bool) -> Result<Option<OtherFrame>> {
        // let now = std::time::Instant::now();
        if self.capture_next(timeout, skip)? {
            // let elapsed = now.elapsed();
            // log::debug!("===>1< capture: elapsed {:?} ", elapsed);
            let texture = self.staging_texture.as_ref().unwrap();
            let ptr = self
                .staging_texture
                .as_ref()
                .unwrap()
                .as_mapped(&self.device_context)?;

            // let elapsed = now.elapsed();
            // log::debug!("===>2< capture: elapsed {:?} ", elapsed);
            Ok(Some(OtherFrame { texture, ptr }))
        } else {
            Ok(None)
        }
    }

    pub fn get_device(&self) -> *mut std::ffi::c_void {
        self.device.as_raw()
    }

    pub fn width(&self) -> i32 {
        self.width as _
    }
    pub fn height(&self) -> i32 {
        self.height as _
    }
}
