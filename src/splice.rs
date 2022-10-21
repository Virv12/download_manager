use std::{
    io,
    os::unix::io::{AsFd, AsRawFd},
    ptr,
    sync::atomic::{AtomicUsize, Ordering},
};

pub struct Pipe(libc::c_int, libc::c_int);

impl Pipe {
    pub fn new() -> io::Result<Self> {
        let mut pipes = [0 as libc::c_int; 2];
        unsafe {
            if libc::pipe2(&mut pipes as *mut libc::c_int, libc::O_NONBLOCK) < 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(Pipe(pipes[0], pipes[1]))
    }
}

impl Drop for Pipe {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.0);
            libc::close(self.1);
        }
    }
}

pub fn splice<I: AsFd, O: AsFd>(i: &I, o: &O, n: usize, upd: &AtomicUsize) -> io::Result<usize> {
    let rfd = i.as_fd().as_raw_fd();
    let wfd = o.as_fd().as_raw_fd();

    let pipe = Pipe::new()?;
    let (rpipe, wpipe) = (pipe.0, pipe.1);

    let mut done = false;
    let mut p = 0;
    while !done && p < n {
        let mut z = p;
        while z < n.min(p + libc::PIPE_BUF) {
            let t = unsafe {
                libc::splice(
                    rfd,
                    ptr::null_mut(),
                    wpipe,
                    ptr::null_mut(),
                    (n - z).min(libc::PIPE_BUF - (z - p)),
                    libc::SPLICE_F_MOVE | libc::SPLICE_F_NONBLOCK,
                )
            };
            if t > 0 {
                z += t as usize;
            } else if t < 0 {
                return Err(io::Error::last_os_error());
            } else {
                done = true;
                break;
            }
        }

        while p < z {
            let t = unsafe {
                libc::splice(
                    rpipe,
                    ptr::null_mut(),
                    wfd,
                    ptr::null_mut(),
                    z - p,
                    libc::SPLICE_F_MOVE | libc::SPLICE_F_NONBLOCK,
                )
            };
            if t > 0 {
                p += t as usize;
                upd.fetch_add(t as usize, Ordering::Relaxed);
            } else if t < 0 {
                return Err(io::Error::last_os_error());
            } else {
                unreachable!();
            }
        }
    }

    Ok(n)
}
