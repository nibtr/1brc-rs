# 1brc-rs

My attempt to do the "One billion row challenge" in Rust for educational
purposes.

## The challenge

Refer to this [repo](https://github.com/gunnarmorling/1brc) for more
details.

### Rules

- No external library dependencies may be used. You're limited to the
standard library of your language.
- Implementations must be provided as a single source file. Try to keep
it relatively short; don't copy-paste a library into your solution as a cheat.
- The computation must happen at application runtime; you cannot process
the measurements file at build time.
- Input value ranges are as follows:
    - Station name: non null UTF-8 string of min length 1 character and max
    length 100 bytes (i.e. this could be 100 one-byte characters, or 50
    two-byte characters, etc.)
    - Temperature value: non null double between -99.9 (inclusive) and 99.9
    (inclusive), always with one fractional digit
- There is a maximum of 10,000 unique station names.
- Implementations must not rely on specifics of a given data set. Any
valid station name as per the constraints above and any data
distribution (number of measurements per station) must be supported.

## How to run

Although the rule states that no external dependencies are allowed, I
think `libc` is at least a minimum dependency since std also uses it. Because of
this, currently from the `mmap` onward, my implementation only supports
unix-like system. 

To run the code, first ensure all rust toolchains are installed on your
machine. Then compile in release mode.

```bash
cargo b --release
```

- Generate the data, this will take approx 15GB of space

```bash
cargo r --bin gen 1_000_000_000
```

- Run a version

```bash
cargo r --bin <version>
```

## Benchmarks

### Specs

OS: Void Linux x86_64 - 6.12 kernel
CPU: AMD Ryzen 5 7600 (12 cores) @5.17 GHz
RAM: 32 GB DDR5

### Results

- Each version was run **5 times**.
- Calculate the **median** and **mean** runtime.
- Speedup is calculated relative to the baseline (v1).

## Benchmark Results

| Version | Description                                     | Median Time (s)| Mean ± SD (s)   | Speedup |
|---------|-------------------------------------------------|----------------|-----------------|---------|
| v1      | Naive version: BTreeMap                         | 159.40         | 161.68 ± 5.68   | 1.0x    |
| v2      | Normal HashMap with capacity                    | 85.16          | 85.54 ± 1.28    | 1.87x   |
| v3      | Lazy string allocate in HashMap key             | 73.17          | 73.15 ± 0.27    | 2.18x   |
| v4      | Vec<u8> as key + unchecked UTF-8 parse          | 59.24          | 59.30 ± 0.30    | 2.69x   |
| v5      | Parse temperature as i32                        | 55.08          | 55.18 ± 0.50    | 2.89x   |
| v6      | FNV-1a hasher                                   | 51.91          | 51.82 ± 0.23    | 3.07x   |
| v7      | mmap                                            | 33.50          | 33.42 ± 0.21    | 4.76x   |
| v8      | memchr                                          | 29.51          | 29.55 ± 0.14    | 5.40x   |
| v9      | Multithreading                                  | 4.26           | 4.27 ± 0.03     | 37.45x  |

## Note and todos

- I kinda want to dive deeper in SIMD, but maybe it's for a future
improvement. Plus, my knowledge on the topic is also limited so it will
take some time.
- Maybe add some dependency like `ahash` or the dedicated `memchr` crate 
to yield better results. I'll do some separate benchmarks on these but
it's not going into the results table above since it's against the rule.
- Think I'd take a look at some of community top benchmarks and see if I
can copy anything (highly doubt I can).
- Blog on this? (maybe step by step with `perf`?)

## License

MIT
