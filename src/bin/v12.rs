#![feature(cold_path)]

use std::{
    collections::BTreeMap,
    f64,
    fs::File,
    io::{self},
    os::fd::AsRawFd,
    thread,
};

const HASH_TABLE_SIZE: usize = 1 << 17;
const INLINE_NAME_CAP: usize = 16;

/// Linear probing step
const STEP: usize = 31;
/// The mask table for `load_u64_masked`, necessary to mask out unwanted bytes
/// Assume little endian, which x86_64 and ARM is
const MASK_TABLE: [u64; 9] = [
    0x0000000000000000,
    0x00000000000000FF,
    0x000000000000FFFF,
    0x0000000000FFFFFF,
    0x00000000FFFFFFFF,
    0x000000FFFFFFFFFF,
    0x0000FFFFFFFFFFFF,
    0x00FFFFFFFFFFFFFF,
    0xFFFFFFFFFFFFFFFF,
];

#[derive(Copy, Clone)]
struct Entry {
    w0: usize,
    w1: usize,
    min: i16,
    max: i16,
    sum: i32,
    count: u32,

    name_offset: usize, // offset relative to full mmap
    name_len: u8,
    inline_name: [u8; INLINE_NAME_CAP],
}

impl Entry {
    fn new(w0: usize, w1: usize, temperature: i16, name_len: u8) -> Self {
        Self {
            w0,
            w1,
            min: temperature,
            max: temperature,
            sum: temperature as i32,
            count: 1,
            name_len,
            inline_name: [0; INLINE_NAME_CAP],
            name_offset: 0,
        }
    }

    #[inline(always)]
    fn update(&mut self, temperature: i16) {
        self.min = self.min.min(temperature);
        self.max = self.max.max(temperature);
        self.sum += temperature as i32;
        self.count += 1;
    }

    #[inline(always)]
    fn write_inline_name(&mut self, station: &[u8], map: &[u8]) {
        if station.len() < INLINE_NAME_CAP {
            self.inline_name[..station.len()].copy_from_slice(station);
        } else {
            std::hint::cold_path();
            self.name_offset = unsafe { station.as_ptr().offset_from(map.as_ptr()) as usize };
        };
    }

    #[inline(always)]
    fn entry_name_eq(&self, station: &[u8], map: &[u8]) -> bool {
        if self.name_len as usize != station.len() {
            return false;
        }

        if station.len() < INLINE_NAME_CAP {
            &self.inline_name[..station.len()] == station
        } else {
            std::hint::cold_path();
            let existing = unsafe {
                map.get_unchecked(
                    self.name_offset as usize..self.name_offset as usize + station.len(),
                )
            };
            existing == station
        }
    }
}

fn main() {
    let f = File::open("data/measurements.txt").expect("file should exist");
    let map = mmap(f).unwrap();

    let n_workers = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let chunks = chunk_file(map, n_workers);
    let mut handles = Vec::with_capacity(n_workers);
    let mut results: Vec<Vec<Entry>> = Vec::with_capacity(n_workers);

    for (start, end) in chunks {
        handles.push(thread::spawn(move || {
            thread_process_chunk(&map[start..end], map)
        }));
    }

    for handle in handles {
        let entries = handle.join().expect("should be able to join");
        results.push(entries);
    }

    let mut final_map: BTreeMap<&[u8], Entry> = BTreeMap::new();
    for thread_entries_result in &results {
        for entry in thread_entries_result {
            let len = entry.name_len as usize;
            let name_bytes = if len < INLINE_NAME_CAP {
                &entry.inline_name[..len]
            } else {
                std::hint::cold_path();
                unsafe {
                    map.get_unchecked(entry.name_offset as usize..entry.name_offset as usize + len)
                }
            };

            if let Some(existing) = final_map.get_mut(name_bytes) {
                existing.min = existing.min.min(entry.min);
                existing.max = existing.max.max(entry.max);
                existing.sum += entry.sum;
                existing.count += entry.count;
            } else {
                final_map.insert(name_bytes, *entry);
            }
        }
    }

    print!("{{");
    for (i, (&name_bytes, entry)) in final_map.iter().enumerate() {
        let name = unsafe { std::str::from_utf8_unchecked(name_bytes) };
        print!(
            "{name}={:.1}/{:.1}/{:.1}",
            entry.min as f64 / 10.0,
            entry.sum as f64 / 10.0 / entry.count as f64,
            entry.max as f64 / 10.0
        );
        if i + 1 != final_map.len() {
            print!(", ");
        }
    }
    println!("}}");
}

