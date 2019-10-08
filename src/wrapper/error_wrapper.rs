use std::ffi::CStr;
use std::mem::size_of;

use crate::binding::avutil::{self, AV_ERROR_MAX_STRING_SIZE};

pub fn err2str(code: i32) -> String {
    unsafe {
        let errbuf = avutil::malloc(AV_ERROR_MAX_STRING_SIZE as usize * size_of::<i8>()) as *mut i8;
        avutil::av_strerror(code, errbuf, AV_ERROR_MAX_STRING_SIZE as usize);
        CStr::from_ptr(errbuf).to_string_lossy().into_owned()
    }
}
