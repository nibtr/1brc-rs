use std::{
    collections::{BTreeMap, HashMap},
    f64,
    fs::File,
    hash::{BuildHasher, Hasher},
    io::{self},
    os::fd::AsRawFd,
    thread,
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
    let map = mmap(f).unwrap();
    let n_workers = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    let chunks = chunk_file(map, n_workers);
    let mut handles = Vec::with_capacity(chunks.len());

    for (start, end) in chunks {
        let map = map; // only copy pointer not data
        handles.push(thread::spawn(move || process_chunk(&map[start..end])));
    }

    // (min, max, sum, count)
    let mut stats: HashMap<&[u8], (i32, i32, i32, usize), Fnv1aHashBuilder> =
        HashMap::with_capacity_and_hasher(10_000, Fnv1aHashBuilder);

    for h in handles {
        let local = h.join().unwrap();

        for (station, (min, max, sum, count)) in local {
            let entry = match stats.get_mut(&station) {
                Some(stats) => stats,
                None => stats.entry(station).or_insert((i32::MAX, i32::MIN, 0, 0)),
            };

            entry.0 = entry.0.min(min);
            entry.1 = entry.1.max(max);
            entry.2 += sum;
            entry.3 += count;
        }
    }

    let stats = BTreeMap::from_iter(
        stats
            .into_iter()
            .map(|(station, stats)| (unsafe { std::str::from_utf8_unchecked(station) }, stats)),
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

fn mmap<'a>(f: File) -> Result<&'a [u8], io::Error> {
    let len = f.metadata()?.len();

    unsafe {
        let ptr = libc::mmap(
            std::ptr::null_mut(),
            len as libc::size_t,
            libc::PROT_READ,
            libc::MAP_PRIVATE,
            f.as_raw_fd(),
            0,
        );

        if ptr == libc::MAP_FAILED {
            Err(io::Error::last_os_error())
        } else {
            if libc::madvise(ptr, len as libc::size_t, libc::MADV_SEQUENTIAL) != 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(std::slice::from_raw_parts(ptr as *const u8, len as usize))
            }
        }
    }
}

fn chunk_file(map: &[u8], n_workers: usize) -> Vec<(usize, usize)> {
    let mut chunks = Vec::with_capacity(n_workers);
    let file_len = map.len();
    let base = file_len / n_workers;
    let mut start: usize = 0;

    for _ in 0..n_workers - 1 {
        let mut end = start + base;
        if end >= file_len {
            break;
        }

        while end < file_len && map[end] != b'\n' {
            end += 1;
        }

        chunks.push((start, end));
        start = end + 1;
    }

    chunks.push((start, file_len));
    chunks
}

fn process_chunk(chunk: &[u8]) -> HashMap<&[u8], (i32, i32, i32, usize), Fnv1aHashBuilder> {
    // (min, max, sum, count)
    let mut stats: HashMap<&[u8], (i32, i32, i32, usize), Fnv1aHashBuilder> =
        HashMap::with_capacity_and_hasher(2048, Fnv1aHashBuilder);

    let mut at = 0;

    while at < chunk.len() {
        let rest = &chunk[at..];
        let nl_ptr = unsafe {
            libc::memchr(
                rest.as_ptr() as *const libc::c_void,
                b'\n' as libc::c_int,
                rest.len(),
            )
        };

        let line = if nl_ptr.is_null() {
            rest
        } else {
            let len = unsafe { (nl_ptr as *const u8).offset_from(rest.as_ptr()) } as usize;
            &rest[..len]
        };

        at += line.len() + 1;

        if line.is_empty() {
            continue;
        }

        let mut fields = line.splitn(2, |c| *c == b';');
        let station = fields.next().unwrap();
        let temperature = parse_temperature(fields.next().unwrap());

        let entry = match stats.get_mut(station) {
            Some(stats) => stats,
            None => stats.entry(station).or_insert((i32::MAX, i32::MIN, 0, 0)),
        };

        entry.0 = entry.0.min(temperature);
        entry.1 = entry.1.max(temperature);
        entry.2 += temperature;
        entry.3 += 1;
    }

    stats
}
