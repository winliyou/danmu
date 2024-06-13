use clap::Parser;
use log::{debug, error, info};
use reqwest::{
    header::{HeaderMap, HeaderValue, COOKIE},
    Client,
};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    fs,
    io::{Cursor, Read},
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::net::TcpStream;

use brotli::Decompressor;
use futures_util::{stream::SplitSink, SinkExt, StreamExt};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{http::Uri, Message},
    MaybeTlsStream, WebSocketStream,
};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Room ID to connect to
    #[clap(short, long)]
    room_id: u64,
    #[clap(short, long)]
    cookie_file: Option<String>,
}

#[tokio::main]
async fn main() {
    env_logger::init();
    debug!("Starting application");
    let args = Args::parse();
    let mut cookie_file =
        fs::File::open(args.cookie_file.unwrap_or_else(|| "cookie.txt".into())).unwrap();
    let mut cookie_content: String = "".into();
    let _ = cookie_file.read_to_string(&mut cookie_content);
    info!("cookie_content: {cookie_content}");
    connect_websocket(args.room_id, &cookie_content).await;
}

async fn connect_websocket(room_id: u64, cookie_content: &String) {
    let client = Client::new();
    let mut headers = HeaderMap::new();
    headers.insert(COOKIE, HeaderValue::from_str(cookie_content).unwrap());

    let url = format!(
        "https://api.live.bilibili.com/xlive/web-room/v1/index/getDanmuInfo?id={room_id}&type=0",
    );
    info!("Request URL: {url}");

    let response = match client.get(&url).headers(headers).send().await {
        Ok(res) => res.json::<Value>().await.unwrap(),
        Err(e) => {
            error!("Failed to get response: {}", e);
            return;
        }
    };

    let host = response["data"]["host_list"][0]["host"].as_str().unwrap();
    let port = response["data"]["host_list"][0]["wss_port"]
        .as_u64()
        .unwrap();
    let token = response["data"]["token"].as_str().unwrap();

    info!("Host: {}, Port: {}, Token: {}", host, port, token);
    let ws_url = format!("wss://{}:{}/sub", host, port);
    info!("WebSocket URL: {}", ws_url);

    match connect_async(Uri::from_str(&ws_url).unwrap()).await {
        Ok((ws_stream, _)) => {
            info!("Connected to WebSocket server: {}", ws_url);
            let (mut write, mut read) = ws_stream.split();

            let auth_payload = json!({
                "uid": 102624818,
                "roomid": room_id,
                "protover": 3,
                "buvid": "",
                "platform": "web",
                "type": 2,
                "key": token
            });

            send_packet(&mut write, 7, &serde_json::to_vec(&auth_payload).unwrap()).await;
            debug!("Auth packet sent");

            let heartbeat_task = tokio::spawn(async move {
                loop {
                    send_heartbeat_packet(&mut write).await;
                    tokio::time::sleep(Duration::from_secs(30)).await;
                }
            });

            let counter = Arc::new(Mutex::new(0 as u32));
            let total_counter = Arc::new(Mutex::new(0 as u64));
            let mps = Arc::new(Mutex::new(0 as u32));

            let counter_recv_clone = counter.clone();
            let recv_task = tokio::spawn(async move {
                let mut peinding_binary_data = vec![];
                let exist_id_str = Arc::new(Mutex::new(VecDeque::new()));

                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(Message::Binary(mut bin)) => {
                            debug!("Received binary data");
                            peinding_binary_data.append(&mut bin);
                            handle_binary_message(
                                &mut peinding_binary_data,
                                &exist_id_str,
                                &counter_recv_clone,
                            );
                            debug!("pending_binary_data size: {:?}", peinding_binary_data.len());
                        }
                        Ok(Message::Text(text)) => {
                            info!("Received text data: {}", text);
                        }
                        Err(e) => {
                            error!("Error receiving message: {}", e);
                        }
                        _ => {
                            debug!("Unhandled websocket message type: {:?}", msg);
                        }
                    }
                }
            });
            let mps_clone_output = mps.clone();
            let total_counter_clone_output = total_counter.clone();
            let input_task = tokio::spawn(async move {
                loop {
                    // if press s then info message count per second and total message count
                    if std::io::stdin().lock().bytes().next().unwrap().unwrap() == b's' {
                        let mps = mps_clone_output.lock().unwrap();
                        let total_counter = total_counter_clone_output.lock().unwrap();
                        println!(
                            "Messages per second: {}, total message count: {}",
                            *mps, *total_counter
                        );
                    }
                }
            });

            let counter_clone_statistic = counter.clone();
            let total_counter_clone_statistic = total_counter.clone();
            let mps_clone_statistic = mps.clone();
            let counter_task = std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(1));
                let mut counter = counter_clone_statistic.lock().unwrap();
                let mut total_counter = total_counter_clone_statistic.lock().unwrap();
                let mut mps = mps_clone_statistic.lock().unwrap();
                *mps = *counter as u32;
                *total_counter += *counter as u64;
                *counter = 0;
            });

            let _ = tokio::join!(heartbeat_task, recv_task, input_task);
            let _ = counter_task.join();
        }
        Err(e) => {
            error!("Failed to connect: {}", e);
        }
    }
}

