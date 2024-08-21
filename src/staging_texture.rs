use std::ffi::c_void;

use windows::{
    core::{Interface, Result},
    Win32::
        Graphics::{
            Direct3D::D3D11_SRV_DIMENSION_TEXTURE2D,
            Direct3D11::{
                ID3D11Device, ID3D11DeviceContext, ID3D11Resource, ID3D11ShaderResourceView,
                ID3D11Texture2D, D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE,
                D3D11_CPU_ACCESS_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ,
                D3D11_RESOURCE_MISC_GENERATE_MIPS, D3D11_SHADER_RESOURCE_VIEW_DESC,
                D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING,
            },
            Dxgi::
                Common::DXGI_FORMAT
            ,
        }
    ,
};

#[derive(Clone, Debug)]
pub struct StagingTexture {
    pub frame_buffer: Option<ID3D11Texture2D>,
    pub frame_buffer_view: Option<ID3D11ShaderResourceView>,
    pub mapping_buffer: Option<ID3D11Texture2D>,
    pub mip_level: u32,
}

impl StagingTexture {
    pub fn new(
        device: &ID3D11Device,
        width: u32,
        height: u32,
        format: DXGI_FORMAT,
        mip_level: u32,
    ) -> Result<Self> {
        let (frame_buffer, frame_buffer_view) = unsafe {
            let mut tex_desc: D3D11_TEXTURE2D_DESC = Default::default();
            tex_desc.Width = width;
            tex_desc.Height = height;
            tex_desc.MipLevels = mip_level + 1;
            tex_desc.ArraySize = 1;
            tex_desc.Format = format;
            tex_desc.SampleDesc.Count = 1;
            tex_desc.Usage = D3D11_USAGE_DEFAULT;
            tex_desc.BindFlags = (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32;
            tex_desc.MiscFlags = D3D11_RESOURCE_MISC_GENERATE_MIPS.0 as u32;
            let mut buffer = None;
            device
                .CreateTexture2D(&tex_desc, None, Some(&mut buffer))
                .expect("Could not create frame texture");

            let texture = buffer.as_ref().unwrap();
            let mut res_desc: D3D11_SHADER_RESOURCE_VIEW_DESC = Default::default();
            res_desc.Format = tex_desc.Format;
            res_desc.ViewDimension = D3D11_SRV_DIMENSION_TEXTURE2D;
            res_desc.Anonymous.Texture2D.MipLevels = u32::max_value();
            res_desc.Anonymous.Texture2D.MostDetailedMip = 0;
            let mut buffer_view = None;
            device
                .CreateShaderResourceView(texture, Some(&res_desc), Some(&mut buffer_view))
                .expect("Could not create resource view");
            (buffer, buffer_view)
        };

        let mapping_buffer = unsafe {
            let mut tex_desc: D3D11_TEXTURE2D_DESC = std::mem::zeroed();
            tex_desc.Width = width / (1 << mip_level);
            tex_desc.Height = height / (1 << mip_level);
            tex_desc.MipLevels = 1;
            tex_desc.ArraySize = 1;
            tex_desc.Format = format;
            tex_desc.SampleDesc.Count = 1;
            tex_desc.Usage = D3D11_USAGE_STAGING;
            tex_desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            let mut mapping_buffer = None;
            device
                .CreateTexture2D(&tex_desc, None, Some(&mut mapping_buffer))
                .expect("Could not create mapping texture");
            mapping_buffer
        };

        Ok(Self {
            frame_buffer,
            frame_buffer_view,
            mapping_buffer,
            mip_level,
        })
    }

    pub fn as_cpu_resource(&self) -> Result<ID3D11Resource> {
        self.mapping_buffer.as_ref().unwrap().cast()
    }

    pub fn as_gpu_resource(&self) -> Result<ID3D11Resource> {
        self.frame_buffer.as_ref().unwrap().cast()
    }

    pub fn as_view_resource(&self) -> Result<ID3D11ShaderResourceView> {
        self.frame_buffer_view.as_ref().unwrap().cast()
    }

    pub fn as_raw(&self) -> Result<*mut c_void> {
        Ok(self.frame_buffer.as_ref().unwrap().as_raw())
    }

    pub fn as_mapped(&self, context: &ID3D11DeviceContext) -> Result<D3D11_MAPPED_SUBRESOURCE> {
        let staging_texture_ptr: ID3D11Resource = self.mapping_buffer.as_ref().unwrap().cast()?;
        let mut mapped_texture: D3D11_MAPPED_SUBRESOURCE = Default::default();
        unsafe {
            context.Map(
                Some(&staging_texture_ptr),
                0,
                D3D11_MAP_READ,
                0,
                Some(&mut mapped_texture),
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

}
