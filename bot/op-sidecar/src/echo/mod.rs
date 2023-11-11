pub mod tcp_server;
pub mod tcp_client;

/// Make tcp package with text want to send
async fn make_data_package(text: String) -> Vec<u8> {

    let text_bytes = text.as_bytes();
    let length = text_bytes.len() as i32;
    let mut data: Vec<u8> = vec![];
    data.extend(length.to_be_bytes());
    data.extend(text_bytes.clone());
    data
}