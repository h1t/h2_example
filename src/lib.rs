use std::time::Duration;

pub fn calc_stat(data: &[Duration]) -> (f64, f64, f64, f64) {
    if data.is_empty() {
        return Default::default();
    }

    let (min, max, total) =
        data.iter()
            .fold((f64::MAX, f64::MIN, 0.0), |(min, max, total), item| {
                let millis = item.as_secs_f64();
                (millis.min(min), millis.max(max), millis + total)
            });

    const MILLIS_PER_SECOND: f64 = 1_000.0;
    let data_len = data.len() as f64;

    (
        min * MILLIS_PER_SECOND,
        max * MILLIS_PER_SECOND,
        total * MILLIS_PER_SECOND,
        total * MILLIS_PER_SECOND / data_len,
    )
}

#[cfg(test)]
mod tests {
    use super::calc_stat;
    use std::time::Duration;

    fn check_stat(millis: &[u64], etalon: (f64, f64, f64, f64)) {
        let data = millis
            .iter()
            .map(|&n| Duration::from_millis(n))
            .collect::<Vec<_>>();
        assert_eq!(calc_stat(&data), etalon)
    }

    #[test]
    fn calc_stat_test() {
        check_stat(&[], (0.0, 0.0, 0.0, 0.0));
        check_stat(&[0], (0.0, 0.0, 0.0, 0.0));
        check_stat(&[1], (1.0, 1.0, 1.0, 1.0));
        check_stat(&[2, 3, 1], (1.0, 3.0, 6.0, 2.0));
    }
}
