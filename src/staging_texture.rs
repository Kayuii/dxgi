use std::ffi::c_void;

use windows::{
    core::{Error, Interface, Result},
    Win32::{Foundation::S_FALSE, Graphics::{
        Direct3D11::{
            ID3D11Device, ID3D11DeviceContext, ID3D11Resource, ID3D11Texture2D,
            D3D11_CPU_ACCESS_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ,
            D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
        },
        Dxgi::{Common::{DXGI_FORMAT, DXGI_SAMPLE_DESC}, DXGI_RESOURCE_PRIORITY_MAXIMUM},
    }},
};

#[derive(Clone, Debug)]
pub struct StagingTexture {
    pub texture: ID3D11Texture2D,
    pub desc: D3D11_TEXTURE2D_DESC,
}

impl StagingTexture {
    pub fn new(
        device: &ID3D11Device,
        width: u32,
        height: u32,
        format: DXGI_FORMAT,
    ) -> Result<Self> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            Format: format,
            MipLevels: 1,
            ArraySize: 1,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BindFlags: 0,
            MiscFlags: 0,
            Usage: D3D11_USAGE_STAGING,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        };

        let mut texture: Option<ID3D11Texture2D> = None;
        unsafe { device.CreateTexture2D(&desc, None, Some(&mut texture))? };

        match texture {
            Some(readable_texture) => {
                // Lower priorities causes stuff to be needlessly copied from gpu to ram,
                // causing huge ram usage on some systems.
                // https://github.com/bryal/dxgcap-rs/blob/208d93368bc64aed783791242410459c878a10fb/src/lib.rs#L225
                unsafe { readable_texture.SetEvictionPriority(DXGI_RESOURCE_PRIORITY_MAXIMUM) };
                Ok(Self { 
                    texture: readable_texture, 
                    desc,
                })
            },
            None => Err(
                Error::new(S_FALSE, "Failed to create staging texture")
            ),
        } 
    }

    pub fn as_resource(&self) -> Result<ID3D11Resource> {
        self.texture.cast()
    }

    pub fn as_raw(&self) -> Result<*mut c_void> {
        Ok(self.texture.as_raw())
    }

    pub fn as_mapped(&self, context: &ID3D11DeviceContext) -> Result<D3D11_MAPPED_SUBRESOURCE> {
        let staging_texture_ptr: ID3D11Resource = self.texture.cast()?;
        let mut mapped_texture: D3D11_MAPPED_SUBRESOURCE = Default::default();
        unsafe { 
            context.Map(
                Some(&staging_texture_ptr),
                0, 
                D3D11_MAP_READ, 
                0, 
                Some(&mut mapped_texture)
            )?;
        };
        // we can instantly unmap because the texture is staging, and will be still accessible by CPU
        // TODO there should be a way to do this by queueing a fence (we only need to wait copies) or something like that,
        // which would probably be more correct solution rather than map-unmap
        unsafe {
            context.Unmap(Some(&staging_texture_ptr), 0);
        };
        Ok(mapped_texture)
    }

    // pub fn as_bytes(&self) -> Result<Vec<u8>> {
    //     let mapped_texture = self.as_mapped(&self.texture)?;
    //     let bytes = mapped_texture.pData as *const u8;
    //     let bytes = unsafe { std::slice::from_raw_parts(bytes, mapped_texture.RowPitch as usize * self.desc.Height as usize) };
    //     Ok(bytes.to_vec())
    // }


    
}
