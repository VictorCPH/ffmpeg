use std::error::Error;
use std::fmt::{self, Display};

use super::binding::avcodec::AVERROR_EOF;
use super::wrapper::error_wrapper::*;

#[derive(Debug)]
pub struct FFmpegError {
    code: i32,
    desc: String,
    detail: String,
}

impl Display for FFmpegError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.detail)
    }
}

impl Error for FFmpegError {
    fn description(&self) -> &str {
        &self.detail
    }
}

impl FFmpegError {
    pub fn new(code: i32, desc: &str) -> Self {
        let desc = desc.to_string();
        let detail: String;
        if code == AVERROR_EOF {
            detail = "AV_EOF".to_string();
        } else {
            let err_str = err2str(code);
            detail = format!("{}: {}", desc, err_str);
        }
        Self { code, desc, detail }
    }
}
