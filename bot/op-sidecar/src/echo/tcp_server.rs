use std::net::SocketAddr;

use anyhow::Result;
use tokio::{net::{TcpListener, TcpStream}, io::{AsyncReadExt, AsyncWriteExt}};
use log::{info, error};
use serde_json;
use serde::{Deserialize, Serialize};
use super::make_data_package;


#[derive(Debug, Deserialize, Serialize)]
struct Resp {
    code: i32,
    msg: Option<String>,
    data: Option<String>,
}

async fn process_receive_command(command: String) -> Resp {

    // todo!("do process");
    Resp {
        code: 0i32,
        msg: Some("success".to_string()),
        data: Some(command.clone()),
    }
}

/// Process tcp Read and Write
async fn handle_stream(mut stream: TcpStream, pair_addr: SocketAddr) -> Result<()> {
    
    stream.set_nodelay(true)?;
    info!("Get Connection With: {:?}", pair_addr.clone());

    loop {
        // Read
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
        let receive_command = String::from_utf8(all_bytes)?;
        info!("Server Receive Command: {}", receive_command);

        if receive_command == "close" || receive_command == "exit" {
            info!("Close Connection With: {:?}", pair_addr);
            break;
        }

        // Write
        // todo
        let resp = process_receive_command(receive_command).await;
        let echo = serde_json::to_string_pretty(&resp).unwrap();
        
        // let echo = echo.replace("\"", "'");
        let data = make_data_package(echo).await;

        stream.write(&data).await?;
        stream.flush().await?;
    }

    Ok(())
}

/// startup sidecar tcp service
pub async fn start_sidecar_server() -> Result<()> {

    let addr = String::from("127.0.0.1:12321");
    return start_sidecar_server_at_address(addr).await;
}

/// startup sidecar tcp service
pub async fn start_sidecar_server_at_address(addr: String) -> Result<()> {

    tokio::spawn(async move {
        let listener = TcpListener::bind(addr.clone()).await.unwrap();
        println!("Sidecar server listen at {:?}", addr);

        loop {
            let (stream, pair_addr) = match listener.accept().await {
                Ok((stream, pair_addr)) => {
                    (stream, pair_addr)
                },
                Err(e) => {
                    error!("Accept Error: {:?}", e);
                    continue;
                }
            };
            
            tokio::spawn(async move {
                match handle_stream(stream, pair_addr).await {
                    Ok(_) => {},
                    Err(e) => {
                        error!("Failed To Handle Stream: {:?}", e);
                    }
                }
            });
        }
    });
    Ok(())
}
