use std::{
    f64,
    fs::File,
    io::{self},
    os::fd::AsRawFd,
};

const HASH_TABLE_SIZE: usize = 1 << 17;

#[derive(Copy, Clone)]
struct Entry {
    first_word: usize,
    second_word: usize,
    name_len: usize,
    name_offset: usize, // offset in mmap
    min: i32,
    max: i32,
    sum: i32,
    count: usize,
}

fn main() {
    let f = File::open("data/measurements.txt").expect("file should exist");
    let map = mmap(f).unwrap();

    let mut entries: Vec<Option<Entry>> = vec![None; HASH_TABLE_SIZE];

    let mut at = 0;
    while at < map.len() {
        let rest = &map[at..];
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
            break;
        }

        // find station and temperature
        let mut i = 0;
        while line[i] != b';' {
            i += 1;
        }
        let station = &line[..i];
        let temperature = &line[(i + 1)..];
        let temperature = parse_temperature(temperature);

        insert_or_update(&mut entries, station, temperature, map);
    }

    let mut results: Vec<_> = entries.into_iter().flatten().collect();
    results.sort_by_key(|e| {
        let name = &map[e.name_offset..(e.name_offset + e.name_len)];
        name.to_vec()
    });

    print!("{{");
    for (i, entry) in results.iter().enumerate() {
        let name_bytes = &map[entry.name_offset..entry.name_offset + entry.name_len];
        let name = unsafe { std::str::from_utf8_unchecked(name_bytes) };
        print!(
            "{name}={:.1}/{:.1}/{:.1}",
            entry.min as f64 / 10.0,
            entry.sum as f64 / 10.0 / entry.count as f64,
            entry.max as f64 / 10.0
        );
        if i + 1 != results.len() {
            print!(", ");
        }
    }
    println!("}}");
}

#[inline(always)]
fn parse_temperature(t: &[u8]) -> i32 {
    let mut neg = 1;
    let mut i = 0;
    if t[i] == b'-' {
        i += 1;
        neg = -1;
    }

    let mut n = (t[i] - b'0') as i32;
    i += 1;

    if t[i] != b'.' {
        n = n * 10 + (t[i] - b'0') as i32;
        i += 1;
    }

    i += 1; // skip .
    n = n * 10 + (t[i] - b'0') as i32;
    neg * n
}

fn mmap(f: File) -> Result<&'static [u8], io::Error> {
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

#[inline(always)]
fn hash_to_idx(word_0: usize, word_1: usize, table_size: usize) -> usize {
    let hash = word_0 ^ word_1;
    let hash = hash ^ (hash >> 33) ^ (hash >> 15);
    hash & (table_size - 1)
}

fn insert_or_update(
    entries: &mut Vec<Option<Entry>>,
    station: &[u8],
    temperature: i32,
    map: &[u8],
) {
    let mut word_0: usize = 0;
    let mut word_1: usize = 0;

    for i in 0..station.len().min(8) {
        word_0 |= (station[i] as usize) << (i * 8);
    }
    for i in 0..station.len().saturating_sub(8).min(8) {
        word_1 |= (station[i + 8] as usize) << (i * 8);
    }

    let mut idx = hash_to_idx(word_0, word_1, entries.len());
    let step = 31;
    let mask = entries.len() - 1;
    let name_offset = unsafe { station.as_ptr().offset_from(map.as_ptr()) } as usize;

    loop {
        match &mut entries[idx] {
            // empty slot -> insert
            None => {
                entries[idx] = Some(Entry {
                    first_word: word_0,
                    second_word: word_1,
                    min: temperature,
                    max: temperature,
                    sum: temperature,
                    count: 1,
                    name_len: station.len(),
                    name_offset,
                });
                return;
            }
            Some(e) => {
                // fast reject if collision
                if e.first_word != word_0 || e.second_word != word_1 {
                    idx = (idx + step) & mask;
                    continue;
                }

                // slow path if collision: compare full slice
                let existing = &map[e.name_offset..(e.name_offset + e.name_len)];
                if existing != station {
                    idx = (idx + step) & mask;
                    continue;
                }

                // exist, update entry
                e.min = e.min.min(temperature);
                e.max = e.max.max(temperature);
                e.sum += temperature;
                e.count += 1;

                return;
            }
        }
    }
}
