use std::{
    collections::BTreeMap,
    f64,
    fs::File,
    io::{self},
    os::fd::AsRawFd,
    thread,
};

const HASH_TABLE_SIZE: usize = 1 << 17;

#[derive(Copy, Clone)]
struct Entry {
    w0: usize,
    w1: usize,
    name_len: usize,
    name_offset: usize, // offset relative to full mmap
    min: i32,
    max: i32,
    sum: i32,
    count: usize,
}

fn main() {
    let f = File::open("data/measurements.txt").expect("file should exist");
    let map = mmap(f).unwrap();

    let n_workers = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let chunks = chunk_file(map, n_workers);
    let mut handles = Vec::with_capacity(n_workers);
    let mut results: Vec<Vec<Option<Entry>>> = vec![Vec::new(); n_workers];

    for (idx, (start, end)) in chunks.into_iter().enumerate() {
        handles.push(thread::spawn(move || {
            (idx, thread_process_chunk(&map[start..end], map))
        }));
    }

    for handle in handles {
        let (idx, entries) = handle.join().expect("should be able to join");
        results[idx] = entries;
    }

    let mut final_map: BTreeMap<&str, Entry> = BTreeMap::new();
    for thread_entries_result in &results {
        for &entry in thread_entries_result.iter().flatten() {
            let name_bytes = &map[entry.name_offset..entry.name_offset + entry.name_len];
            let name = unsafe { std::str::from_utf8_unchecked(name_bytes) };

            if let Some(existing) = final_map.get_mut(name) {
                existing.min = existing.min.min(entry.min);
                existing.max = existing.max.max(entry.max);
                existing.sum += entry.sum;
                existing.count += entry.count;
            } else {
                final_map.insert(name, entry);
            }
        }
    }

    print!("{{");
    for (i, (name, entry)) in final_map.iter().enumerate() {
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
    let mut hash = word_0 ^ word_1;
    hash ^= (hash >> 33) ^ (hash >> 15);
    hash & (table_size - 1)
}

fn thread_insert_or_update(
    entries: &mut [Option<Entry>],
    station: &[u8],
    temperature: i32,
    map: &[u8], // full mmap for offset calculations
) {
    let mut w0: usize = 0;
    let mut w1: usize = 0;

    for i in 0..station.len().min(8) {
        w0 |= (station[i] as usize) << (i * 8);
    }
    for i in 0..station.len().saturating_sub(8).min(8) {
        w1 |= (station[i + 8] as usize) << (i * 8);
    }

    let mut idx = hash_to_idx(w0, w1, entries.len());
    let step = 31;
    let mask = entries.len() - 1;

    loop {
        match &mut entries[idx] {
            // empty slot -> insert
            None => {
                entries[idx] = Some(Entry {
                    w0,
                    w1,
                    min: temperature,
                    max: temperature,
                    sum: temperature,
                    count: 1,
                    name_len: station.len(),
                    name_offset: unsafe { station.as_ptr().offset_from(map.as_ptr()) } as usize,
                });
                return;
            }
            Some(e) => {
                // fast reject if collision
                if e.w0 != w0 || e.w1 != w1 {
                    idx = (idx + step) & mask;
                    continue;
                }

                if e.name_len as usize != station.len() {
                    idx = (idx + step) & mask;
                    continue;
                }

                // slow path if collision: compare full slice
                let existing =
                    unsafe { map.get_unchecked(e.name_offset..e.name_offset + e.name_len) };
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

fn thread_process_chunk(chunk: &[u8], map: &[u8]) -> Vec<Option<Entry>> {
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

    entries
}
