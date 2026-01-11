# rust1b

My attempt to do the "One billion row challenge" in Rust for educational purposes, with various optimization techniques
learned from different sources.

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
