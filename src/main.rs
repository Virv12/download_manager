use std::{
    fs::File,
    io::{self, Seek, SeekFrom, Error},
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
};

#[cfg(feature = "progress")]
use std::{sync::atomic::Ordering, time::Duration};

use url::Url;

use crate::meta::Meta;

mod meta;
mod schema;
mod utility;

type Message = (Arc<Meta>, usize);

const NUM_THREADS: usize = utility::parse_or(option_env!("NUM_THREADS"), 64);

fn thread_handler(rx: Arc<Mutex<Receiver<Message>>>) {
    while let Ok((meta, idx)) = {
        let lock = rx.lock().unwrap();
        lock.recv()
    } {
        let hdr = &meta.header;
        let sgm = &meta.segments[idx];
        let mut file = File::options().write(true).open(&hdr.path).unwrap();
        file.seek(SeekFrom::Start(sgm.offset as u64)).unwrap();
        hdr.scheme().unwrap().handle(hdr, sgm, file);
    }
}

struct DownloadManager {
    // TODO: find a solution without Option
    tx: Option<Sender<Message>>,
    handles: Option<Box<[JoinHandle<()>; NUM_THREADS]>>,
}

impl DownloadManager {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let rx = Arc::new(Mutex::new(rx));
        let handles = Box::new([(); NUM_THREADS].map(|_| {
            let rx = rx.clone();
            thread::spawn(move || thread_handler(rx))
        }));
        DownloadManager {
            tx: Some(tx),
            handles: Some(handles),
        }
    }

    fn download(&mut self, url: Url, path: PathBuf) -> Arc<Meta> {
        File::create(&path).unwrap();
        let meta = Arc::new(Meta::new(url, path));
        for idx in 0..meta.segments.len() {
            self.tx.as_ref().unwrap().send((meta.clone(), idx)).unwrap();
        }
        meta
    }
}

impl Drop for DownloadManager {
    fn drop(&mut self) {
        self.tx.take();
        for handle in self.handles.take().unwrap().into_iter() {
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
            Ok(dm.download(
                line.trim().try_into().unwrap(),
                Path::new(&line).file_name().unwrap().into(),
            ))
        })
        .collect::<Result<Vec<_>, Error>>()
        .unwrap();

    #[cfg(feature = "progress")]
    {
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
}