async fn send_packet(
    write: &mut SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
    op: u32,
    payload: &[u8],
) {
    let header_size: u32 = 16;
    let body_size: u32 = payload.len() as u32;
    let total_size: u32 = header_size + body_size;

    let mut header = vec![0; header_size as usize];
    header[0..4].copy_from_slice(&total_size.to_be_bytes());
    header[4..6].copy_from_slice(&(header_size as u16).to_be_bytes());
    header[6..8].copy_from_slice(&(1 as u16).to_be_bytes());
    header[8..12].copy_from_slice(&op.to_be_bytes());
    header[12..16].copy_from_slice(&(1 as u32).to_be_bytes());

    let mut packet = header;
    packet.extend_from_slice(payload);

    if let Err(e) = write.send(Message::Binary(packet)).await {
        error!("Error sending message: {}", e);
    }
}

async fn send_heartbeat_packet(
    write: &mut SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
) {
    let payload = b"ping";
    send_packet(write, 2, payload).await;
    debug!("Sent heartbeat packet");
}

fn handle_binary_message(
    bin: &mut Vec<u8>,
    existed_id_str: &Arc<Mutex<VecDeque<String>>>,
    counter: &Arc<Mutex<u32>>,
) {
    let mut pos = 0;
    while pos < bin.len() {
        debug!("pos: {pos}, bin size: {}", bin.len());
        let packet_size = u32::from_be_bytes([bin[pos], bin[pos + 1], bin[pos + 2], bin[pos + 3]]);
        if (packet_size as usize + pos) > bin.len() {
            bin.drain(..pos);
            return;
        }
        let proto_ver = u16::from_be_bytes([bin[pos + 6], bin[pos + 7]]);
        match proto_ver {
            0 | 1 => {
                handle_uncompressed_message(bin, &mut pos, packet_size, existed_id_str, counter);
            }
            3 => {
                handle_compressed_message(bin, &mut pos, packet_size, existed_id_str, counter);
            }
            _ => error!("Unsupported protocol version: {}", proto_ver),
        }
    }
    debug!("pos: {pos}");
    bin.drain(..pos);
}

fn handle_uncompressed_message(
    bin: &mut Vec<u8>,
    pos: &mut usize,
    packet_size: u32,
    existed_id_str: &Arc<Mutex<VecDeque<String>>>,
    counter: &Arc<Mutex<u32>>,
) {
    debug!("Uncompressed message");
    debug!(
        "Received response, pos: {pos}, packet size: {packet_size}, bin: {:?}",
        &bin[(*pos)..]
    );
    let message_type = u32::from_be_bytes(bin[(*pos + 8)..(*pos + 12)].try_into().unwrap());
    match message_type {
        3 => {
            *pos += packet_size as usize + 4;
            info!("🐶陈睿，故意多搞4个字节是吧");
        }
        5 => {
            let bin_data = bin[(*pos + 16)..(*pos + packet_size as usize)].to_vec();
            let json_str = String::from_utf8(bin_data).unwrap();
            debug!("Received danmu message: {json_str}");
            if let Ok(json) = serde_json::from_str::<Value>(&json_str) {
                handle_danmu_message(&json, existed_id_str, counter);
            }
            *pos += packet_size as usize;
        }
        _ => {
            debug!("Unhandled message type: {}", message_type);
            *pos += packet_size as usize;
        }
    }
}

fn handle_compressed_message(
    bin: &mut Vec<u8>,
    pos: &mut usize,
    packet_size: u32,
    existed_id_str: &Arc<Mutex<VecDeque<String>>>,
    counter: &Arc<Mutex<u32>>,
) {
    debug!("handle_compressed_message");
    let mut decompressor = Decompressor::new(
        Cursor::new(&bin[(*pos + 16)..(*pos + packet_size as usize)]),
        1024 * 1024,
    );
    let mut decoded_data = Vec::new();
    match decompressor.read_to_end(&mut decoded_data) {
        Ok(_) => {
            *pos += packet_size as usize;
            handle_binary_message(&mut decoded_data, existed_id_str, counter);
            if decoded_data.len() > 0 {
                bin.append(&mut decoded_data);
            }
        }
        Err(e) => {
            error!("Brotli decompression error: {}", e);
        }
    }
}

fn handle_danmu_message(
    json: &Value,
    existed_id_str: &Arc<Mutex<VecDeque<String>>>,
    counter: &Arc<Mutex<u32>>,
) {
    debug!("handle_danmu_message");
    if json["cmd"] == "DANMU_MSG" {
        let info = &json["info"];
        let user_name = &info[2][1].as_str().unwrap();
        let message_detail_str = &info[0][15]["extra"].as_str().unwrap().to_string();
        let message_detail_json = serde_json::from_str::<Value>(&message_detail_str).unwrap();
        let messageid = message_detail_json["id_str"].as_str().unwrap().to_string();
        let mut message = message_detail_json["content"].as_str().unwrap().to_string();
        let reply_to = message_detail_json["reply_uname"].as_str().unwrap();
        if !reply_to.is_empty() {
            message = format!("{} 回复 {}：{}", user_name, reply_to, message);
        } else {
            message = format!("{} ：{}", user_name, message);
        }
        let mut existed_id_str = existed_id_str.lock().unwrap();
        if existed_id_str.contains(&messageid) {
            debug!("Duplicated message from user ID: {}", messageid);
            return;
        }
        if existed_id_str.len() > 30 {
            existed_id_str.pop_front();
        }
        existed_id_str.push_back(messageid.clone());

        let mut counter = counter.lock().unwrap();
        *counter += 1;

        println!("{}", message);
    } else {
        info!("unhandle danmu message: {}", json);
    }
}
