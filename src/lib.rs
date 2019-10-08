mod binding;
mod wrapper;

pub mod error;
pub mod format;
pub mod stream;

pub use format::Format;
pub use stream::{Frame, Orientation, Stream};

use self::binding::avcodec;
use self::binding::avformat;

pub fn init() {
    unsafe {
        avformat::av_register_all();
        avformat::avformat_network_init();
        avcodec::avcodec_register_all();
    }
}
