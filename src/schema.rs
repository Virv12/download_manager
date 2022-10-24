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

impl TryFrom<&str> for &dyn Schema {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "http" => Ok(&Http {}),
            _ => Err(()),
        }
    }
}
