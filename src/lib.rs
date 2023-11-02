use std::time::Duration;

pub fn calc_stat(data: &[Duration]) -> (f64, f64, f64) {
    let (min, max, total) =
        data.iter()
            .fold((f64::MAX, f64::MIN, 0.0), |(min, max, total), item| {
                let millis = item.as_secs_f64();
                (millis.min(min), millis.max(max), millis + total)
            });

    (min * 1_000.0, max * 1_000.0, total * 1_000.0)
}
