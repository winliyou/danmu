use clap::Parser;
use log::{debug, error, info};
use reqwest::{
    header::{HeaderMap, HeaderValue, COOKIE},
    Client,
};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
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

// WebSocket连接
async fn connect_websocket(room_id: u64) {
    let client = Client::new();
    let cookie_str = "";
    unimplemented!("复制你的b站cookie到上面这个变量");
    let mut headers = HeaderMap::new();
    headers.insert(COOKIE, HeaderValue::from_str(&cookie_str).unwrap());
    let url = format!(
        "https://api.live.bilibili.com/xlive/web-room/v1/index/getDanmuInfo?id={room_id}&type=0",
    );
    info!("url: {url}");
    let response = client
        .get(url)
        .headers(headers)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let host = &response["data"]["host_list"][0]["host"];
    let port = &response["data"]["host_list"][0]["wss_port"];
    let token = &response["data"]["token"];

    info!(
        "host: {}, port: {}, token: {}",
        host.as_str().unwrap(),
        port.as_u64().unwrap(),
        // token.as_str().unwrap()
        token
    );
    let url = format!(
        "wss://{}:{}/sub",
        host.as_str().unwrap(),
        port.as_u64().unwrap()
    );
    info!("url: {}", url);
    match connect_async(Uri::from_str(&url).unwrap()).await {
        Ok((ws_stream, _)) => {
            info!("success to connect wss server: {}", url);
            let (mut write, mut read) = ws_stream.split();
            // 发送认证信息
            debug!("ready to send auth packet");
            let auth = json!(
                {
                "uid": 102624818,
                "roomid": room_id,
                "protover": 3,
                // "buvid": "4AF0ACD7-9551-A8C0-A266-3008F272EE8833116infoc".to_string(),
                "buvid": "",
                "platform": "web",
                "type": 2,
                "key": token
            });
            let payload = serde_json::to_string(&auth).unwrap();
            send_packet(&mut write, 7, &payload.as_bytes()).await;
            debug!("auth packet sent");
            // 心跳逻辑可以在这里添加
            let heartbeat_task = tokio::spawn(async move {
                loop {
                    send_heartbeat_packet(&mut write).await;
                    tokio::time::sleep(Duration::from_secs(30)).await;
                }
            });
            let mut totalcounter: u32 = 0;
            let counter = Arc::new(Mutex::new(0 as u32));
            let counter_clone = counter.clone();
            let recv_task = tokio::spawn(async move {
                let mut pending_data: Vec<u8> = vec![];
                let exist_id_str: Arc<Mutex<VecDeque<String>>> =
                    Arc::new(Mutex::new(VecDeque::new()));
                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(Message::Binary(mut bin)) => {
                            debug!("recv binary data");
                            pending_data.append(&mut bin);
                            handle_binary_message(&mut pending_data, &exist_id_str, &counter_clone)
                                .await;
                        }
                        Ok(Message::Text(text)) => {
                            info!("recv text data: {}", text);
                        }
                        Err(e) => {
                            error!("Error receiving message: {}", e);
                        }
                        _ => {
                            info!("other message type: {msg:?}");
                        }
                    }
                }
            });
            let counter_thread_clone = counter.clone();
            let counter_task = std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_millis(1000));
                let mut current_counter = counter_thread_clone.lock().unwrap();
                totalcounter += *current_counter;
                info!("====== 每秒弹幕数目: {}", current_counter);
                info!("======= 总弹幕数目: {totalcounter}");
                *current_counter = 0;
            });
            let _ = tokio::join!(heartbeat_task, recv_task);
            let _ = counter_task.join();
        }
        Err(e) => {
            error!("Failed to connect: {}", e);
        }
    }
}

// 发送二进制数据包
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

// 发送心跳包
async fn send_heartbeat_packet(
    write: &mut SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
) {
    let payload = b"ping";
    send_packet(write, 2, payload).await;
    debug!("Sent heartbeat packet");
}

