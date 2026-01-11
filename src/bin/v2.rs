use std::{
    collections::{BTreeMap, HashMap},
    fs::File,
    io::{BufRead, BufReader},
};

fn main() {
    let f = File::open("data/measurements.txt").expect("file should exist");
    let f = BufReader::new(f);

    // (min, max, sum, count)
    let mut stats: HashMap<String, (f64, f64, f64, usize)> = HashMap::with_capacity(10_000);

    for line in f.lines().map_while(Result::ok) {
        let (station, temperature) = line.split_once(";").expect("delimiter should be ;");
        let temperature = temperature
            .parse::<f64>()
            .expect("should be a valid floating point");

        let stats = stats
            .entry(station.to_string())
            .or_insert((f64::MAX, f64::MIN, 0.0, 0));

        stats.0 = stats.0.min(temperature);
        stats.1 = stats.1.max(temperature);
        stats.2 += temperature;
        stats.3 += 1;
    }

    let stats = BTreeMap::from_iter(stats);
    let mut stats = stats.into_iter().peekable();

    print!("{{");
    while let Some((station, (min, max, sum, count))) = stats.next() {
        print!("{station}={min}/{:.1}/{max}", sum / (count as f64));
        if stats.peek().is_some() {
            print!(", ");
        }
    }
    print!("}}")
}
