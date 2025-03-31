use crate::socket::TcpStream;

pub struct BufRead<'a> {
    buffer: Vec<u8>,
    tcp_stream: &'a TcpStream
}

impl<'a> BufRead<'a> {
    pub fn new(tcp_stream: &'a TcpStream) -> Self {
        Self {
            buffer: Vec::new(),
            tcp_stream
        }
    }

    pub async fn read_line(&mut self) -> Result<String, String> {
        loop {
            let mut buf = [0; 1];

            match self.tcp_stream.read_bytes(&mut buf).await {
                Ok(0) => return Err("EOF".to_string()),
                Ok(_) => {
                    self.buffer.push(buf[0]);
                    if buf[0] == b'\n' {
                        let line = String::from_utf8_lossy(&self.buffer).to_string();
                        self.buffer.clear();
                        return Ok(line);
                    }
                },
                Err(e) => return Err(e)
            }
        }
    }
}
