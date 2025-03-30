use slava::{socket::TcpStream, Slava};

const HTTP_HEADER: &'static [u8] = b"HTTP/1.1 200 OK\r\nContent-Type: video/mp4\r\nConnection: close\r\n\r\n";
const CONGRATULATIONS: &'static [u8] = include_bytes!("omedetou.mp4");

fn main() {
    let slava = Slava::slava();
    let slava1 = slava.clone();

    slava.spawn(async move {
        let mut tcp_listener = TcpStream::new(4396);

        loop {
            match tcp_listener.accept().await {
                Ok(stream) => {
                    eprintln!("accepting connection");
                    slava1.spawn(async move {
                        if let Err(e) = stream.write(HTTP_HEADER).await {
                            eprintln!("error writing HTTP header: {}", e);
                            return;
                        }

                        if let Err(e) = stream.write(CONGRATULATIONS).await {
                            eprintln!("error writing HTTP payload: {}", e);
                            return;
                        }
                    });
                    eprintln!("done serving contents");
                }
                Err(e) => eprintln!("{}", e)
            }
        }
    });

    slava.run();
}
