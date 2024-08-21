mod dxgi1_2;

use std::{io::{self, Write as _}, path::PathBuf, time::{Duration, Instant}};

use dxgi1_2::Capturer;
use env_logger::{init_from_env, Env, DEFAULT_FILTER_ENV};
use log;
use hwcodec::{common::{DataFormat, Driver, API::API_DX11}, vram::{decode::Decoder, encode::Encoder, DecodeContext, DynamicContext, EncodeContext, FeatureContext}};


fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_from_env(Env::default().filter_or(DEFAULT_FILTER_ENV, "debug"));

    let mut capture = Capturer::new(0).unwrap();
    let data_format = DataFormat::H265;
    let luid = capture.get_luid();
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

    capture.set_output_texture(true);
    loop {
        let start = Instant::now();
        let texture = match capture.frame(16) {
            Ok(pixel_data) => {
                Ok(pixel_data.to()?)
            },
            Err(_err) => {
                Err(anyhow::anyhow!("dxgi_capturer [Error]{:?}",_err))
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
            let _frames = dec.decode(&f.data).unwrap();
            dec_sum += start.elapsed();
            // for f in frames {
            //     render.render(f.texture).unwrap();
            // }
        }
        log::info!("dup: {:?}, enc: {:?}, dec: {:?}", dup_sum, enc_sum, dec_sum);
        dup_sum = Duration::ZERO;
        enc_sum = Duration::ZERO;
        dec_sum = Duration::ZERO;
    }
    Ok(())
}
