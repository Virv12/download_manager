use std::fs::File;

use url::Url;

use crate::meta::{Header, Segment};

mod http;
pub use http::Http;

pub trait Schema {
    fn size(&self, url: &Url) -> Option<usize>;
    // TODO: Better abstraction than File.
    fn handle(&self, header: &Header, segment: &Segment, file: File);
}
