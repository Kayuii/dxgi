use std::{io::{self, Write as _}, path::PathBuf, time::{Duration, Instant}};

use dxgi::{utils::get_hardware_adapter_desc, CaptureDXGI};
use env_logger::{init_from_env, Env, DEFAULT_FILTER_ENV};
use log;
use windows::Win32::{Foundation::{E_ACCESSDENIED, E_INVALIDARG, E_UNEXPECTED, S_FALSE}, Graphics::Dxgi::{DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_INVALID_CALL, DXGI_ERROR_MODE_CHANGE_IN_PROGRESS, DXGI_ERROR_NOT_CURRENTLY_AVAILABLE, DXGI_ERROR_NOT_FOUND, DXGI_ERROR_SESSION_DISCONNECTED, DXGI_ERROR_UNSUPPORTED, DXGI_ERROR_WAIT_TIMEOUT}};
use hwcodec::{common::{DataFormat, Driver, API::API_DX11}, vram::{decode::Decoder, encode::Encoder, DecodeContext, DynamicContext, EncodeContext, FeatureContext}};


fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_from_env(Env::default().filter_or(DEFAULT_FILTER_ENV, "debug"));
    
    let adapter = get_hardware_adapter_desc()
        .expect("Failed to get hardware adapters")
        .into_iter()
        .find(|adapter| adapter.is_hardware); 

    if adapter.is_none() {
        log::error!("No hardware adapters found");
        return Ok(());
    }
    let adapter = adapter.unwrap();
    log::info!("Using adapter: {:#?}", adapter);

    let mut capture = CaptureDXGI::new_by_luid(adapter.luid, 1).unwrap();

    let data_format = DataFormat::H265;
    let luid = ((adapter.luid.HighPart as i64) << 32)
            | adapter.luid.LowPart as i64;
    let en_ctx = EncodeContext {
        f: FeatureContext {
            driver: Driver::FFMPEG,
            api: API_DX11,
            data_format,
            luid,
        },
        d: DynamicContext {
            device: Some(capture.get_device()),
            width: capture.width(),
            height: capture.height(),
            kbitrate: 5000,
            framerate: 30,
            gop: 5,
        },
    };
    let de_ctx = DecodeContext {
        device: Some(capture.get_device()),
        driver: Driver::FFMPEG,
        api: API_DX11,
        data_format,
        luid,
    };
    let mut dec = Decoder::new(de_ctx).unwrap();
    let mut enc = Encoder::new(en_ctx).unwrap();
    let filename = PathBuf::from("output/1.265");

    let mut file = std::fs::File::create(filename).unwrap();
    let mut dup_sum = Duration::ZERO;
    let mut enc_sum = Duration::ZERO;
    let mut dec_sum = Duration::ZERO;


    loop {
        let start = Instant::now();
        let texture = match capture.capture(16, false) {
            Ok(Some(pixel_data)) => {
                Ok(pixel_data.texture.as_raw()?)
            },
            Ok(None) => {
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("dxgi_capturer [NoChange]"),
                ))
            },
            Err(_err) => {
                match _err.code() {
                    //  灾难性故障 HRESULT(0x8000FFFF)
                    E_UNEXPECTED |
                    //  拒绝访问 HRESULT(0x80070005)
                    E_ACCESSDENIED |
                    // 正在进行的模式更改阻止了调用的完成。如果稍后尝试，调用可能会成功 HRESULT(0x887A0025)
                    DXGI_ERROR_MODE_CHANGE_IN_PROGRESS |
                    DXGI_ERROR_ACCESS_LOST | DXGI_ERROR_WAIT_TIMEOUT
                    | DXGI_ERROR_INVALID_CALL
                    | DXGI_ERROR_NOT_FOUND
                    | DXGI_ERROR_NOT_CURRENTLY_AVAILABLE
                    | DXGI_ERROR_UNSUPPORTED
                    | DXGI_ERROR_SESSION_DISCONNECTED
                    | E_INVALIDARG 
                    | S_FALSE => {
                        // log::debug!("===> get_pixelbuffer err[{}] {}",
                        //     hr.0,
                        //     hr.message(),
                        // );
                        Err(io::Error::new(
                            io::ErrorKind::Other,
                            format!("dxgi_capturer err[{:?}]", _err),
                        ))
                    }
                    hr => {
                        log::debug!("===> get_pixelbuffer err[{}] {}",
                            hr.0,
                            hr.message(),
                        );
                        Err(io::Error::new(
                            io::ErrorKind::Other,
                            format!("dxgi_capturer uninit {:?}", _err),
                        ))
                    }
                }
            }
        };


        if texture.is_err() {
            continue;
        }
        let texture = texture.unwrap();

        dup_sum += start.elapsed();
        let start = Instant::now();
        let frame = enc.encode(texture, 5).unwrap();
        enc_sum += start.elapsed();
        for f in frame {
            file.write_all(&mut f.data).unwrap();
            let start = Instant::now();
            let frames = dec.decode(&f.data).unwrap();
            dec_sum += start.elapsed();
            // for f in frames {
            //     render.render(f.texture).unwrap();
            // }
        }
    }



    Ok(())
}
