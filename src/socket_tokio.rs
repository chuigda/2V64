use std::io::{Error as IOError, Result as IOResult};
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::socket::{add_read_fd, add_write_fd, socket_context_get_or_init, TcpStream};

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
        if buf.len() == 0 {
            return Poll::Ready(Ok(0));
        }

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

        if bytes_written == 0 {
            let waker = cx.waker().clone();
            add_write_fd(self.fd, waker);
            return Poll::Pending;
        }

        Poll::Ready(Ok(bytes_written as usize))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<IOResult<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_> ) -> Poll<IOResult<()>> {
        unsafe { libc::shutdown(self.fd, libc::SHUT_WR) };

        let mut socket_context = socket_context_get_or_init();
        socket_context.readfds.remove(&self.fd);
        socket_context.writefds.remove(&self.fd);

        Poll::Ready(Ok(()))
    }
}
