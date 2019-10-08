use std::ffi::CString;
use std::os::raw::{c_int, c_void};
use std::ptr;

use crate::binding::audio_fifo::{self, AVAudioFifo};
use crate::binding::avcodec::{AVERROR_EOF, EAGAIN};
use crate::binding::avformat::{
    self, AVCodec, AVCodecContext, AVCodecParameters, AVDictionaryEntry, AVFormatContext, AVFrame,
    AVMediaType_AVMEDIA_TYPE_VIDEO, AVPacket, AVPixelFormat_AV_PIX_FMT_BGR24, AVRational,
    AVRounding_AV_ROUND_UP, AVStream, AVSEEK_FLAG_ANY,
};
use crate::binding::avutil::AV_NOPTS_VALUE;
use crate::binding::imgutils;
use crate::binding::swresample::{self, AVSampleFormat_AV_SAMPLE_FMT_S16, SwrContext};
use crate::binding::swscale::{self, SwsContext, SWS_FAST_BILINEAR};

#[derive(Debug)]
pub struct WrapperFrameOutput {
    pub bgr24_buf: [*mut u8; 4usize],
    pub linesize: [c_int; 4usize],
    pub img_convert_ctx: *mut SwsContext,
    pub buffer: *mut u8,
    pub allocated: c_int,
    pub buffersize: c_int,
    pub width: c_int,
    pub height: c_int,
    pub stride: c_int,
}

#[derive(Debug, Default)]
pub struct WrapperStreamMeta {
    pub fps: f32,
    pub idx: c_int,
    pub rtt: c_int,
    pub nfr: c_int,
    pub dur: f32,
    pub tb: f32,
}

#[derive(Debug)]
pub struct WrapperDecodeCtx {
    pub cctx: *mut AVCodecContext,
    pub frame: *mut AVFrame,
    pub packet: *mut AVPacket,
    pub output: WrapperFrameOutput,
    pub need_sent: c_int,
    pub pts: f32,
    pub dts: f32,
}

#[inline]
fn av_q2d(a: AVRational) -> f64 {
    a.num as f64 / a.den as f64
}

pub unsafe extern "C" fn next_packet(
    format_context: *mut AVFormatContext,
    packet: *mut AVPacket,
) -> i32 {
    if !(*packet).data.is_null() {
        avformat::av_packet_unref(packet);
    }

    let ret = avformat::av_read_frame(format_context, packet);
    match ret {
        n if n < 0 => n,
        _ => 0,
    }
}

pub unsafe extern "C" fn next_packet_for_stream(
    format_context: *mut AVFormatContext,
    stream_index: i32,
    packet: *mut AVPacket,
) -> i32 {
    let mut ret = next_packet(format_context, packet);
    while (*packet).stream_index != stream_index && ret == 0 {
        ret = next_packet(format_context, packet);
    }
    return ret;
}

pub unsafe fn frame_to_rawdata_rgb(
    avctx: *mut AVCodecContext,
    from: *mut AVFrame,
    output: *mut WrapperFrameOutput,
) -> i32 {
    let width = (*avctx).width;
    let height = (*avctx).height;
    let pixel_format = (*avctx).pix_fmt;

    if (*output).img_convert_ctx.is_null() {
        (*output).img_convert_ctx = swscale::sws_getContext(
            width,
            height,
            pixel_format,
            width,
            height,
            AVPixelFormat_AV_PIX_FMT_BGR24,
            SWS_FAST_BILINEAR as i32,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        )
    }

    let mut ret: i32;
    if (*output).allocated == 0 {
        ret = imgutils::av_image_alloc(
            (*output).bgr24_buf.as_mut_ptr(),
            (*output).linesize.as_mut_ptr(),
            width,
            height,
            AVPixelFormat_AV_PIX_FMT_BGR24,
            1,
        );
        if ret < 0 {
            return ret;
        }
        (*output).allocated = 1;
    }

    ret = swscale::sws_scale(
        (*output).img_convert_ctx,
        (*from).data.as_ptr() as (*const *const u8),
        (*from).linesize.as_ptr(),
        0,
        height,
        (*output).bgr24_buf.as_mut_ptr(),
        (*output).linesize.as_mut_ptr(),
    );
    if ret < 0 {
        return ret;
    }

    let stride = (*output).linesize[0];

    (*output).buffer = (*output).bgr24_buf[0];
    (*output).height = height;
    (*output).width = width;
    (*output).stride = stride;

    return 0;
}

