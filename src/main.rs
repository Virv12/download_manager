use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
};

use url::Url;

#[cfg(feature = "progress")]
use std::{sync::atomic::Ordering, time::Duration};

use crate::meta::Meta;
use crate::utility::parse_or;

mod meta;
mod schema;

mod utility;

type Message = (Arc<Meta>, usize);

const NUM_THREADS: usize = parse_or(option_env!("NUM_THREADS"), 64);

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

    fn download(&mut self, url: Url, path: PathBuf) -> usize {
        File::create(&path).unwrap();
        let meta = Arc::new(Meta::new(url, path));

        let id = self.id;
        self.id += 1;

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
                line.trim().try_into().unwrap(),
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
