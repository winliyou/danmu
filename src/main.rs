use clap::Parser;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn fetch_liveroom_chat_url(
    room_id: u64,
) -> Result<BiliBiliLiveRoomConfig, Box<dyn std::error::Error>> {
    let client = Client::new();
    let url = format!(
        "https://api.live.bilibili.com/room/v1/Danmu/getConf?room_id={}",
        room_id
    );

    let response = client.get(&url).send().await?;
    println!("response code: {}", response.status());
    return Ok(response.json().await?);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let room_config = fetch_liveroom_chat_url(args.room_id.parse()?).await?;
    println!("room_config: {:?}", room_config);

    // 连接到 WebSocket 服务器
    let mut stream = tokio::net::TcpStream::connect((
        room_config.data.host_server_list[0].host.clone(),
        room_config.data.host_server_list[0].ws_port,
    ))
    .await?;
    println!("Connected to WebSocket server");

    // 发送 WebSocket 握手请求
    let handshake_request = format!(
        "GET ws://{}/sub HTTP/1.1\r\n\
         Host: {}\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {}\r\n\
         Sec-WebSocket-Version: 13\r\n\
         Upgrade: websocket\r\n\r\n",
        room_config.data.host_server_list[0].host,
        room_config.data.host_server_list[0].host,
        room_config.data.token.clone()
    );

    println!("handshake_request: {}", handshake_request);
    stream.write_all(handshake_request.as_bytes()).await?;

    // 接收服务器的响应
    let mut buffer = [0; 1024];
    let n = stream.read(&mut buffer).await?;
    let response = String::from_utf8_lossy(&buffer[..n]);
    println!("Server response:\n{}", response);
    let exit_handle_danmu = false;
    while !exit_handle_danmu {
        let n = stream.read(&mut buffer).await?;
        let response = String::from_utf8_lossy(&buffer[..n]);
        println!(
            "got response from server, length: {} , content:\n{}",
            n, response
        );
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct Danmaku {
    user_name: String,
    content: String,
}

#[derive(Debug, Deserialize)]
pub struct BiliBiliLiveRoomConfig {
    pub code: i32,
    pub msg: String,
    pub message: String,
    pub data: BiliBiliLiveRoomConfigData,
}

#[derive(Debug, Deserialize)]
pub struct BiliBiliLiveRoomConfigData {
    pub refresh_row_factor: f32,
    pub refresh_rate: u32,
    pub max_delay: u32,
    pub port: u16,
    pub host: String,
    pub host_server_list: Vec<BiliBiliHostServer>,
    pub server_list: Vec<BiliBiliServer>,
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct BiliBiliHostServer {
    pub host: String,
    pub port: u16,
    pub wss_port: u16,
    pub ws_port: u16,
}

#[derive(Debug, Deserialize)]
pub struct BiliBiliServer {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Serialize)]
pub struct BiliBiliWebSocketMessage {
    pub package_size: u32,
    pub header_size: u16,
    pub protocol_version: u16,
    pub operation: u32,
    pub message_id: u32,
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    room_id: String,
}
