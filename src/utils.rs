pub(crate) fn format_order_entries(orders: &[(String, &[String; 2])]) -> String {
    orders.iter()
        .map(|(exchange, order)| format!(r#"{{ "exchange": "{}", "price": {}, "amount": {} }}"#,
                                         exchange, order[0], order[1]))
        .collect::<Vec<String>>()
        .join(",\n        ")
}