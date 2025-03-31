use slava::{bufread::BufRead, socket::TcpListener, Slava};

const HTTP_HEADER: &'static [u8] = b"HTTP/1.1 200 OK\r
Server: slava/slava-http\r
Content-Type: video/mp4\r
Connection: close\r
\r
";
const CONGRATULATIONS: &'static [u8] = include_bytes!("omedetou.mp4");

fn main() {
    let slava = Slava::slava();
    let slava1 = slava.clone();

    slava.spawn(async move {
        let mut tcp_listener = TcpListener::new(4396);
        eprintln!("slava/tokio mixed server started listening on port 4396");

        loop {
            match tcp_listener.accept().await {
                Ok(mut stream) => {
                    eprintln!("accepting connection");
                    slava1.spawn(async move {
                        let mut bufread = BufRead::new(&stream);
                        let request_line = match bufread.read_line().await {
                            Ok(line) => line,
                            Err(e) => {
                                eprintln!("error reading HTTP request: {}", e);
                                return;
                            }
                        };
                        eprintln!("read request line: {}", request_line.trim());

                        if let Err(e) = stream.write_bytes(HTTP_HEADER).await {
                            eprintln!("error writing HTTP header: {}", e);
                            return;
                        }

                        if let Err(e) = stream.write_bytes(CONGRATULATIONS).await {
                            eprintln!("error writing HTTP payload: {}", e);
                            return;
                        }

                        eprintln!("done serving contents");
                    });
                }
                Err(e) => eprintln!("{}", e)
            }
        }
    });

    slava.run(4);
}