pub unsafe fn wrapper_get_meta(s: *mut AVStream, kind: i32, out: *mut WrapperStreamMeta) {
    (*out).idx = (*s).index;
    (*out).nfr = (*s).nb_frames as i32;
    (*out).tb = av_q2d((*s).time_base) as f32;

    if kind != AVMediaType_AVMEDIA_TYPE_VIDEO {
        return;
    }
    (*out).fps = av_q2d(avformat::av_stream_get_r_frame_rate(s)) as f32;

    if (*s).duration == AV_NOPTS_VALUE {
        (*out).dur = 0.0;
    } else {
        (*out).dur = ((*s).duration as f64 * av_q2d((*s).time_base)) as f32;
    }

    let mut tag = ptr::null_mut::<AVDictionaryEntry>();
    let rotate = CString::new("rotate").unwrap();
    tag = avformat::av_dict_get((*s).metadata, rotate.as_ptr(), tag, 0);
    if tag.is_null() {
        (*out).rtt = 0;
    } else {
        (*out).rtt = avformat::atoi((*tag).value) % 360;
    }
}

pub unsafe fn extract_next_frame(
    format_context: *mut AVFormatContext,
    ctx: *mut WrapperDecodeCtx,
    stream_index: i32,
) -> i32 {
    if (*(*ctx).cctx).codec.is_null() {
        return -99999;
    }

    let mut ret;

    loop {
        if (*ctx).need_sent != 0 {
            ret = next_packet_for_stream(format_context, stream_index, (*ctx).packet);
            if ret < 0 {
                return ret;
            }

            ret = avformat::avcodec_send_packet((*ctx).cctx, (*ctx).packet);

            if ret < 0 {
                if ret == AVERROR_EOF {
                    return 0;
                }
                return ret;
            }
            (*ctx).need_sent = 0;
        }

        ret = avformat::avcodec_receive_frame((*ctx).cctx, (*ctx).frame);
        let streams = std::slice::from_raw_parts(
            (*format_context).streams,
            (*format_context).nb_streams as usize,
        );
        let stream: *mut AVStream = streams[stream_index as usize];

        if ret == AVERROR_EOF || ret == -(EAGAIN as i32) {
            (*ctx).need_sent = 1;
            continue;
        } else if ret < 0 {
            return ret;
        } else if ret >= 0 {
            (*ctx).pts =
                (*(*ctx).frame).best_effort_timestamp as f32 * (av_q2d((*stream).time_base) as f32);
            (*ctx).dts = (*(*ctx).frame).pkt_dts as f32 * (av_q2d((*stream).time_base) as f32);
            return frame_to_rawdata_rgb((*ctx).cctx, (*ctx).frame, &mut (*ctx).output);
        }
    }
}

pub unsafe fn wrapper_stream_seek_by_time(
    s: *mut AVStream,
    c: *mut AVFormatContext,
    cc: *mut AVCodecContext,
    position: f32,
) -> i32 {
    let mut ts: i64 = (position as f64 / av_q2d((*s).time_base)) as i64;

    if (*c).start_time != AV_NOPTS_VALUE {
        ts += (*c).start_time;
    }

    let rc = avformat::av_seek_frame(c, (*s).index, ts, AVSEEK_FLAG_ANY as i32);
    if rc < 0 {
        return rc;
    }

    if !cc.is_null() {
        avformat::avcodec_flush_buffers(cc);
    }
    return rc;
}

pub unsafe fn wrapper_stream_create_decode_ctx(
    s: *mut AVStream,
    out: *mut WrapperDecodeCtx,
) -> i32 {
    let av_codec_par: *mut AVCodecParameters = (*s).codecpar;
    let codec: *mut AVCodec = avformat::avcodec_find_decoder((*av_codec_par).codec_id);
    if codec.is_null() {
        return -1;
    }

    let mut ret;
    (*out).cctx = avformat::avcodec_alloc_context3(ptr::null_mut());
    if (*out).cctx.is_null() {
        return -2;
    }

    ret = avformat::avcodec_parameters_to_context((*out).cctx, av_codec_par);
    if ret < 0 {
        return -3;
    }

    ret = avformat::avcodec_open2((*out).cctx, codec, ptr::null_mut());
    if ret < 0 {
        return -4;
    }

    (*out).frame = avformat::av_frame_alloc();
    (*out).packet = avformat::av_packet_alloc();
    (*(*out).packet).size = 0;
    (*out).need_sent = 1;
    avformat::av_init_packet((*out).packet);

    return 0;
}

