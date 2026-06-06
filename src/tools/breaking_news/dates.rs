pub fn parse_pub_date_ms(value: &str) -> Option<u64> {
    let trimmed = value.trim();
    let without_weekday = match trimmed.find(',') {
        Some(comma) => trimmed[comma + 1..].trim(),
        None => trimmed,
    };
    let parts: Vec<&str> = without_weekday.split_whitespace().collect();
    if parts.len() < 5 {
        return None;
    }
    let day: i64 = parts[0].parse().ok()?;
    let month: i64 = match parts[1] {
        "Jan" => 1, "Feb" => 2, "Mar" => 3, "Apr" => 4,
        "May" => 5, "Jun" => 6, "Jul" => 7, "Aug" => 8,
        "Sep" => 9, "Oct" => 10, "Nov" => 11, "Dec" => 12,
        _ => return None,
    };
    let year: i64 = parts[2].parse().ok()?;
    let time_parts: Vec<&str> = parts[3].split(':').collect();
    if time_parts.len() < 2 {
        return None;
    }
    let hour: i64 = time_parts[0].parse().ok()?;
    let minute: i64 = time_parts[1].parse().ok()?;
    let second: i64 = time_parts.get(2).and_then(|part| part.parse().ok()).unwrap_or(0);
    let tz_offset: i64 = match parts[4] {
        "GMT" | "UT" | "UTC" | "Z" => 0,
        zone if zone.starts_with('+') || zone.starts_with('-') => {
            let sign: i64 = if zone.starts_with('-') { -1 } else { 1 };
            let magnitude: i64 = zone[1..].parse().ok()?;
            sign * ((magnitude / 100) * 3600 + (magnitude % 100) * 60)
        }
        _ => 0,
    };

    let civil_year = if month <= 2 { year - 1 } else { year };
    let era = if civil_year >= 0 {
        civil_year / 400
    } else {
        (civil_year - 399) / 400
    };
    let year_of_era = civil_year - era * 400;
    let month_index = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146097 + day_of_era - 719468;
    let total_seconds = days * 86400 + hour * 3600 + minute * 60 + second - tz_offset;
    if total_seconds < 0 {
        return None;
    }
    Some((total_seconds as u64) * 1000)
}
