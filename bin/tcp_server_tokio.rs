use slava::{bufread::BufRead, socket::TcpListener};
use tokio::task::spawn as tokio_spawn;

const HTTP_HEADER: &'static [u8] = b"HTTP/1.1 200 OK\r\nContent-Type: video/mp4\r\nConnection: close\r\n\r\n";
const CONGRATULATIONS: &'static [u8] = include_bytes!("omedetou.mp4");

#[tokio::main]
async fn main() {
    let mut tcp_listener = TcpListener::new(4396);

    loop {
        match tcp_listener.accept().await {
            Ok(mut stream) => {
                eprintln!("accepting connection");
                tokio_spawn(async move {
                    let mut bufread = BufRead::new(&stream);
                    let request_line = match bufread.read_line().await {
                        Ok(line) => line,
                        Err(e) => {
                            eprintln!("error reading HTTP request: {}", e);
                            return;
                        }
                    };
                    eprintln!("read request line: {}", request_line.trim());

                    if let Err(e) = stream.write(HTTP_HEADER).await {
                        eprintln!("error writing HTTP header: {}", e);
                        return;
                    }

                    if let Err(e) = stream.write(CONGRATULATIONS).await {
                        eprintln!("error writing HTTP payload: {}", e);
                        return;
                    }

                    eprintln!("done serving contents");
                });
            }
            Err(e) => eprintln!("{}", e)
        }
    }
}
