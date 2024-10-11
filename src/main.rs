mod utils;

use crate::utils::{format_order_entries, parse_pairs, detect_and_color_changes};
use clap::Parser;
use futures_util::{StreamExt, sink::SinkExt};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;
use serde::Deserialize;
use colored::*;

// run with cargo run -- --pairs ethbtc,btcusdt
// add other pairs as flags if needed in this format: --pairs pair1,pair2,pair3,...

// commit 1
// start time: 19:03:16 @ 11.10.2024 (1728666196)
// end time: 19:51:47 @ 11.10.2024 (1728669107)
// time diff: 48.5 min

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The currency pairs to subscribe to (comma separated, e.g. ethbtc,btcusdt)
    #[arg(short, long, default_value = "ethbtc")]
    pairs: String,
}

#[derive(Debug)]
enum PreviousOrder {
    Binance(Option<OrderBook>),
    Bitstamp(Option<BitstampOrderBookData>),
}


#[tokio::main]
async fn main() {
    let args = Args::parse();
    let pairs = parse_pairs(&args.pairs);

    let mut binance_streams = vec![];
    let mut bitstamp_streams = vec![];

    for pair in &pairs {
        let binance_ws_url = format!("wss://stream.binance.com:9443/ws/{}@depth20@100ms", pair);
        let bitstamp_ws_url = "wss://ws.bitstamp.net";

        let binance_url = Url::parse(&binance_ws_url).unwrap();
        let (binance_ws_stream, _) = connect_async(binance_url).await.expect("Failed to connect to Binance WebSocket");

        let bitstamp_url = Url::parse(bitstamp_ws_url).unwrap();
        let (bitstamp_ws_stream, _) = connect_async(bitstamp_url).await.expect("Failed to connect to Bitstamp WebSocket");

        let (_, binance_read) = binance_ws_stream.split();
        let (mut bitstamp_write, bitstamp_read) = bitstamp_ws_stream.split();

        let subscribe_message = format!(r#"{{
            "event": "bts:subscribe",
            "data": {{"channel": "order_book_{}"}}
        }}"#, pair);
        bitstamp_write.send(Message::Text(subscribe_message.to_string())).await.expect("Failed to subscribe to Bitstamp");

        binance_streams.push((pair.clone(), binance_read));
        bitstamp_streams.push((pair.clone(), bitstamp_read));
    }

    let mut previous_binance_order_books = vec![];
    let mut previous_bitstamp_order_books = vec![]; // Track previous Bitstamp data

    for _ in 0..pairs.len() {
        previous_binance_order_books.push(None);
        previous_bitstamp_order_books.push(None); // Initialize for each pair
    }

    loop {
        for ((pair, binance_read), (_, bitstamp_read)) in binance_streams.iter_mut().zip(bitstamp_streams.iter_mut()) {
            let idx = pairs.iter().position(|p| p == pair).unwrap();

            tokio::select! {
                Some(binance_message) = binance_read.next() => {
                    if let Ok(Message::Text(text)) = binance_message {
                        if let Ok(order_book) = serde_json::from_str::<OrderBook>(&text) {
                            let previous = PreviousOrder::Binance(previous_binance_order_books[idx].clone());
                            previous_binance_order_books[idx] = Some(order_book.clone());
                            calculate_spread_and_top10(pair, &Some(order_book), &previous, &previous_bitstamp_order_books[idx]);
                        }
                    }
                },
                Some(bitstamp_message) = bitstamp_read.next() => {
                    if let Ok(Message::Text(text)) = bitstamp_message {
                        if let Ok(order_book) = serde_json::from_str::<BitstampOrderBook>(&text) {
                            let previous = PreviousOrder::Bitstamp(previous_bitstamp_order_books[idx].clone());
                            previous_bitstamp_order_books[idx] = Some(order_book.data.clone());
                            calculate_spread_and_top10(pair, &previous_binance_order_books[idx], &previous, &Some(order_book.data));
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
struct OrderBook {
    lastUpdateId: u64,
    bids: Vec<[String; 2]>,
    asks: Vec<[String; 2]>,
}

#[derive(Debug, Deserialize, Clone)]
struct BitstampOrderBookData {
    bids: Vec<[String; 2]>,
    asks: Vec<[String; 2]>,
}

#[derive(Debug, Deserialize)]
struct BitstampOrderBook {
    data: BitstampOrderBookData,
}

fn calculate_spread_and_top10(
    pair: &str,
    binance: &Option<OrderBook>,
    previous_binance: &PreviousOrder,
    bitstamp: &Option<BitstampOrderBookData>
) {
    if let Some(binance) = binance {
        let mut all_asks: Vec<(String, &[String; 2])> = binance.asks.iter().map(|ask| ("binance".to_string(), ask)).collect();
        let mut all_bids: Vec<(String, &[String; 2])> = binance.bids.iter().map(|bid| ("binance".to_string(), bid)).collect();

        if let Some(bitstamp_data) = bitstamp {
            all_asks.extend(bitstamp_data.asks.iter().map(|ask| ("bitstamp".to_string(), ask)));
            all_bids.extend(bitstamp_data.bids.iter().map(|bid| ("bitstamp".to_string(), bid)));
        }

        // Sort asks and bids
        all_asks.sort_by(|a, b| a.1[0].parse::<f64>().unwrap().partial_cmp(&b.1[0].parse::<f64>().unwrap()).unwrap());
        all_bids.sort_by(|a, b| b.1[0].parse::<f64>().unwrap().partial_cmp(&a.1[0].parse::<f64>().unwrap()).unwrap());

        let top_asks = &all_asks[..10.min(all_asks.len())];
        let top_bids = &all_bids[..10.min(all_bids.len())];

        let best_ask = top_asks[0].1[0].parse::<f64>().unwrap();
        let best_bid = top_bids[0].1[0].parse::<f64>().unwrap();
        let spread = best_ask - best_bid;

        // Print spread information
        println!(r#"{{
    "pair": "{}",
    "spread": {}{}{},"#,
                 pair.magenta(),
                 "Spread: ".blue(),
                 format!("{:.8}", spread).green().bold(),
                 " (Best Ask - Best Bid)".blue()
        );

        // Format and display the asks
        let previous_asks = match previous_binance {
            PreviousOrder::Binance(prev) => prev.as_ref().map(|p| &p.asks),
            PreviousOrder::Bitstamp(_) => None,
        };
        println!(r#""asks": [{}],"#,
                 format_order_entries(top_asks, detect_and_color_changes(top_asks, &previous_asks))
        );

        // Format and display the bids
        let previous_bids = match previous_binance {
            PreviousOrder::Binance(prev) => prev.as_ref().map(|p| &p.bids),
            PreviousOrder::Bitstamp(_) => None,
        };
        println!(r#""bids": [{}]
}}"#,
                 format_order_entries(top_bids, detect_and_color_changes(top_bids, &previous_bids))
        );
    }
}

