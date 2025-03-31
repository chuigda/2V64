use slava::{socket::TcpListener, Slava};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const HTTP_HEADER: &'static [u8] = b"HTTP/1.1 200 OK\r
Server: slava-tokio/slava-http-tokio-ioutil-mixed\r
Content-Type: video/mp4\r
Connection: close\r
\r
";
const CONGRATULATIONS: &'static [u8] = include_bytes!("omedetou.mp4");

fn main() {
    let slava = Slava::slava();
    let slava1 = slava.clone();

    slava.spawn(async move {
        let mut tcp_listener = TcpListener::new(4398);
        eprintln!("slava server started listening on port 4398");

        loop {
            match tcp_listener.accept().await {
                Ok(mut stream) => {
                    eprintln!("accepting connection");
                    slava1.spawn(async move {
                        let mut buf_reader = BufReader::new(&mut stream);
                        let mut request_line = String::new();
                        if let Err(e) =  buf_reader.read_line(&mut request_line).await {
                            eprintln!("error reading HTTP request: {}", e);
                            return;
                        };
                        eprintln!("read request line: {}", request_line.trim());

                        if let Err(e) = stream.write_all(HTTP_HEADER).await {
                            eprintln!("error writing HTTP header: {}", e);
                            return;
                        }

                        if let Err(e) = stream.write_all(CONGRATULATIONS).await {
                            eprintln!("error writing HTTP payload: {}", e);
                            return;
                        }

                        eprintln!("done serving contents");
                    });
                }
                Err(e) => eprintln!("error accepting TCP connection: {}", e)
            }
        }
    });

    slava.run(4);
}
