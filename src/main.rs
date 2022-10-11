use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    net::TcpStream,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use urlparse::Url;

enum Location {
    Http {
        netloc: String,
        hostname: String,
        uri: String,
    },
}

impl From<String> for Location {
    fn from(url: String) -> Location {
        let Url {
            scheme,
            mut netloc,
            path,
            query,
            fragment,
            hostname,
            port,
            ..
        } = urlparse::urlparse(url);

        assert_eq!(scheme, "http");

        if port.is_none() {
            netloc += ":80";
        }

        let mut uri = path;
        if let Some(query) = &query {
            uri.push('?');
            uri += query;
        }
        if let Some(fragment) = &fragment {
            uri.push('#');
            uri += fragment;
        }

        Location::Http {
            netloc,
            hostname: hostname.unwrap(),
            uri,
        }
    }
}

struct Meta {
    loc: Location,
    path: String,
    partial: AtomicUsize,
    size: usize,
}

struct Segment {
    meta: Arc<Meta>,
    offset: usize,
    size: usize,
}

const THREADS: usize = 64;
const SEGMENT: usize = (2 * 1 << 30) / THREADS;
const BUFFER: usize = 1 << 20;

/// Performs an HTTP/HEAD request and returns the content-length or `None` if not specified
/// TODO: checking for Accept-Ranges: bytes?
fn get_size(loc: &Location) -> Option<usize> {
    match loc {
        Location::Http {
            netloc,
            hostname,
            uri,
        } => {
            let mut stream = TcpStream::connect(netloc).unwrap();
            write!(stream, "HEAD {} HTTP/1.0\r\n", uri).unwrap();
            write!(stream, "Host: {}\r\n", hostname).unwrap();
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
                    let y: usize = x.trim().parse().unwrap();
                    return Some(y);
                }
            }

            None
        }
    }
}

fn thread_handler(rx: Arc<Mutex<Receiver<Segment>>>) {
    while let Ok(Segment { meta, offset, size }) = {
        let lock = rx.lock().unwrap();
        lock.recv()
    } {
        let mut file = File::options().write(true).open(&meta.path).unwrap();
        file.seek(SeekFrom::Start(offset as u64)).unwrap();

        match &meta.loc {
            Location::Http {
                netloc,
                hostname,
                uri,
            } => {
                let mut stream = TcpStream::connect(netloc).unwrap();
                write!(stream, "GET {} HTTP/1.0\r\n", uri).unwrap();
                write!(stream, "Host: {}\r\n", hostname).unwrap();
                write!(stream, "Range: bytes={}-{}\r\n", offset, offset + size - 1).unwrap();
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

                let mut buf = [0u8; BUFFER];
                loop {
                    let x = reader.read(&mut buf).unwrap();
                    if x == 0 {
                        break;
                    }
                    meta.partial.fetch_add(x, Ordering::Relaxed);
                    file.write_all(&buf[..x]).unwrap();
                }
            }
        }
    }
}

struct DownloadManager {
    tx: Option<Sender<Segment>>,
    handles: Vec<JoinHandle<()>>,
    files: BTreeMap<usize, Arc<Meta>>,
    id: usize,
}

impl DownloadManager {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let rx = Arc::new(Mutex::new(rx));
        // TODO: get number of threads
        let handles = (0..THREADS)
            .map(|_| {
                let rx = rx.clone();
                thread::spawn(move || thread_handler(rx))
            })
            .collect();
        let files = BTreeMap::new();
        let id = 0;
        DownloadManager {
            tx: Some(tx),
            handles,
            files,
            id,
        }
    }

    fn download(&mut self, url: String, path: String) -> usize {
        let loc = url.into();
        let size = get_size(&loc).unwrap();
        File::create(&path).unwrap();
        let id = self.id;
        self.id += 1;
        let meta = Meta {
            loc,
            path,
            partial: Default::default(),
            size,
        };
        let arc = Arc::new(meta);
        let mut offset = 0;
        let segment_size = SEGMENT;
        while offset < size {
            let segment = Segment {
                meta: arc.clone(),
                offset,
                size: segment_size.min(size - offset),
            };
            self.tx.as_ref().unwrap().send(segment).unwrap();
            offset += segment_size;
        }
        self.files.insert(id, arc);
        id
    }

    fn completed(&self, id: usize) -> bool {
        let arc = self.files.get(&id).unwrap();
        arc.partial.load(Ordering::Relaxed) == arc.size
    }

    fn progress(&self, id: usize) -> f64 {
        let arc = self.files.get(&id).unwrap();
        arc.partial.load(Ordering::Relaxed) as f64 / arc.size as f64
    }
}

impl Drop for DownloadManager {
    fn drop(&mut self) {
        self.tx.take();
        for handle in self.handles.drain(..) {
            handle.join().unwrap();
        }
    }
}

fn main() {
    let mut url = String::new();
    std::io::stdin().read_line(&mut url).unwrap();

    let mut dm = DownloadManager::new();
    let id = dm.download(url.trim().into(), "download".into());
    while !dm.completed(id) {
        println!("{}", dm.progress(id));
        thread::sleep(Duration::from_secs(1));
    }
}
