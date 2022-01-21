#![warn(clippy::all)]

use std::{env, fs, mem::size_of, path::Path, process::Command};

use av_metrics_decoders::{
    CastFromPrimitive,
    ChromaSampling,
    Decoder,
    FrameInfo,
    Pixel,
    Y4MDecoder,
};
use clap::{App, Arg, ArgMatches, SubCommand};
use image::{ImageBuffer, Rgb, RgbImage};
use tempfile::Builder;
use yuv::{
    color::{Depth, MatrixCoefficients, Range},
    convert::RGBConvert,
    YUV,
};

fn main() {
    let args = App::new("butter-video")
        .about("Calculates butteraugli and ssimulacra metrics for videos")
        .subcommand(
            SubCommand::with_name("butter")
                .about("Calculate butteraugli")
                .arg(Arg::with_name("input1").required(true).index(1))
                .arg(Arg::with_name("input2").required(true).index(2)),
        )
        .subcommand(
            SubCommand::with_name("ssimulacra")
                .about("Calculate ssimulacra")
                .arg(Arg::with_name("input1").required(true).index(1))
                .arg(Arg::with_name("input2").required(true).index(2)),
        )
        .get_matches();

    let result = match args.subcommand_name().unwrap() {
        "butter" => compute_butter(args.subcommand_matches("butter").unwrap()),
        "ssimulacra" => compute_ssimulacra(args.subcommand_matches("ssimulacra").unwrap()),
        _ => unreachable!(),
    };

    println!("{}", result);
}

fn compute_butter(args: &ArgMatches) -> f64 {
    let butteraugli_path =
        env::var("BUTTERAUGLI_PATH").unwrap_or_else(|_| "butteraugli".to_string());
    let input1 = Path::new(args.value_of("input1").unwrap());
    let input2 = Path::new(args.value_of("input2").unwrap());
    run_metric(&butteraugli_path, input1, input2)
}

fn compute_ssimulacra(args: &ArgMatches) -> f64 {
    let ssimulacra_path = env::var("SSIMULACRA_PATH").unwrap_or_else(|_| "ssimulacra".to_string());
    let input1 = Path::new(args.value_of("input1").unwrap());
    let input2 = Path::new(args.value_of("input2").unwrap());
    run_metric(&ssimulacra_path, input1, input2)
}

fn run_metric(base_command: &str, input1: &Path, input2: &Path) -> f64 {
    let mut dec1 = Y4MDecoder::new(input1).expect("Failed to open file");
    let details1 = dec1.get_video_details();
    let mut dec2 = Y4MDecoder::new(input2).expect("Failed to open file");
    let details2 = dec2.get_video_details();
    assert_eq!(details1.height, details2.height);
    assert_eq!(details1.width, details2.width);

    let mut sum = 0.0f64;
    let mut frameno = 0;

    loop {
        match (details1.bit_depth, details2.bit_depth) {
            (8, 8) => {
                let frame1 = dec1.read_video_frame::<u8>();
                let frame2 = dec2.read_video_frame::<u8>();
                if frame1.is_none() || frame2.is_none() {
                    if frame1.is_some() || frame2.is_some() {
                        eprintln!(
                            "WARNING: Clips did not match in length! Ending at frame {}",
                            frameno
                        );
                    }
                    break;
                }
                sum += compare_frame(base_command, &frame1.unwrap(), &frame2.unwrap());
            }
            (8, _) => {
                let frame1 = dec1.read_video_frame::<u8>();
                let frame2 = dec2.read_video_frame::<u16>();
                if frame1.is_none() || frame2.is_none() {
                    if frame1.is_some() || frame2.is_some() {
                        eprintln!(
                            "WARNING: Clips did not match in length! Ending at frame {}",
                            frameno
                        );
                    }
                    break;
                }
                sum += compare_frame(base_command, &frame1.unwrap(), &frame2.unwrap());
            }
            (_, 8) => {
                let frame1 = dec1.read_video_frame::<u16>();
                let frame2 = dec2.read_video_frame::<u8>();
                if frame1.is_none() || frame2.is_none() {
                    if frame1.is_some() || frame2.is_some() {
                        eprintln!(
                            "WARNING: Clips did not match in length! Ending at frame {}",
                            frameno
                        );
                    }
                    break;
                }
                sum += compare_frame(base_command, &frame1.unwrap(), &frame2.unwrap());
            }
            (_, _) => {
                let frame1 = dec1.read_video_frame::<u16>();
                let frame2 = dec2.read_video_frame::<u16>();
                if frame1.is_none() || frame2.is_none() {
                    if frame1.is_some() || frame2.is_some() {
                        eprintln!(
                            "WARNING: Clips did not match in length! Ending at frame {}",
                            frameno
                        );
                    }
                    break;
                }
                sum += compare_frame(base_command, &frame1.unwrap(), &frame2.unwrap());
            }
        };

        frameno += 1;
    }

    if frameno == 0 {
        panic!("No frames read");
    }

    sum / frameno as f64
}

