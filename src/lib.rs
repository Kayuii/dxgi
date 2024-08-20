use staging_texture::StagingTexture;
use windows::Win32::{Foundation::LUID, Graphics::{Direct3D11::D3D11_MAPPED_SUBRESOURCE, Dxgi::IDXGIOutputDuplication}};

pub mod d3d11;
pub mod utils;
pub mod staging_texture;

pub use d3d11::CaptureDXGI;

pub struct OtherFrame<'a> {
    pub texture: &'a StagingTexture,
    pub ptr: D3D11_MAPPED_SUBRESOURCE,
}

pub struct OutputDuplication {
    duplication: IDXGIOutputDuplication,
    output_dimensions: (u32, u32),
}


#[derive(Debug, Clone)]
pub struct AdapterDesc {
    // 适配器索引
    pub index: u32,
    // 适配器描述
    pub description: String,
    // 逻辑设备id
    pub luid: LUID,
    // 设备id
    pub device_id: u32,
    // 厂商id
    pub vendor_id: u32,
    // 是否是默认适配器
    pub is_default: bool,
    // 软件驱动
    pub is_software: bool,
    // 硬件驱动
    pub is_hardware: bool,
    // 独立显卡
    pub is_discrete: bool,
    // 集成显卡
    pub is_integrated: bool,
}

impl std::fmt::Display for AdapterDesc {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, " Adapter: [{}]", self.description)?;
        write!(f, " LUID=[{:?}]", self.luid)?;
        write!(f, " VendorId=[{:04x}]", self.vendor_id)?;
        write!(f, " DeviceId=[{:04x}]", self.device_id)?;
        Ok(())
    }
}
