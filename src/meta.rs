use std::{path::PathBuf, sync::atomic::AtomicUsize};

use url::Url;

use crate::{schema::Schema, utility};

const SEGMENT_SIZE: usize = utility::parse_or(option_env!("SEGMENT_SIZE"), 1 << 20);

pub struct Header {
    pub url: Url,
    // TODO: path lifetime.
    pub path: PathBuf,
    pub size: usize,
}

impl Header {
    fn new(url: Url, path: PathBuf) -> Self {
        let size = Self::request_size(&url).unwrap();
        Self { url, path, size }
    }

    pub fn scheme(&self) -> Option<&dyn Schema> {
        self.url.scheme().try_into().ok()
    }

    fn request_size(url: &Url) -> Option<usize> {
        (url.scheme().try_into() as Result<&dyn Schema, _>)
            .unwrap()
            .size(url)
    }
}

pub struct Segment {
    pub offset: usize,
    pub size: usize,
    pub downloaded: AtomicUsize,
}

impl Segment {
    fn build(file_size: usize) -> Box<[Self]> {
        (0..(file_size + SEGMENT_SIZE - 1) / SEGMENT_SIZE)
            .map(|x| Self {
                offset: SEGMENT_SIZE * x,
                size: SEGMENT_SIZE.min(file_size - SEGMENT_SIZE * x),
                downloaded: Default::default(),
            })
            .collect()
    }
}

pub struct Meta {
    pub header: Header,
    pub segments: Box<[Segment]>, // TODO: `Box`-ing is not needed
}

impl Meta {
    pub fn new(url: Url, path: PathBuf) -> Self {
        let header = Header::new(url, path);
        let segments = Segment::build(header.size);
        Self { header, segments }
    }
}
