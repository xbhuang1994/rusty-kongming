use std::net::SocketAddr;

use anyhow::Result;
use tokio::{net::{TcpListener, TcpStream}, io::{AsyncReadExt, AsyncWriteExt}};
use log::{info, error};
use serde_json;
use serde::{Deserialize, Serialize};
use super::make_data_package;
use runtime::dynamic_config;


#[derive(Debug, Deserialize, Serialize)]
struct Resp {
    code: i32,
    msg: Option<String>,
    data: Option<String>,
}

async fn handle_config_command(args: &Vec<String>) -> Resp {

    if args.len() < 2 {
        return Resp {
            code: 1i32,
            msg: Some("Invalid Arguments".to_string()),
            data: None,
        };
    }
    
    let arg_main = args[1].clone();
    let mut data = String::from("");
    let mut code = 0i32;
    let mut msg = String::from("success");
    if "list" == arg_main {
        let config = dynamic_config::get_all_config();
        data = serde_json::to_string(&config).unwrap();
    } else if "get" == arg_main {
        if args.len() < 3 {
            code = 1i32;
            msg = String::from("Not Config Key Given");
        } else {
            data = dynamic_config::get_config(args[2].clone());
        }
    } else if "set" == arg_main {
        if args.len() < 4 {
            code = 1i32;
            msg = String::from("Require Config Key And Value");
        } else {
            let result = dynamic_config::set_config(args[2].clone(), args[3].clone());
            match result {
                Ok(_) => {},
                Err(e) => {
                    code = 1i32;
                    msg = String::from("Failed Set Config");
                }
            }
        }
    }

    return Resp {
        code: code,
        msg: Some(msg),
        data: Some(data),
    };
}

async fn process_receive_command(command_line: String) -> Resp {

    let parts = command_line.split(" ");
    let parts: Vec<&str> = parts.collect();
    let mut args: Vec<String> = vec![];
    parts.iter().for_each(|s| args.push(String::from(*s)));

    if "config" == parts[0] {
        return handle_config_command(&args).await;
    } else {
        return Resp {
            code: 1i32,
            msg: Some("Invalid Command".to_string()),
            data: None,
        };
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
