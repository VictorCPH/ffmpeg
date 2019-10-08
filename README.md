# ffmpeg

[![Build Status](https://travis-ci.org/VictorCPH/ffmpeg.svg?branch=master)](https://travis-ci.org/VictorCPH/ffmpeg)

`ffmpeg` provides Rust FFI bindings to FFmpeg C libraries.

## Dependency

FFmpeg v3.3.9:

- libavutil55
- libavcodec57
- libavformat57
- libswscale4
- libswresample4

The dynamic libraries are stored in `lib64` directory.

## How to use

Add this to your Cargo.toml:

```toml
[dependencies]
ffmpeg = "0.1.0"
```

Copy `lib64` directory to your repository.

Run:

```shell
LIBRARY_PATH=./lib64 LD_LIBRARY_PATH=./lib64 cargo run
```

Test:

```shell
LIBRARY_PATH=./lib64 LD_LIBRARY_PATH=./lib64 cargo test
```

## How to run integration tests

Clone this repository:

```
git clone https://github.com/VictorCPH/ffmpeg.git
```

Run integration tests:

```shell
LIBRARY_PATH=./lib64 LD_LIBRARY_PATH=./lib64 cargo test -- --nocapture
```

Output:

```shell
running 7 tests
test test_video_not_existed ... ok
test test_video_broken ... ok
test test_meta ... ok
test test_seek ... ok
test test_decode_audio ... ok
test test_video_from_blob ... ok
test test_video_from_file ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## License

The repo is available as open source under the terms of the [MIT License](http://opensource.org/licenses/MIT).
