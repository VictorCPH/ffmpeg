use std::error::Error;
use std::fs::File;
use std::io::Read;

extern crate ffmpeg;
use ffmpeg::format;
use ffmpeg::{Orientation, Stream};

fn traverse_frame(stream: &mut Stream, width: i32, height: i32, stride: i32) {
    let mut frame_count = 0;
    loop {
        match stream.next_video_frame() {
            Err(err) => {
                assert_eq!("AV_EOF", err.description());
                break;
            }
            Ok(frame) => {
                let pts = frame.pts;
                let dts = frame.dts;
                assert_eq!(width, frame.width);
                assert_eq!(height, frame.height);
                assert_eq!(stride, frame.stride);
                if stream.duration > 0.0001 {
                    // FIXME: duration may be 0.0
                    assert!(pts > -0.0001 && pts < stream.duration);
                    assert!(dts > -0.0001 && dts < stream.duration);
                }
                frame_count += 1;
            }
        }
    }
    assert!(frame_count > 0)
}

#[test]
fn test_meta() {
    ffmpeg::init();
    let mut fm = format::load_video_from_file("fixture/video/example.mp4").unwrap();
    assert_eq!(2, fm.n_streams);
    assert_eq!(1, fm.n_video_streams);
    assert_eq!(1, fm.n_audio_streams);

    assert_eq!(1, fm.video_streams().len());
    assert_eq!(1, fm.audio_streams().len());

    let vs = &fm.video_streams()[0];
    assert_eq!(Orientation::Top, vs.orientation);
    assert!(vs.fps > 24.0 && vs.fps < 24.1);
}

#[test]
fn test_video_from_file() {
    ffmpeg::init();
    for format in ["flv", "mp4", "mov", "avi"].iter() {
        let mut fm =
            format::load_video_from_file(&format!("fixture/video/example.{}", format)[..])
                .unwrap();
        assert_eq!(1, fm.video_streams().len());
        traverse_frame(&mut fm.video_streams()[0], 640, 360, 1920);
    }
}

#[test]
fn test_video_from_blob() {
    ffmpeg::init();
    for format in ["flv", "mp4", "mov", "avi"].iter() {
        let file_name = &format!("fixture/video/example.{}", format)[..];
        let mut blob = Vec::new();
        File::open(file_name)
            .unwrap()
            .read_to_end(&mut blob)
            .unwrap();

        let mut fm = format::load_video_from_blob(blob).unwrap();
        assert_eq!(1, fm.video_streams().len());
        traverse_frame(&mut fm.video_streams()[0], 640, 360, 1920);
    }
}

#[test]
fn test_video_not_existed() {
    ffmpeg::init();
    let err = format::load_video_from_file("not_existed.mp4").unwrap_err();
    assert_eq!("ffmpeg_open: No such file or directory", err.description())
}

#[test]
fn test_video_broken() {
    ffmpeg::init();
    let err = format::load_video_from_file("fixture/video/broken.mp4").unwrap_err();
    assert_eq!(
        "ffmpeg_open: Invalid data found when processing input",
        err.description()
    )
}

#[test]
fn test_seek() {
    ffmpeg::init();
    let mut fm = format::load_video_from_file("fixture/video/example.mp4").unwrap();
    assert_eq!(1, fm.video_streams().len());

    let vs = &fm.video_streams()[0];
    assert_eq!((), vs.seek_by_time(1.5).unwrap());
    assert_eq!((), vs.seek_by_frame(10).unwrap());
}

#[test]
fn test_decode_audio() {
    ffmpeg::init();
    let mut fm = format::load_video_from_file("fixture/video/audio.mp3").unwrap();
    assert_eq!(1, fm.audio_streams().len());

    let audio_stream = &fm.audio_streams()[0];
    let expected_size = 225048;

    let pcm = audio_stream.get_audio_data(1, 16000).unwrap();
    assert_eq!(expected_size, pcm.len());

    audio_stream.seek_by_frame(0).unwrap();
    let pcm = audio_stream.get_audio_data(1, 8000).unwrap();
    assert_eq!(expected_size / 2, pcm.len());
}
