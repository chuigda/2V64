use std::collections::{HashMap, HashSet};
use std::ffi::c_int;
use std::io::{Error as IOError, Result as IOResult};
use std::pin::Pin;
use std::sync::{Mutex, Once, OnceLock};
use std::task::{Context, Poll, Waker};
use std::thread::{sleep as thread_sleep, spawn as spawn_thread};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

struct SocketContext {
    pub readfds: HashMap<c_int, Waker>,
    pub writefds: HashMap<c_int, Waker>,

    pub closefds: HashSet<c_int>
}

static SOCKET_CONTEXT: OnceLock<Mutex<SocketContext>> = OnceLock::new();
static SOCKET_BACKGROUND_THREAD: Once = Once::new();

macro_rules! socket_context {
    () => { SOCKET_CONTEXT.get_or_init(init_socket_context).lock().unwrap() }
}

fn init_socket_context() -> Mutex<SocketContext> {
    Mutex::new(SocketContext {
        readfds: HashMap::new(),
        writefds: HashMap::new(),

        closefds: HashSet::new()
    })
}

fn maybe_init_background_thread() {
    SOCKET_BACKGROUND_THREAD.call_once(|| {
        spawn_thread(|| {
            loop {
                let mut socket_context = socket_context!();
                if !socket_context.closefds.is_empty() {
                    let mut buf = [0u8; 1024];
                    let closefds = socket_context.closefds.clone();
                    for closefd in closefds {
                        let bytes_read = unsafe { libc::read(closefd, buf.as_mut_ptr() as *mut _, buf.len()) };
                        if bytes_read > 0 {
                            continue;
                        }

                        if bytes_read < 0 {
                            let errno = unsafe { *libc::__errno_location() };
                            if errno == libc::EAGAIN && errno == libc::EWOULDBLOCK {
                                continue;
                            }
                        }

                        unsafe { libc::close(closefd) };
                        socket_context.closefds.remove(&closefd);
                    }
                }

                let nfds = socket_context.readfds.len() + socket_context.writefds.len();
                if nfds == 0 {
                    drop(socket_context);
                    thread_sleep(Duration::from_millis(160));
                    continue;
                }

                let mut poll_fds = Vec::with_capacity(nfds);
                for readfd in socket_context.readfds.keys() {
                    poll_fds.push(libc::pollfd {
                        fd: *readfd,
                        events: libc::POLLIN,
                        revents: 0,
                    });
                }
                for writefd in socket_context.writefds.keys() {
                    poll_fds.push(libc::pollfd {
                        fd: *writefd,
                        events: libc::POLLOUT,
                        revents: 0,
                    });
                }
                drop(socket_context);

                let fd = unsafe { libc::poll(poll_fds.as_mut_ptr(), nfds as libc::nfds_t, 160) };

                if fd < 0 {
                    panic!("poll failed, error code = {}", unsafe { *libc::__errno_location() });
                }

                if fd == 0 {
                    continue;
                }

                let mut socket_context = socket_context!();
                for poll_fd in poll_fds.iter() {
                    if poll_fd.revents != 0 {
                        if let Some(waker) = socket_context.readfds.remove(&poll_fd.fd) {
                            waker.wake();
                        }

                        if let Some(waker) = socket_context.writefds.remove(&poll_fd.fd) {
                            waker.wake();
                        }
                    }
                }
            }
        });
    });
}

fn add_read_fd(fd: c_int, waker: Waker) {
    maybe_init_background_thread();
    socket_context!().readfds.insert(fd, waker);
}

fn add_write_fd(fd: c_int, waker: Waker) {
    maybe_init_background_thread();
    socket_context!().writefds.insert(fd, waker);
}

#[derive(Debug)]
pub struct TcpListener {
    sockfd: c_int
}

impl TcpListener {
    pub fn new(port: u16) -> Self {
        let sockfd = unsafe { libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0) };
        unsafe {
            libc::fcntl(sockfd, libc::F_SETFL, libc::O_NONBLOCK);
            libc::setsockopt(
                sockfd,
                libc::SOL_SOCKET,
                libc::SO_REUSEADDR,
                &1 as *const _ as *const _,
                std::mem::size_of::<c_int>() as _
            );
        }

        let server_addr = libc::sockaddr_in {
            sin_family: libc::AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: libc::in_addr { s_addr: libc::INADDR_ANY },
            sin_zero: [0; 8],
        };

        unsafe {
            libc::bind(
                sockfd,
                &server_addr as *const _ as *const libc::sockaddr,
                std::mem::size_of_val(&server_addr) as u32,
            );
            let listen_result = libc::listen(sockfd, 5);
            if listen_result != 0 {
                panic!("listen failed, error code = {listen_result}");
            }
        }

        Self { sockfd }
    }

    pub fn accept(&mut self) -> Pin<Box<dyn Future<Output=Result<TcpStream, String>> + Send + Sync>> {
        struct AcceptFuture {
            listener_sockfd: c_int
        }

        impl Future for AcceptFuture {
            type Output = Result<TcpStream, String>;

            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                unsafe {
                    let fd = libc::accept(self.listener_sockfd, std::ptr::null_mut(), std::ptr::null_mut());
                    if fd >= 0 {
                        libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK);
                        return Poll::Ready(Ok(TcpStream { fd }));
                    }

                    let errno = *libc::__errno_location();
                    if errno != libc::EAGAIN {
                        return Poll::Ready(Err(format!("accept failed, error code = {}", errno)));
                    }

                    let waker = cx.waker().clone();
                    add_read_fd(self.listener_sockfd, waker);
                    Poll::Pending
                }
            }
        }

        Box::pin(AcceptFuture { listener_sockfd: self.sockfd })
    }
}