pub unsafe fn read_and_convert_audio_frame(
    swr_context: *mut SwrContext,
    fifo: *mut AVAudioFifo,
    decoding_packet: *mut AVPacket,
    codec_context: *mut AVCodecContext,
    decoding_frame: *mut AVFrame,
    dst_sample_fmt: i32,
    dst_sample_rate: i32,
    dst_nb_channels: i32,
) -> i32 {
    let mut converted_input_samples = ptr::null_mut::<*mut u8>();
    let mut rc: i32;

    loop {
        rc = avformat::avcodec_send_packet(codec_context, decoding_packet);
        if rc < 0 {
            break;
        }

        rc = avformat::avcodec_receive_frame(codec_context, decoding_frame);
        if rc < 0 {
            break;
        }

        let src_frame_size = (*decoding_frame).nb_samples;
        let dst_frame_size = avformat::av_rescale_rnd(
            src_frame_size as i64,
            dst_sample_rate as i64,
            (*codec_context).sample_rate as i64,
            AVRounding_AV_ROUND_UP,
        );
        converted_input_samples =
            avformat::calloc(dst_nb_channels as usize, std::mem::size_of::<*mut u8>())
                as (*mut *mut u8);
        if converted_input_samples.is_null() {
            rc = -1;
            break;
        }

        if avformat::av_samples_alloc(
            converted_input_samples,
            ptr::null_mut(),
            dst_nb_channels,
            dst_frame_size as i32,
            dst_sample_fmt,
            0,
        ) < dst_frame_size as i32
        {
            rc = -1;
            break;
        }

        rc = swresample::swr_convert(
            swr_context,
            converted_input_samples,
            dst_frame_size as i32,
            (*decoding_frame).extended_data as (*mut *const u8),
            src_frame_size,
        );
        if rc < 0 {
            break;
        }

        rc = audio_fifo::av_audio_fifo_realloc(
            fifo,
            audio_fifo::av_audio_fifo_size(fifo) + dst_frame_size as i32,
        );
        if rc < 0 {
            break;
        }

        if audio_fifo::av_audio_fifo_write(
            fifo,
            converted_input_samples as (*mut *mut c_void),
            dst_frame_size as i32,
        ) < dst_frame_size as i32
        {
            rc = -1;
            break;
        }
        break;
    }

    if !converted_input_samples.is_null() {
        avformat::av_freep(converted_input_samples as *mut c_void);
        avformat::free(converted_input_samples as *mut c_void);
    }
    return rc;
}

pub unsafe fn wrapper_destroy_stream(ctx: *mut WrapperDecodeCtx) {
    if ctx.is_null() {
        return;
    }
    if !(*ctx).frame.is_null() {
        avformat::av_frame_free(&mut (*ctx).frame);
    }
    if !(*ctx).packet.is_null() {
        avformat::av_packet_free(&mut (*ctx).packet);
    }
    if !(*ctx).cctx.is_null() {
        avformat::avcodec_free_context(&mut (*ctx).cctx);
    }
    if !(*ctx).output.allocated != 0 {
        avformat::av_freep((*ctx).output.bgr24_buf.as_mut_ptr() as (*mut c_void));
        swscale::sws_freeContext((*ctx).output.img_convert_ctx);
        (*ctx).output.allocated = 0;
    }
}

