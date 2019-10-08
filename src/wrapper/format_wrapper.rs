#![allow(non_snake_case)]

use crate::binding::avformat::{
    self, AVFormatContext, AVIOContext, AVMediaType_AVMEDIA_TYPE_AUDIO,
    AVMediaType_AVMEDIA_TYPE_VIDEO, AVStream, AVSEEK_SIZE, SEEK_CUR, SEEK_END, SEEK_SET,
};
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;

pub const WRAPPER_BUFFER_SIZE: i32 = 32768;

#[derive(Debug)]
pub struct WrapperAvioBuffer {
    pub base: *mut u8,
    pub ptr: *mut u8,
    pub total: i64,
    pub size: i64,
}

#[derive(Debug)]
pub struct WrapperInputParams {
    pub path: *const c_char,
    pub ns: c_int,
    pub ans: c_int,
    pub vns: c_int,
    pub fmtctx: *mut AVFormatContext,
    pub ioctx: *mut AVIOContext,
    pub ioctx_buffer: *mut u8,
    pub iobuffer: *mut WrapperAvioBuffer,
}

fn ffmin(a: c_int, b: c_int) -> c_int {
    if a > b {
        return b;
    }
    a
}

pub unsafe extern "C" fn read_packet(opaque: *mut c_void, buf: *mut u8, buf_size: c_int) -> c_int {
    let bd: *mut WrapperAvioBuffer = opaque as (*mut WrapperAvioBuffer);
    let new_buf_size = ffmin(buf_size, (*bd).size as i32);
    avformat::memcpy(
        buf as (*mut c_void),
        (*bd).ptr as (*const c_void),
        new_buf_size as usize,
    );
    (*bd).ptr = (*bd).ptr.offset(new_buf_size as isize);
    (*bd).size -= new_buf_size as i64;
    new_buf_size
}

pub unsafe extern "C" fn seek_packet(opaque: *mut c_void, offset: i64, whence: i32) -> i64 {
    let bd: *mut WrapperAvioBuffer = opaque as (*mut WrapperAvioBuffer);
    let new_size: i64;

    match whence as u32 {
        SEEK_SET => {
            if offset < (*bd).total {
                (*bd).ptr = (*bd).base.offset(offset as isize);
                (*bd).size = (*bd).total - offset;
                return offset;
            }
        }
        SEEK_CUR => {
            new_size = (*bd).size - offset;
            if new_size >= 0 && new_size < (*bd).total {
                (*bd).ptr = (*bd).ptr.offset(offset as isize);
                (*bd).size = new_size;
                return (*bd).total - (*bd).size;
            }
        }
        SEEK_END => {
            new_size = -offset;
            if new_size >= 0 && new_size < (*bd).total {
                (*bd).ptr = (*bd).base.offset(((*bd).total + offset) as isize);
                (*bd).size = -offset;
                return (*bd).total - (*bd).size;
            }
        }
        AVSEEK_SIZE => return (*bd).total,
        _ => return -1,
    }
    return -1;
}

pub unsafe fn wrapper_fillin_buffer(data: *const u8, size: i64) -> *mut WrapperAvioBuffer {
    let buffer: *mut WrapperAvioBuffer =
        avformat::malloc(std::mem::size_of::<WrapperAvioBuffer>()) as (*mut WrapperAvioBuffer);
    (*buffer).base = avformat::malloc(size as usize) as (*mut u8);
    (*buffer).ptr = (*buffer).base;
    (*buffer).size = size;
    (*buffer).total = size;
    avformat::memcpy(
        (*buffer).ptr as (*mut c_void),
        data as (*mut c_void),
        size as usize,
    );
    buffer
}

pub unsafe fn wrapper_avformat_open_input(fp: *const c_char, p: *mut WrapperInputParams) -> i32 {
    if fp.is_null() {
        (*p).fmtctx = avformat::avformat_alloc_context();
        if (*p).fmtctx.is_null() {
            return -888;
        }

        (*p).ioctx_buffer = avformat::av_malloc(WRAPPER_BUFFER_SIZE as usize) as (*mut u8);
        (*p).ioctx = avformat::avio_alloc_context(
            (*p).ioctx_buffer,
            WRAPPER_BUFFER_SIZE,
            0,
            (*p).iobuffer as *mut c_void,
            Some(read_packet),
            None,
            Some(seek_packet),
        );
        (*(*p).fmtctx).pb = (*p).ioctx;
    }

    let mut ret: i32;
    ret = avformat::avformat_open_input(&mut (*p).fmtctx, fp, ptr::null_mut(), ptr::null_mut());
    if ret < 0 {
        return ret;
    }

    ret = avformat::avformat_find_stream_info((*p).fmtctx, ptr::null_mut());
    if ret != 0 {
        return ret;
    }

    let mut videos = 0;
    let mut audios = 0;

    (*p).ns = (*(*p).fmtctx).nb_streams as i32;
    let streams_slice = std::slice::from_raw_parts((*(*p).fmtctx).streams, (*p).ns as usize);
    for i in 0..(*p).ns as usize {
        let codec_type = (*(*streams_slice[i]).codecpar).codec_type;
        if codec_type == AVMediaType_AVMEDIA_TYPE_VIDEO {
            videos += 1;
        } else if codec_type == AVMediaType_AVMEDIA_TYPE_AUDIO {
            audios += 1;
        }
    }
    (*p).vns = videos;
    (*p).ans = audios;
    return 0;
}

pub unsafe fn wrapper_get_stream_handlers(
    fmtctx: *mut AVFormatContext,
    codec_type: i32,
    n: i32,
    pstream: *mut *mut *mut AVStream,
) {
    *pstream =
        avformat::malloc(n as usize * std::mem::size_of::<*mut AVStream>()) as (*mut *mut AVStream);
    let streams = std::slice::from_raw_parts_mut(*pstream, n as usize);
    let streams_slice =
        std::slice::from_raw_parts((*fmtctx).streams, (*fmtctx).nb_streams as usize);
    let mut current = 0;
    for i in 0..(*fmtctx).nb_streams as usize {
        if (*(*streams_slice[i]).codecpar).codec_type == codec_type {
            streams[current] = streams_slice[i];
            current += 1;
        }
    }
}

#[inline]
pub unsafe fn wrapper_free_stream_handlers(streams: *mut *mut AVStream) {
    avformat::free(streams as *mut c_void);
}

#[inline]
pub unsafe fn wrapper_free_ioctx(ioctx: *mut AVIOContext) {
    let buffer_ptr: *mut *mut u8 = &mut (*ioctx).buffer;
    avformat::av_freep(buffer_ptr as *mut c_void);
    avformat::av_freep(ioctx as *mut c_void);
}