impl Drop for TcpListener {
    fn drop(&mut self) {
        unsafe { libc::close(self.sockfd) };
    }
}

#[derive(Debug)]
pub struct TcpStream {
    fd: c_int
}

impl TcpStream {
    pub fn read_bytes<'a>(
        &self,
        buf: &'a mut [u8]
    ) -> Pin<Box<dyn 'a + Future<Output=Result<usize, String>> + Send + Sync>> {
        struct ReadFuture<'b> {
            fd: c_int,
            buf: &'b mut [u8]
        }

        impl<'b> Future for ReadFuture<'b> {
            type Output = Result<usize, String>;

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                unsafe {
                    let bytes_read = libc::read(
                        self.fd,
                        self.buf.as_mut_ptr() as *mut _,
                        self.buf.len()
                    );

                    if bytes_read > 0 {
                        Poll::Ready(Ok(bytes_read as usize))
                    } else if bytes_read == 0 {
                        Poll::Ready(Ok(0))
                    } else {
                        let errno = *libc::__errno_location();
                        if errno != libc::EAGAIN && errno != libc::EWOULDBLOCK {
                            return Poll::Ready(Err(format!("read failed, error code = {}", errno)));
                        }

                        let waker = cx.waker().clone();
                        add_read_fd(self.fd, waker);
                        Poll::Pending
                    }
                }
            }
        }

        Box::pin(ReadFuture { fd: self.fd, buf })
    }

    pub fn write_bytes<'a>(
        &mut self,
        buf: &'a [u8]
    ) -> Pin<Box<dyn 'a + Future<Output=Result<usize, String>> + Send + Sync>> {
        struct WriteFuture<'b> {
            fd: c_int,
            buf: &'b [u8],
            bytes_written: usize
        }

        impl<'b> Future for WriteFuture<'b> {
            type Output = Result<usize, String>;

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                unsafe {
                    let slice_to_write = &self.buf[self.bytes_written..];
                    let bytes_written = libc::write(
                        self.fd,
                        slice_to_write.as_ptr() as *const _,
                        slice_to_write.len()
                    );

                    if bytes_written >= 0 {
                        self.bytes_written += bytes_written as usize;
                        if self.bytes_written == self.buf.len() {
                            return Poll::Ready(Ok(self.bytes_written));
                        }

                        let waker = cx.waker().clone();
                        add_write_fd(self.fd, waker);
                        Poll::Pending
                    } else {
                        let errno = *libc::__errno_location();
                        if errno != libc::EAGAIN && errno != libc::EWOULDBLOCK {
                            return Poll::Ready(Err(format!("write failed, error code = {}", errno)));
                        }

                        let waker = cx.waker().clone();
                        add_write_fd(self.fd, waker);
                        Poll::Pending
                    }
                }
            }
        }

        Box::pin(WriteFuture { fd: self.fd, buf, bytes_written: 0 })
    }
}

impl Drop for TcpStream {
    fn drop(&mut self) {
        unsafe { libc::shutdown(self.fd, libc::SHUT_WR) };

        let mut socket_context = socket_context!();
        socket_context.closefds.insert(self.fd);
        socket_context.readfds.remove(&self.fd);
        socket_context.writefds.remove(&self.fd);
    }
}

impl AsyncRead for TcpStream {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<IOResult<()>> {
        let local_buf = buf.initialize_unfilled();
        let bytes_read = unsafe { libc::read(self.fd, local_buf.as_mut_ptr() as *mut _, local_buf.len() as _) };
        if bytes_read < 0 {
            let errno = unsafe { *libc::__errno_location() };
            if errno == libc::EAGAIN || errno == libc::EWOULDBLOCK {
                let waker = cx.waker().clone();
                add_read_fd(self.fd, waker);
                return Poll::Pending;
            }

            return Poll::Ready(Err(IOError::from_raw_os_error(errno)));
        }

        buf.advance(bytes_read as usize);
        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for TcpStream {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<IOResult<usize>> {
        let bytes_written = unsafe { libc::write(self.fd, buf.as_ptr() as *const _, buf.len() as _) };
        if bytes_written < 0 {
            let errno = unsafe { *libc::__errno_location() };
            if errno == libc::EAGAIN || errno == libc::EWOULDBLOCK {
                let waker = cx.waker().clone();
                add_write_fd(self.fd, waker);
                return Poll::Pending;
            }

            return Poll::Ready(Err(IOError::from_raw_os_error(errno)));
        }

        Poll::Ready(Ok(bytes_written as usize))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<IOResult<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_> ) -> Poll<IOResult<()>> {
        unsafe { libc::shutdown(self.fd, libc::SHUT_WR) };

        let mut socket_context = socket_context!();
        socket_context.readfds.remove(&self.fd);
        socket_context.writefds.remove(&self.fd);

        Poll::Ready(Ok(()))
    }
}