pub unsafe fn wrapper_stream_decode_audio(
    s: *mut AVStream,
    c: *mut AVFormatContext,
    dst_channel_layout: u64,
    dst_sample_rate: i32,
    recv_buffer: *mut *mut u8,
    buf_size: *mut i32,
) -> i32 {
    let av_codec_par: *mut AVCodecParameters = (*s).codecpar;
    let codec: *mut AVCodec = avformat::avcodec_find_decoder((*av_codec_par).codec_id);
    if codec.is_null() {
        return -1;
    }

    let mut codec_context: *mut AVCodecContext = avformat::avcodec_alloc_context3(ptr::null_mut());
    if codec_context.is_null() {
        return -2;
    }
    avformat::av_codec_set_pkt_timebase(codec_context, (*s).time_base);

    let mut rc = avformat::avcodec_parameters_to_context(codec_context, av_codec_par);
    if rc < 0 {
        return -3;
    }

    rc = avformat::avcodec_open2(codec_context, codec, ptr::null_mut());
    if rc < 0 {
        return rc;
    }

    let mut swr_context: *mut SwrContext;
    let mut fifo: *mut AVAudioFifo = ptr::null_mut();
    let mut decoding_frame: *mut AVFrame = ptr::null_mut();
    let mut output_samples: *mut *mut u8 = ptr::null_mut();

    let mut packet: AVPacket = std::mem::uninitialized();
    avformat::av_init_packet(&mut packet);

    loop {
        let mut src_channel = (*codec_context).channel_layout;
        if src_channel == 0 {
            src_channel = 1;
        }
        let src_sample_rate = (*codec_context).sample_rate;
        let src_sample_fmt = (*codec_context).sample_fmt;
        let dst_nb_channels = avformat::av_get_channel_layout_nb_channels(dst_channel_layout);
        let dst_sample_fmt = AVSampleFormat_AV_SAMPLE_FMT_S16;

        swr_context = swresample::swr_alloc_set_opts(
            ptr::null_mut(),
            dst_channel_layout as i64,
            dst_sample_fmt,
            dst_sample_rate,
            src_channel as i64,
            src_sample_fmt,
            src_sample_rate,
            0,
            ptr::null_mut(),
        );
        if swr_context.is_null() {
            rc = -1;
            break;
        }

        rc = swresample::swr_init(swr_context);
        if rc < 0 {
            break;
        }

        fifo = audio_fifo::av_audio_fifo_alloc(dst_sample_fmt, dst_nb_channels, 1);
        if fifo.is_null() {
            rc = -1;
            break;
        }

        decoding_frame = avformat::av_frame_alloc();
        if decoding_frame.is_null() {
            rc = -1;
            break;
        }

        while 0 == next_packet_for_stream(c, (*s).index, &mut packet) {
            rc = read_and_convert_audio_frame(
                swr_context,
                fifo,
                &mut packet,
                codec_context,
                decoding_frame,
                dst_sample_fmt,
                dst_sample_rate,
                dst_nb_channels,
            );
            if rc < 0 {
                break;
            }
        }

        let fifo_size = audio_fifo::av_audio_fifo_size(fifo);
        if fifo_size <= 0 {
            break;
        }

        output_samples = avformat::calloc(dst_nb_channels as usize, std::mem::size_of::<*mut u8>())
            as *mut *mut u8;
        if output_samples.is_null() {
            rc = -1;
            break;
        }

        if avformat::av_samples_alloc(
            output_samples,
            ptr::null_mut(),
            dst_nb_channels,
            fifo_size,
            dst_sample_fmt,
            0,
        ) < fifo_size
        {
            rc = -1;
            break;
        }

        if audio_fifo::av_audio_fifo_read(fifo, output_samples as *mut *mut c_void, fifo_size)
            < fifo_size
        {
            rc = -1;
            break;
        }

        let out_size =
            avformat::av_samples_get_buffer_size(ptr::null_mut(), 1, fifo_size, dst_sample_fmt, 1);

        *recv_buffer = avformat::malloc(out_size as usize) as *mut u8;
        if (*recv_buffer).is_null() {
            rc = -1;
            break;
        }

        avformat::memcpy(
            *recv_buffer as *mut c_void,
            *output_samples as *const c_void,
            out_size as usize,
        );
        *buf_size = out_size;
        break;
    }

    avformat::av_packet_unref(&mut packet);
    if !decoding_frame.is_null() {
        avformat::av_frame_free(&mut decoding_frame);
    }

    if !swr_context.is_null() {
        swresample::swr_free(&mut swr_context);
    }

    if !fifo.is_null() {
        audio_fifo::av_audio_fifo_free(fifo);
    }

    if !output_samples.is_null() {
        avformat::av_freep(output_samples as *mut c_void);
        avformat::free(output_samples as *mut c_void);
    }

    if !codec_context.is_null() {
        avformat::avcodec_free_context(&mut codec_context);
    }

    return rc;
}
