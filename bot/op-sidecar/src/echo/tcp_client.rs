
use tokio::{net::TcpStream, io::{AsyncReadExt, AsyncWriteExt}};
use std::{vec, io::Write};
use anyhow::Result;
use std::io;
use super::make_data_package;

pub async fn start_sidecar_client(addr: String) -> Result<()> {

    let mut stream = TcpStream::connect(addr.clone()).await.unwrap();
    println!("\nWelcome to Sidecar-Console\n");
    loop {
        // Wait
        print!("[op-sidecar | {}]> ", addr.clone());
        io::stdout().flush()?;

        // Send
        let mut input = String::new();
        io::stdin().read_line(&mut input).expect("Failed to read command");
        let text = String::from(input.trim());

        if text.is_empty() {
            continue;
        }
        let should_close = text == "close" || text == "exit";
        
        let data = make_data_package(text).await;
        stream.write(&data).await?;
        stream.flush().await?;

        if should_close {
            stream.shutdown().await?;
            break;
        }

        // Receive
        // Firstly, get the header
        let mut header = [0 as u8; 4];
        stream.read(&mut header).await?;
        let length = i32::from_be_bytes(header);

        // Secondly, read all data
        let mut read_length = 0;
        let mut all_bytes: Vec<u8> = vec![];
        loop {
            let mut buf = [0 as u8; 1024];
            let size = stream.read(&mut buf).await? as i32;
            for index in 0..size {
                all_bytes.push(buf[index as usize]);
            }
            read_length += size;
            if read_length >= length {
                break;
            }
        }
        let receive_text = String::from_utf8(all_bytes)?;
        println!("{}", receive_text);
    }

    match stream.shutdown().await {
        Ok(_) => {},
        Err(_) => {}
    }

    Ok(())
}