#[inline(always)]
fn parse_temperature(t: &[u8]) -> i16 {
    let mut i = 0;
    let neg = (t[0] == b'-') as i16;
    i += neg as usize;

    // first digit
    let d0 = (t[i] - b'0') as i16;

    // check if there are two digits before the dot: DD.D vs D.D
    let two_digits = (t[i + 1] != b'.') as i16;

    // second digit (= first digit if two_digits == 0)
    let d1 = (t[i + two_digits as usize] - b'0') as i16;

    let frac = (t[i + two_digits as usize + 2] - b'0') as i16;
    let int_part = d0 * (1 + 9 * two_digits) + d1 * two_digits;
    let val = int_part * 10 + frac;
    val - (neg * val * 2)
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

/// return an array of chunks (start_of_chunk, end_of_chunk)
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

fn thread_process_chunk(chunk: &[u8], map: &[u8]) -> Vec<Entry> {
    // credit: thomaswue
    // he's using a vec with manual collision handling
    // see: https://github.com/gunnarmorling/1brc/blob/main/src/main/java/dev/morling/onebrc/CalculateAverage_thomaswue.java#L108
    let mut entries: Vec<Option<Entry>> = vec![None; HASH_TABLE_SIZE];

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

        let semicolon_ptr = unsafe {
            libc::memchr(
                line.as_ptr() as *const libc::c_void,
                b';' as libc::c_int,
                line.len(),
            )
        };
        let semicolon_pos = if semicolon_ptr.is_null() {
            continue; // skip malformed lines
        } else {
            unsafe { (semicolon_ptr as *const u8).offset_from(line.as_ptr()) as usize }
        };

        let station = &line[..semicolon_pos];
        let temperature = &line[(semicolon_pos + 1)..];
        let temperature = parse_temperature(temperature);

        thread_insert_or_update(&mut entries, station, temperature, map);
    }

    entries.into_iter().flatten().collect()
}

fn thread_insert_or_update(
    entries: &mut [Option<Entry>],
    station: &[u8],
    temperature: i16,
    map: &[u8], // full mmap for offset calculations
) {
    let bytes = station.as_ptr();
    let len = station.len();

    let w0 = unsafe { load_u64_masked(bytes, len.min(8)) } as usize;
    let w1 = unsafe { load_u64_masked(bytes.add(8), len.saturating_sub(8).min(8)) } as usize;

    let mut idx = hash_to_idx(w0, w1, entries.len());
    let mask = entries.len() - 1;

    loop {
        match &mut entries[idx] {
            // empty slot -> insert
            None => {
                let mut e = Entry::new(w0, w1, temperature, station.len() as u8);
                e.write_inline_name(station, map);
                entries[idx] = Some(e);

                return;
            }
            Some(e) => {
                // check first 16 bytes, fast reject if collision
                if e.w0 != w0 || e.w1 != w1 {
                    idx = (idx + STEP) & mask;
                    continue;
                }

                // check for name match
                if !e.entry_name_eq(station, map) {
                    idx = (idx + STEP) & mask;
                    continue;
                }

                // exist, update entry
                e.update(temperature);
                return;
            }
        }
    }
}

// credit: thomaswue
// see: https://github.com/gunnarmorling/1brc/blob/main/src/main/java/dev/morling/onebrc/CalculateAverage_thomaswue.java#L188
#[inline(always)]
unsafe fn load_u64_masked(ptr: *const u8, len: usize) -> u64 {
    unsafe {
        let v = core::ptr::read_unaligned(ptr as *const u64);
        // use the length to index into a table of masks
        // branchless and avoids bit shift
        // e.g: If len is 3, we still read 8 bytes, 3 bytes are good data, the remanining 5 is
        // garbage. We need to mask it with the masks table
        // so len 3 gives us 0x0000000000FFFFFF -> keeps the first 3 bytes
        v & *MASK_TABLE.get_unchecked(len.min(8))
    }
}

// credit: thomaswue
// see: https://github.com/gunnarmorling/1brc/blob/main/src/main/java/dev/morling/onebrc/CalculateAverage_thomaswue.java#L302
#[inline(always)]
fn hash_to_idx(w0: usize, w1: usize, table_size: usize) -> usize {
    let mut hash = w0 ^ w1;
    hash ^= (hash >> 33) ^ (hash >> 15);
    hash & (table_size - 1)
}
