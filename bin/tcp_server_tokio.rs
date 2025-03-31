use slava::socket::TcpStream;
use tokio::task::spawn as tokio_spawn;

const HTTP_HEADER: &'static [u8] = b"HTTP/1.1 200 OK\r\nContent-Type: video/mp4\r\nConnection: close\r\n\r\n";
const CONGRATULATIONS: &'static [u8] = include_bytes!("omedetou.mp4");

#[tokio::main]
async fn main() {
    let mut tcp_listener = TcpStream::new(4396);

    loop {
        match tcp_listener.accept().await {
            Ok(stream) => {
                eprintln!("accepting connection");
                tokio_spawn(async move {
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
}
