pub fn format_bytes(value: Option<i64>, signed: bool) -> String {
    let Some(mut value) = value else {
        return "unavailable".to_string();
    };

    let sign = if signed {
        if value >= 0 {
            "+"
        } else {
            value = value.abs();
            "-"
        }
    } else {
        ""
    };

    let units = ["B", "K", "M", "G", "T", "P"];
    let mut amount = value as f64;
    for unit in units {
        if amount < 1024.0 || unit == "P" {
            let mut rendered = format!("{amount:.1}");
            if rendered.ends_with(".0") {
                rendered.truncate(rendered.len() - 2);
            }
            return format!("{sign}{rendered}{unit}");
        }
        amount /= 1024.0;
    }

    format!("{sign}{value}B")
}
