use colored::*;

pub(crate) fn detect_and_color_changes(
    current_orders: &[(String, &[String; 2])],
    previous_orders: &Option<&Vec<[String; 2]>>
) -> Vec<String> {
    current_orders.iter().map(|(exchange, order)| {
        let current_price: f64 = order[0].parse().unwrap();
        let amount = &order[1];

        let colorized_price = match previous_orders {
            Some(prev_orders) => {
                if let Some(prev_order) = prev_orders.iter().find(|&o| o[0] == order[0]) {
                    let previous_price: f64 = prev_order[0].parse().unwrap();

                    if current_price > previous_price {
                        current_price.to_string().green().bold()  // Price went up
                    } else if current_price < previous_price {
                        current_price.to_string().red().bold()    // Price went down
                    } else {
                        current_price.to_string().normal()        // No change
                    }
                } else {
                    current_price.to_string().blue().bold()
                }
            },
            None => {
                current_price.to_string().blue().bold()
            }
        };

        format!(
            r#"{{ "exchange": "{}", "price": {}, "amount": {} }}"#,
            exchange, colorized_price, amount
        )
    }).collect()
}


pub(crate) fn format_order_entries(orders: &[(String, &[String; 2])], changes: Vec<String>) -> String {
    changes.join(",\n        ")
}

pub(crate) fn parse_pairs(pairs: &str) -> Vec<String> {
    pairs.split(',')
        .map(|p| p.trim().to_string())
        .collect()
}
