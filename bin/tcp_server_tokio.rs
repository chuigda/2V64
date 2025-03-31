use slava::socket::TcpListener;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
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
                    let mut buf_reader = BufReader::new(&mut stream);
                    let mut request_line = String::new();
                    if let Err(e) =  buf_reader.read_line(&mut request_line).await {
                        eprintln!("error reading HTTP request: {}", e);
                        return;
                    };
                    eprintln!("read request line: {}", request_line.trim());

                    if let Err(e) = stream.write(HTTP_HEADER).await {
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
}
