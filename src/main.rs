#[cfg(feature = "splice")]
mod splice;

use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write},
    net::TcpStream,
    path::Path,
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

struct Header {
    loc: Location,
    path: String,
    size: usize,
}

struct Segment {
    offset: usize,
    size: usize,
    downloaded: AtomicUsize,
}

struct Meta {
    header: Header,
    segments: Box<[Segment]>, // TODO: `Box`-ing is not needed
}

type Message = (Arc<Meta>, usize);

const NUM_THREADS: usize = 64;
const SEGMENT_SIZE: usize = (2 << 30) / NUM_THREADS;
#[cfg(not(feature = "splice"))]
const BUFFER_SIZE: usize = 1 << 20;

/// Performs an HTTP/HEAD request and returns the content-length or `None` if not specified
// TODO: checking for Accept-Ranges: bytes?
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
                    let y = x.trim().parse().unwrap();
                    return Some(y);
                }
            }

            None
        }
    }
}

fn thread_handler(rx: Arc<Mutex<Receiver<Message>>>) {
    while let Ok((meta, idx)) = {
        let lock = rx.lock().unwrap();
        lock.recv()
    } {
        let hdr = &meta.header;
        let sgm = &meta.segments[idx];
        let mut file = File::options().write(true).open(&hdr.path).unwrap();
        file.seek(SeekFrom::Start(sgm.offset as u64)).unwrap();

        match &hdr.loc {
            Location::Http {
                netloc,
                hostname,
                uri,
            } => {
                let mut stream = TcpStream::connect(netloc).unwrap();
                write!(stream, "GET {} HTTP/1.0\r\n", uri).unwrap();
                write!(stream, "Host: {}\r\n", hostname).unwrap();
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
                        sgm.downloaded.fetch_add(x, Ordering::Relaxed);
                        file.write_all(&buf[..x]).unwrap();
                    }
                }

                #[cfg(feature = "splice")]
                {
                    let buf = reader.buffer();
                    let len = buf.len();
                    file.write_all(buf).unwrap();
                    sgm.downloaded.fetch_add(len, Ordering::Relaxed);

                    let x = splice::splice(
                        &reader.into_inner(),
                        &file,
                        sgm.size - len,
                        &sgm.downloaded,
                    )
                    .unwrap();
                    assert_eq!(x, sgm.size - len);
                }
            }
        }
    }
}

struct DownloadManager {
    tx: Option<Sender<Message>>,
    handles: Vec<JoinHandle<()>>,
    files: BTreeMap<usize, Arc<Meta>>,
    id: usize,
}

impl DownloadManager {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let rx = Arc::new(Mutex::new(rx));
        // TODO: get number of threads
        let handles = (0..NUM_THREADS)
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

        let header = Header { loc, path, size };
        let segments: Box<_> = (0..(size + SEGMENT_SIZE - 1) / SEGMENT_SIZE)
            .map(|x| Segment {
                offset: SEGMENT_SIZE * x,
                size: SEGMENT_SIZE.min(size - SEGMENT_SIZE * x),
                downloaded: Default::default(),
            })
            .collect();
        let meta = Arc::new(Meta { header, segments });

        for idx in 0..meta.segments.len() {
            self.tx.as_ref().unwrap().send((meta.clone(), idx)).unwrap();
        }
        self.files.insert(id, meta);
        id
    }

    fn get_info(&self, id: usize) -> Arc<Meta> {
        self.files.get(&id).unwrap().clone()
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
    let mut dm = DownloadManager::new();
    let infos = io::stdin()
        .lines()
        .map(|line| {
            let line = line?;
            let id = dm.download(
                line.trim().into(),
                Path::new(&line)
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .into(),
            );
            Ok(dm.get_info(id))
        })
        .collect::<Result<Vec<_>, io::Error>>()
        .unwrap();

    for _ in 0..infos.len() {
        println!();
    }

    loop {
        let mut l = false;
        print!("\x1b[{}A", infos.len());

        for info in &infos {
            let mut p = 0;
            let mut show = |x: usize, c: char| {
                while info.header.size * (p + 1) <= 80 * x {
                    print!("{}", c);
                    p += 1;
                }
            };

            print!("[");
            for sgm in info.segments.iter() {
                let download = sgm.downloaded.load(Ordering::Relaxed);
                l |= download != sgm.size;
                show(sgm.offset + download, '#');
                show(sgm.offset + sgm.size, ' ');
            }
            println!("]");
        }

        if !l {
            break;
        }

        thread::sleep(Duration::from_secs(1));
    }
}
