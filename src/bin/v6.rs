use std::{
    collections::{BTreeMap, HashMap},
    f64,
    fs::File,
    hash::{BuildHasher, Hasher},
    io::{BufRead, BufReader},
};

const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

struct Fnv1aHasher {
    hash: u64,
}

impl Default for Fnv1aHasher {
    fn default() -> Self {
        Self {
            hash: FNV_OFFSET_BASIS,
        }
    }
}

impl Hasher for Fnv1aHasher {
    #[inline(always)]
    fn finish(&self) -> u64 {
        self.hash
    }

    #[inline(always)]
    fn write(&mut self, bytes: &[u8]) {
        let mut h = self.hash;
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(FNV_PRIME);
        }
        self.hash = h;
    }
}

struct Fnv1aHashBuilder;
impl BuildHasher for Fnv1aHashBuilder {
    type Hasher = Fnv1aHasher;

    fn build_hasher(&self) -> Self::Hasher {
        Fnv1aHasher::default()
    }
}

fn main() {
    let f = File::open("data/measurements.txt").expect("file should exist");
    let f = BufReader::new(f);

    // (min, max, sum, count)
    let mut stats: HashMap<Vec<u8>, (i32, i32, i32, usize), Fnv1aHashBuilder> =
        HashMap::with_capacity_and_hasher(10_000, Fnv1aHashBuilder);

    for line in f.split(b'\n').map_while(Result::ok) {
        let mut fields = line.splitn(2, |c| *c == b';');
        let station = fields.next().unwrap();
        let temperature = fields.next().unwrap();
        let temperature = parse_temperature(temperature);

        let stats = match stats.get_mut(station) {
            Some(stats) => stats,
            None => stats
                .entry(station.to_vec())
                .or_insert((i32::MAX, i32::MIN, 0, 0)),
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
        print!(
            "{station}={:.1}/{:.1}/{:.1}",
            min as f64 / 10.0,
            sum as f64 / 10.0 / count as f64,
            max as f64 / 10.0
        );
        if stats.peek().is_some() {
            print!(", ");
        }
    }
    print!("}}")
}

fn parse_temperature(t: &[u8]) -> i32 {
    // rule states that file is valid floating point with 1 decimal place
    let mut signed = 1;
    let mut n = 0;
    for &b in t {
        match b {
            b'-' => signed = -1,
            b'.' => {}
            _ => n = n * 10 + (b - b'0') as i32,
        }
    }
    signed * n
}
