use std::{
    fs::File,
    io::{BufRead, BufReader, Write},
    net::TcpStream,
};

#[cfg(not(feature = "splice"))]
use std::io::Read;
#[cfg(feature = "progress")]
use std::sync::atomic::Ordering;

use url::Url;

use crate::{
    meta::{Header, Segment},
    schema::Schema,
    utility,
};

#[cfg(not(feature = "splice"))]
const BUFFER_SIZE: usize = utility::parse_or(option_env!("BUFFER_SIZE"), 1 << 20);

pub struct Http {}

fn netloc(url: &Url) -> String {
    format!("{}:{}", url.domain().unwrap(), url.port().unwrap_or(80))
}

fn query(url: &Url) -> String {
    let mut s = url.path().to_owned();
    if let Some(query) = url.query() {
        s += "?";
        s += query;
    }
    if let Some(fragment) = url.fragment() {
        s += "#";
        s += fragment;
    }
    s
}

impl Schema for Http {
    /// Performs an HTTP/HEAD request and returns the content-length or `None` if not specified
    // TODO: checking for Accept-Ranges: bytes?
    fn size(&self, url: &Url) -> Option<usize> {
        let mut stream = TcpStream::connect(netloc(url)).unwrap();
        write!(stream, "HEAD {} HTTP/1.0\r\n", query(url)).unwrap();
        write!(stream, "Host: {}\r\n", url.host_str().unwrap()).unwrap();
        write!(stream, "\r\n").unwrap();

        // TODO: do we really want a BufRead?
        let mut reader = BufReader::new(stream);
        let mut buf = String::new();

        loop {
            buf.clear();
            let n = reader.read_line(&mut buf).unwrap();
            if n <= 2 {
                break;
            }

            buf.make_ascii_lowercase();
            if let Some(x) = buf.strip_prefix("content-length: ") {
                let y = x.trim().parse().unwrap();
                return Some(y);
            }
        }

        None
    }

    fn handle(&self, hdr: &Header, sgm: &Segment, mut file: File) {
        let url = &hdr.url;
        let mut stream = TcpStream::connect(netloc(url)).unwrap();
        write!(stream, "GET {} HTTP/1.0\r\n", query(url)).unwrap();
        write!(stream, "Host: {}\r\n", url.host_str().unwrap()).unwrap();
        write!(
            stream,
            "Range: bytes={}-{}\r\n",
            sgm.offset,
            sgm.offset + sgm.size - 1
        )
        .unwrap();
        write!(stream, "\r\n").unwrap();

        // TODO: do we really want a BufRead?
        let mut reader = BufReader::new(stream);
        let mut buf = String::new();

        loop {
            buf.clear();
            let n = reader.read_line(&mut buf).unwrap();
            if n <= 2 {
                break;
            }
        }

        #[cfg(not(feature = "splice"))]
        {
            let mut buf = [0u8; BUFFER_SIZE];
            loop {
                let x = reader.read(&mut buf).unwrap();
                if x == 0 {
                    break;
                }
                #[cfg(feature = "progress")]
                sgm.downloaded.fetch_add(x, Ordering::Relaxed);
                file.write_all(&buf[..x]).unwrap();
            }
        }

        #[cfg(feature = "splice")]
        {
            let buf = reader.buffer();
            let len = buf.len();
            file.write_all(buf).unwrap();
            #[cfg(feature = "progress")]
            sgm.downloaded.fetch_add(len, Ordering::Relaxed);

            let x = utility::splice(&reader.into_inner(), &file, sgm.size - len, &sgm.downloaded)
                .unwrap();
            assert_eq!(x, sgm.size - len);
        }
    }
}