// 处理解码后的二进制消息
async fn handle_binary_message(
    bin: &mut Vec<u8>,
    existed_id_str: &Arc<Mutex<VecDeque<String>>>,
    counter: &Arc<Mutex<u32>>,
) {
    // 获取协议版本
    let mut pos = 0;
    while pos < bin.len() {
        let packet_size =
            u32::from_be_bytes([bin[pos + 0], bin[pos + 1], bin[pos + 2], bin[pos + 3]]);
        debug!("check packet_size: {packet_size} , pos: {pos} , bin[0]: {}, bin[1]: {}, bin[2]: {},  bin[3]: {}",bin[pos + 0],bin[pos+1],bin[pos+2],bin[pos+3]);
        if (packet_size as usize + pos) > bin.len() {
            debug!(
                "need size: {} , bin.len: {}",
                packet_size as usize + pos,
                bin.len()
            );
            bin.drain(..pos);
            debug!("need more data");
            break;
        }
        let proto_ver = u16::from_be_bytes([bin[pos + 6], bin[pos + 7]]);

        // 根据协议版本选择处理逻辑
        match proto_ver {
            0 | 1 => {
                // 无压缩或非Brotli压缩，这里假设直接是JSON，简化处理
                let message_type = u32::from_be_bytes(bin[pos + 8..pos + 12].try_into().unwrap());
                debug!("message_type: {message_type}");
                match message_type {
                    3 => {
                        debug!(
                            "瞎整的心跳回复包应该的位置: {}",
                            pos + packet_size as usize + 4
                        );
                        if bin.len() >= pos + packet_size as usize + 4 {
                            pos += packet_size as usize + 4;
                            debug!("Heartbeat response received , 🐶陈睿故意瞎整协议是吧.");
                            continue;
                        } else {
                            debug!("need more data for heartbeat response");
                            bin.drain(..pos);
                            break;
                        }
                    }
                    5 => {
                        let bin_slice = &bin[pos..(pos + packet_size as usize)];
                        debug!(
                            "pos: {pos} , packet_size: {packet_size} ,  bin_slice: {bin_slice:?}"
                        );
                        // 假设其他操作是JSON数据
                        let bin_data = bin[(pos + 16)..(pos + packet_size as usize)].to_vec();
                        let json_str = String::from_utf8(bin_data.clone()).unwrap();
                        let json = serde_json::from_str::<Value>(&json_str);
                        match json {
                            Ok(json_success) => {
                                if let Some(cmd_type) = json_success["cmd"].as_str() {
                                    match cmd_type {
                                        "WATCHED_CHANGE" => {
                                            debug!(
                                                "看过的人数: {}",
                                                json_success["data"]["num"].as_u64().unwrap()
                                            );
                                        }
                                        "ONLINE_RANK_V2" => {}
                                        "DANMU_MSG" => {
                                            debug!(
                                                "json content: {json_str} , header: {:?}",
                                                &bin[pos..(pos + 16)]
                                            );
                                            let dm_sender =
                                                json_success["info"][2][1].as_str().unwrap();
                                            let dm_message =
                                                json_success["info"][1].as_str().unwrap();

                                            if let Some(extra_json_str) =
                                                json_success["info"][0][15]["extra"].as_str()
                                            {
                                                if let Ok(extra_json) =
                                                    serde_json::from_str::<Value>(extra_json_str)
                                                {
                                                    let id_str =
                                                        extra_json["id_str"].as_str().unwrap();
                                                    debug!("id_str: {id_str}");
                                                    let mut exist_id_str_obj =
                                                        existed_id_str.lock().unwrap();
                                                    if exist_id_str_obj.contains(&id_str.into()) {
                                                        debug!(
                                                            "existed id_str, existed_id_str: {:?}",
                                                            exist_id_str_obj
                                                        );
                                                    } else {
                                                        exist_id_str_obj.push_back(id_str.into());
                                                        info!(
                                                            "弹幕, {dm_sender} 说:  {dm_message}"
                                                        );
                                                        if exist_id_str_obj.len() > 10000 {
                                                            exist_id_str_obj.pop_front();
                                                        }
                                                        let mut current_counter =
                                                            counter.lock().unwrap();
                                                        *current_counter += 1;
                                                    }
                                                    pos += packet_size as usize;
                                                    continue;
                                                }
                                            }

                                            info!("弹幕, {dm_sender} 说:  {dm_message}");
                                        }
                                        _ => {
                                            debug!("不感兴趣的message type: {}", cmd_type);
                                        }
                                    }
                                }
                            }
                            Err(_) => debug!("not json data: {:?}", json_str),
                        }
                    }
                    _ => {
                        debug!("不感兴趣的消息类型")
                    }
                }
            }
            3 => {
                // Brotli解压
                let mut decompressor = Decompressor::new(
                    Cursor::new(&bin[(pos + 16)..(pos + packet_size as usize)]),
                    1024 * 1024,
                );
                let mut decoded_data = Vec::new();
                match decompressor.read_to_end(&mut decoded_data) {
                    Ok(_) => {
                        // let mut ttt = existed_id_str.clone();
                        // 解压成功，递归处理解压后的数据
                        Box::pin(async move {
                            handle_binary_message(&mut decoded_data, existed_id_str, counter).await;
                        })
                        .await;
                    }
                    Err(e) => {
                        error!("Brotli decompression error: {}", e);
                    }
                }
            }
            _ => {
                error!("Unsupported protocol version: {}", proto_ver);
            }
        }
        pos += packet_size as usize;
        debug!("after parse message, pos: {pos} , packet_size: {packet_size}");
    }
}

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Room ID to connect to
    #[clap(short, long)]
    room_id: u64,
}

#[tokio::main]
async fn main() {
    env_logger::init();
    debug!("ready to start");
    let args = Args::parse();
    connect_websocket(args.room_id).await;
}
