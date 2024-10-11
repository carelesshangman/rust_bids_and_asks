mod utils;

use crate::utils::format_order_entries;
use clap::Parser;
use futures_util::{StreamExt, sink::SinkExt};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;
use serde::Deserialize;
use colored::*;

//start time: 19:03:16 @ 11.10.2024 (1728666196)
//end time: 19:51:47 @ 11.10.2024 (1728669107)
//time diff: 48.5 min

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "ethbtc")]
    pair: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let pair = args.pair.to_lowercase();

    let binance_ws_url = format!("wss://stream.binance.com:9443/ws/{}@depth20@100ms", pair);
    let bitstamp_ws_url = "wss://ws.bitstamp.net";

    let binance_url = Url::parse(&binance_ws_url).unwrap();
    let (binance_ws_stream, _) = connect_async(binance_url).await.expect("Failed to connect to Binance WebSocket");

    let bitstamp_url = Url::parse(bitstamp_ws_url).unwrap();
    let (bitstamp_ws_stream, _) = connect_async(bitstamp_url).await.expect("Failed to connect to Bitstamp WebSocket");

    let (_, mut binance_read) = binance_ws_stream.split();
    let (mut bitstamp_write, mut bitstamp_read) = bitstamp_ws_stream.split();

    let subscribe_message = format!(r#"{{
        "event": "bts:subscribe",
        "data": {{
            "channel": "order_book_{}"
        }}
    }}"#, pair);
    bitstamp_write.send(Message::Text(subscribe_message.to_string())).await.expect("Failed to subscribe to Bitstamp");

    let mut binance_order_book: Option<OrderBook> = None;
    let mut bitstamp_order_book: Option<BitstampOrderBookData> = None;

    loop {
        tokio::select! {
            Some(binance_message) = binance_read.next() => {
                if let Ok(Message::Text(text)) = binance_message {
                    if let Ok(order_book) = serde_json::from_str::<OrderBook>(&text) {
                        binance_order_book = Some(order_book);
                    }
                }
            },
            Some(bitstamp_message) = bitstamp_read.next() => {
                if let Ok(Message::Text(text)) = bitstamp_message {
                    if let Ok(order_book) = serde_json::from_str::<BitstampOrderBook>(&text) {
                        bitstamp_order_book = Some(order_book.data);
                    }
                }
            }
        }

        if let (Some(ref binance), Some(ref bitstamp)) = (&binance_order_book, &bitstamp_order_book) {
            calculate_spread_and_top10(binance, bitstamp);
        }
    }
}

#[derive(Debug, Deserialize)]
struct OrderBook {
    lastUpdateId: u64,
    bids: Vec<[String; 2]>,
    asks: Vec<[String; 2]>,
}

#[derive(Debug, Deserialize)]
struct BitstampOrderBookData {
    bids: Vec<[String; 2]>,
    asks: Vec<[String; 2]>,
}

#[derive(Debug, Deserialize)]
struct BitstampOrderBook {
    data: BitstampOrderBookData,
}

fn calculate_spread_and_top10(binance: &OrderBook, bitstamp: &BitstampOrderBookData) {
    let mut all_asks: Vec<(String, &[String; 2])> = binance.asks.iter().map(|ask| ("binance".to_string(), ask)).collect();
    let mut all_bids: Vec<(String, &[String; 2])> = binance.bids.iter().map(|bid| ("binance".to_string(), bid)).collect();

    all_asks.extend(bitstamp.asks.iter().map(|ask| ("bitstamp".to_string(), ask)));
    all_bids.extend(bitstamp.bids.iter().map(|bid| ("bitstamp".to_string(), bid)));

    all_asks.sort_by(|a, b| a.1[0].parse::<f64>().unwrap().partial_cmp(&b.1[0].parse::<f64>().unwrap()).unwrap());
    all_bids.sort_by(|a, b| b.1[0].parse::<f64>().unwrap().partial_cmp(&a.1[0].parse::<f64>().unwrap()).unwrap());

    let top_asks = &all_asks[..10.min(all_asks.len())];
    let top_bids = &all_bids[..10.min(all_bids.len())];

    let best_ask = top_asks[0].1[0].parse::<f64>().unwrap();
    let best_bid = top_bids[0].1[0].parse::<f64>().unwrap();
    let spread = best_ask - best_bid;

    println!(r#"{{
    "spread": {}{}{},
    "asks": [
        {}
    ],
    "bids": [
        {}
    ]
}}"#,
             "Spread: ".blue(),
             format!("{:.8}", spread).green().bold(),
             " (Best Ask - Best Bid)".blue(),
             format_order_entries(top_asks).red(),
             format_order_entries(top_bids).yellow()
    );
}
