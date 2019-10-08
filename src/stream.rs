use std::os::raw::c_void;
use std::ptr;

use super::binding::avformat::{
    self, AVMediaType_AVMEDIA_TYPE_AUDIO, AVMediaType_AVMEDIA_TYPE_VIDEO, AVStream,
};
use super::error::*;
use super::format::Format;
use super::wrapper::stream_wrapper::*;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Orientation {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Debug)]
pub struct Stream {
    pub orientation: Orientation,
    pub n_frame: i32,
    pub fps: f32,
    pub duration: f32,
    pub format: *mut Format,
    pub time_base: f32,
    kind: i32,
    decode_ctx: WrapperDecodeCtx,
    index: i32,
    handle: *mut AVStream,
}

#[derive(Debug)]
pub struct Frame {
    pub buffer: Vec<u8>,
    pub width: i32,
    pub height: i32,
    pub stride: i32,
    pub pts: f32,
    pub dts: f32,
}

pub unsafe fn new_stream(handle: *mut AVStream, format: *mut Format, kind: i32) -> Stream {
    let mut meta: WrapperStreamMeta = std::mem::uninitialized();
    wrapper_get_meta(handle, kind, &mut meta);

    Stream {
        fps: meta.fps,
        orientation: degree_to_orientation(meta.rtt),
        n_frame: meta.nfr,
        format: format,
        duration: meta.dur,
        time_base: meta.tb,
        kind: kind,
        handle: handle,
        index: meta.idx,
        decode_ctx: WrapperDecodeCtx {
            cctx: ptr::null_mut(),
            frame: ptr::null_mut(),
            packet: ptr::null_mut(),
            output: WrapperFrameOutput {
                bgr24_buf: [ptr::null_mut(); 4usize],
                linesize: [0; 4usize],
                allocated: 0,
                buffer: ptr::null_mut(),
                img_convert_ctx: ptr::null_mut(),
                buffersize: 0,
                width: 0,
                height: 0,
                stride: 0,
            },
            need_sent: 0,
            pts: 0.0,
            dts: 0.0,
        },
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        let ctx = &mut self.decode_ctx;
        if !ctx.cctx.is_null() {
            unsafe { wrapper_destroy_stream(ctx) };
        }
        ctx.cctx = ptr::null_mut();
    }
}

impl Stream {
    pub fn seek_by_time(&self, t: f32) -> Result<(), FFmpegError> {
        let ret = unsafe {
            wrapper_stream_seek_by_time(self.handle, (*self.format).fmtctx, self.decode_ctx.cctx, t)
        };
        match ret {
            n if n < 0 => Err(FFmpegError::new(n, "ffmpeg_seek_time")),
            _ => Ok(()),
        }
    }

    pub fn seek_by_frame(&self, pos: i32) -> Result<(), FFmpegError> {
        let t = pos as f32 / self.fps;
        let ret = unsafe {
            wrapper_stream_seek_by_time(self.handle, (*self.format).fmtctx, self.decode_ctx.cctx, t)
        };
        match ret {
            n if n < 0 => Err(FFmpegError::new(n, "ffmpeg_seek_frame")),
            _ => Ok(()),
        }
    }

    pub fn next_video_frame(&mut self) -> Result<Frame, FFmpegError> {
        if self.kind != AVMediaType_AVMEDIA_TYPE_VIDEO {
            return Err(FFmpegError::new(-1, "stream type mismatch"));
        }

        if let Err(err) = self.init_ctx() {
            return Err(err);
        }

        let ret =
            unsafe { extract_next_frame((*self.format).fmtctx, &mut self.decode_ctx, self.index) };
        if ret < 0 {
            return Err(FFmpegError::new(ret, "next_frame"));
        }

        let output = &self.decode_ctx.output;
        Ok(Frame {
            buffer: unsafe {
                std::slice::from_raw_parts(output.buffer, (output.height * output.stride) as usize)
                    .to_vec()
            },
            width: output.width,
            height: output.height,
            stride: output.stride,
            pts: self.decode_ctx.pts,
            dts: self.decode_ctx.dts,
        })
    }

    pub fn get_audio_data(
        &self,
        channel_layout: i32,
        sample_rate: i32,
    ) -> Result<Vec<u8>, FFmpegError> {
        if self.kind != AVMediaType_AVMEDIA_TYPE_AUDIO {
            return Err(FFmpegError::new(-1, "stream type mismatch"));
        }
        if channel_layout <= 0 {
            return Err(FFmpegError::new(-1, "invalid channel layout"));
        }
        if sample_rate <= 0 {
            return Err(FFmpegError::new(-1, "invalid sample rate"));
        }

        let mut recv_buffer: *mut u8 = unsafe { std::mem::uninitialized() };
        let mut buffer_size: i32 = unsafe { std::mem::uninitialized() };
        let ret = unsafe {
            wrapper_stream_decode_audio(
                self.handle,
                (*self.format).fmtctx,
                channel_layout as u64,
                sample_rate,
                &mut recv_buffer,
                &mut buffer_size,
            )
        };
        if ret < 0 {
            return Err(FFmpegError::new(ret, "ffmpeg_stream_decode_audio"));
        }

        let data =
            unsafe { std::slice::from_raw_parts(recv_buffer, buffer_size as usize).to_vec() };
        if !recv_buffer.is_null() {
            unsafe { avformat::free(recv_buffer as *mut c_void) };
        }
        Ok(data)
    }

    fn init_ctx(&mut self) -> Result<(), FFmpegError> {
        if self.decode_ctx.cctx.is_null() {
            let ret =
                unsafe { wrapper_stream_create_decode_ctx(self.handle, &mut self.decode_ctx) };
            if ret < 0 {
                return Err(FFmpegError::new(ret, "ffmpeg_create_decode_ctx"));
            }
        }
        Ok(())
    }
}

fn degree_to_orientation(deg: i32) -> Orientation {
    match deg % 360 {
        0 => Orientation::Top,
        90 => Orientation::Left,
        180 => Orientation::Bottom,
        270 => Orientation::Right,
        _ => Orientation::Top,
    }
}
