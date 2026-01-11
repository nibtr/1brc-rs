use std::{
    collections::{BTreeMap, HashMap},
    f64,
    fs::File,
    io::{BufRead, BufReader},
};

fn main() {
    let f = File::open("data/measurements.txt").expect("file should exist");
    let f = BufReader::new(f);

    // (min, max, sum, count)
    let mut stats: HashMap<Vec<u8>, (f64, f64, f64, usize)> = HashMap::with_capacity(10_000);

    for line in f.split(b'\n').map_while(Result::ok) {
        let mut fields = line.splitn(2, |c| *c == b';');
        let station = fields.next().unwrap();
        let temperature = fields.next().unwrap();
        let temperature: f64 = unsafe { std::str::from_utf8_unchecked(temperature) }
            .parse()
            .unwrap();

        let stats = match stats.get_mut(station) {
            Some(stats) => stats,
            None => stats
                .entry(station.to_vec())
                .or_insert((f64::MAX, f64::MIN, 0.0, 0)),
        };

        stats.0 = stats.0.min(temperature);
        stats.1 = stats.1.max(temperature);
        stats.2 += temperature;
        stats.3 += 1;
    }

    let stats = BTreeMap::from_iter(
        stats
            .into_iter()
            .map(|(station, stats)| (unsafe { String::from_utf8_unchecked(station) }, stats)),
    );
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
