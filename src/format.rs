use std::ffi::CString;
use std::os::raw::c_void;
use std::ptr;

use super::binding::avformat::{
    self, AVFormatContext, AVIOContext, AVMediaType_AVMEDIA_TYPE_AUDIO,
    AVMediaType_AVMEDIA_TYPE_VIDEO, AVStream,
};
use super::error::*;
use super::stream::{self, Stream};
use super::wrapper::format_wrapper::*;

#[derive(Debug)]
pub struct Format {
    pub n_streams: i32,
    pub n_video_streams: i32,
    pub n_audio_streams: i32,
    pub video_streams: Vec<Stream>,
    pub audio_streams: Vec<Stream>,
    pub fmtctx: *mut AVFormatContext,
    pub cache: *mut u8,
    pub ioctx: *mut AVIOContext,
    pub bd: *mut WrapperAvioBuffer,
}

pub fn load_video_from_file(path: &str) -> Result<Format, FFmpegError> {
    let c_path = CString::new(path).unwrap();
    let mut params = WrapperInputParams {
        path: c_path.as_ptr(),
        ns: 0,
        ans: 0,
        vns: 0,
        fmtctx: ptr::null_mut(),
        ioctx: ptr::null_mut(),
        ioctx_buffer: ptr::null_mut(),
        iobuffer: ptr::null_mut(),
    };

    let ret = unsafe { wrapper_avformat_open_input(c_path.as_ptr(), &mut params) };
    match ret {
        0 => Ok(Format {
            n_streams: params.ns,
            n_video_streams: params.vns,
            n_audio_streams: params.ans,
            video_streams: vec![],
            audio_streams: vec![],
            fmtctx: params.fmtctx,
            ioctx: ptr::null_mut(),
            cache: ptr::null_mut(),
            bd: ptr::null_mut(),
        }),
        code => Err(FFmpegError::new(code, "ffmpeg_open")),
    }
}

pub fn load_video_from_blob(blob: Vec<u8>) -> Result<Format, FFmpegError> {
    let buffer = unsafe { wrapper_fillin_buffer(blob.as_ptr(), blob.len() as i64) };
    let mut params = WrapperInputParams {
        path: ptr::null_mut(),
        ns: 0,
        ans: 0,
        vns: 0,
        fmtctx: ptr::null_mut(),
        ioctx: ptr::null_mut(),
        ioctx_buffer: ptr::null_mut(),
        iobuffer: buffer,
    };

    let ret = unsafe { wrapper_avformat_open_input(ptr::null_mut(), &mut params) };
    match ret {
        0 => Ok(Format {
            n_streams: params.ns,
            n_video_streams: params.vns,
            n_audio_streams: params.ans,
            video_streams: vec![],
            audio_streams: vec![],
            fmtctx: params.fmtctx,
            ioctx: params.ioctx,
            cache: unsafe { (*buffer).base },
            bd: buffer,
        }),
        code => Err(FFmpegError::new(code, "ffmpeg_open")),
    }
}

impl Drop for Format {
    fn drop(&mut self) {
        if !self.fmtctx.is_null() {
            unsafe { avformat::avformat_close_input(&mut self.fmtctx) };
            self.fmtctx = ptr::null_mut::<AVFormatContext>();
        }
        if !self.ioctx.is_null() {
            unsafe { wrapper_free_ioctx(self.ioctx) };
            self.ioctx = ptr::null_mut::<AVIOContext>();
        }
        if !self.cache.is_null() {
            unsafe { avformat::free(self.cache as *mut c_void) };
            self.cache = ptr::null_mut::<u8>();
        }
        if !self.bd.is_null() {
            unsafe { avformat::free(self.bd as *mut c_void) };
            self.bd = ptr::null_mut::<WrapperAvioBuffer>();
        }
    }
}

impl Format {
    fn get_streams(&mut self, t: i32, n: i32) -> Vec<Stream> {
        if n <= 0 {
            return vec![];
        }

        unsafe {
            let mut streams: *mut *mut AVStream = std::mem::uninitialized();
            wrapper_get_stream_handlers(self.fmtctx, t, n, &mut streams);
            let slice = std::slice::from_raw_parts(streams, n as usize);
            let mut ret = Vec::new();
            for stream in slice {
                ret.push(stream::new_stream(*stream, self, t))
            }
            wrapper_free_stream_handlers(streams);
            ret
        }
    }

    pub fn video_streams(&mut self) -> &mut Vec<Stream> {
        if self.video_streams.is_empty() {
            self.video_streams =
                self.get_streams(AVMediaType_AVMEDIA_TYPE_VIDEO, self.n_video_streams);
        }
        &mut self.video_streams
    }

    pub fn audio_streams(&mut self) -> &mut Vec<Stream> {
        if self.audio_streams.is_empty() {
            self.audio_streams =
                self.get_streams(AVMediaType_AVMEDIA_TYPE_AUDIO, self.n_audio_streams);
        }
        &mut self.audio_streams
    }
}
