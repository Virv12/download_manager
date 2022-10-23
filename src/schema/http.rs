use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
#[cfg(feature = "progress")]
use std::sync::atomic::Ordering;

use url::Url;

#[cfg(not(feature = "splice"))]
use crate::utility::parse_or;
use crate::meta::{Header, Segment};
use crate::schema::Schema;
#[cfg(feature = "splice")]
use crate::splice;

#[cfg(not(feature = "splice"))]
const BUFFER_SIZE: usize = parse_or(option_env!("BUFFER_SIZE"), 1 << 20);

pub struct Http {}

fn netloc(url: &Url) -> String {
    format!("{}:{}", url.domain().unwrap(), url.port().unwrap_or(80))
}

fn query(url: &Url) -> String {
    format!("{}?{}#{}", url.path(), url.query().unwrap_or(""), url.fragment().unwrap_or(""))
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

            let x = splice::splice(
                &reader.into_inner(),
                &file,
                sgm.size - len,
                &sgm.downloaded,
            ).unwrap();
            assert_eq!(x, sgm.size - len);
        }
    }
}