fn compare_frame<T: Pixel, U: Pixel>(
    base_command: &str,
    frame1: &FrameInfo<T>,
    frame2: &FrameInfo<U>,
) -> f64 {
    let (_, path1) = Builder::new()
        .suffix(".ppm")
        .tempfile()
        .unwrap()
        .keep()
        .unwrap();
    let (_, path2) = Builder::new()
        .suffix(".ppm")
        .tempfile()
        .unwrap()
        .keep()
        .unwrap();
    {
        if size_of::<T>() == 1 {
            let image1: RgbImage = ImageBuffer::from_raw(
                frame1.planes[0].cfg.width as u32,
                frame1.planes[0].cfg.height as u32,
                yuv_to_rgb_u8(frame1),
            )
            .unwrap();
            image1.save(&path1).unwrap();
        } else {
            let image1: ImageBuffer<Rgb<u16>, Vec<u16>> = ImageBuffer::from_raw(
                frame1.planes[0].cfg.width as u32,
                frame1.planes[0].cfg.height as u32,
                yuv_to_rgb_u16(frame1),
            )
            .unwrap();
            image1.save(&path1).unwrap();
        }
        if size_of::<U>() == 1 {
            let image2: RgbImage = ImageBuffer::from_raw(
                frame2.planes[0].cfg.width as u32,
                frame2.planes[0].cfg.height as u32,
                yuv_to_rgb_u8(frame2),
            )
            .unwrap();
            image2.save(&path2).unwrap();
        } else {
            let image2: ImageBuffer<Rgb<u16>, Vec<u16>> = ImageBuffer::from_raw(
                frame2.planes[0].cfg.width as u32,
                frame2.planes[0].cfg.height as u32,
                yuv_to_rgb_u16(frame2),
            )
            .unwrap();
            image2.save(&path2).unwrap();
        }
    }
    let output = Command::new(base_command)
        .arg(&path1)
        .arg(&path2)
        .output()
        .unwrap();

    let _ = fs::remove_file(path1);
    let _ = fs::remove_file(path2);

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .find(|line| !line.is_empty())
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

fn yuv_to_rgb_u8<T: Pixel>(frame: &FrameInfo<T>) -> Vec<u8> {
    assert!(size_of::<T>() == 1);

    let plane_y = &frame.planes[0];
    let plane_u = &frame.planes[1];
    let plane_v = &frame.planes[2];

    // TODO: Support HDR content
    let colorspace = if plane_y.cfg.height > 576 {
        MatrixCoefficients::BT709
    } else {
        MatrixCoefficients::BT601
    };
    let (ss_x, ss_y) = match frame.chroma_sampling {
        ChromaSampling::Cs400 => {
            return (0..plane_y.cfg.height)
                .flat_map(|y| {
                    (0..plane_y.cfg.width).flat_map(move |x| {
                        let val = u8::cast_from(plane_y.p(x, y));
                        [val, val, val].into_iter()
                    })
                })
                .collect();
        }
        ChromaSampling::Cs420 => (1, 1),
        ChromaSampling::Cs422 => (0, 1),
        ChromaSampling::Cs444 => (0, 0),
    };

    debug_assert_eq!(frame.bit_depth, 8);
    let converter = RGBConvert::<u8>::new(Range::Limited, colorspace).unwrap();
    (0..plane_y.cfg.height)
        .flat_map(|y| {
            let converter = converter.clone();
            (0..plane_y.cfg.width).flat_map(move |x| {
                let (chroma_x, chroma_y) = (x >> ss_x, y >> ss_y);
                let y = u8::cast_from(plane_y.p(x, y));
                let u = u8::cast_from(plane_u.p(chroma_x, chroma_y));
                let v = u8::cast_from(plane_v.p(chroma_x, chroma_y));
                let yuv = YUV { y, u, v };
                let rgb = converter.to_rgb(yuv);
                [rgb.r, rgb.g, rgb.b].into_iter()
            })
        })
        .collect()
}

fn yuv_to_rgb_u16<T: Pixel>(frame: &FrameInfo<T>) -> Vec<u16> {
    assert!(size_of::<T>() == 2);

    let plane_y = &frame.planes[0];
    let plane_u = &frame.planes[1];
    let plane_v = &frame.planes[2];

    // TODO: Support HDR content
    let colorspace = if plane_y.cfg.height > 576 {
        MatrixCoefficients::BT709
    } else {
        MatrixCoefficients::BT601
    };
    let (ss_x, ss_y) = match frame.chroma_sampling {
        ChromaSampling::Cs400 => {
            return (0..plane_y.cfg.height)
                .flat_map(|y| {
                    (0..plane_y.cfg.width).flat_map(move |x| {
                        let val = u16::cast_from(plane_y.p(x, y));
                        [val, val, val].into_iter()
                    })
                })
                .collect();
        }
        ChromaSampling::Cs420 => (1, 1),
        ChromaSampling::Cs422 => (0, 1),
        ChromaSampling::Cs444 => (0, 0),
    };

    let converter = RGBConvert::<u16>::new(
        Range::Limited,
        colorspace,
        match frame.bit_depth {
            10 => Depth::Depth10,
            12 => Depth::Depth12,
            16 => Depth::Depth16,
            _ => panic!("Unsupported bit depth"),
        },
    )
    .unwrap();
    (0..plane_y.cfg.height)
        .flat_map(|y| {
            let converter = converter.clone();
            (0..plane_y.cfg.width).flat_map(move |x| {
                let (chroma_x, chroma_y) = (x >> ss_x, y >> ss_y);
                let y = u16::cast_from(plane_y.p(x, y));
                let u = u16::cast_from(plane_u.p(chroma_x, chroma_y));
                let v = u16::cast_from(plane_v.p(chroma_x, chroma_y));
                let yuv = YUV { y, u, v };
                let rgb = converter.to_rgb(yuv);
                [rgb.r, rgb.g, rgb.b].into_iter()
            })
        })
        .collect()
}
