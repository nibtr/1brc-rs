use rand::distr::{Distribution, Uniform};
use rand::rng;
use rand::seq::IndexedRandom;
use std::{
    env,
    fs::{self, File},
    io::{self, Read, Write},
    time::Instant,
};

fn check_args(args: &[String]) -> usize {
    if args.len() != 2 {
        usage_and_exit();
    }

    match args[1].replace('_', "").parse::<usize>() {
        Ok(n) if n > 0 => n,
        _ => usage_and_exit(),
    }
}

fn usage_and_exit() -> ! {
    eprintln!("Usage:  create_measurements <positive integer number of records to create>");
    eprintln!("        You can use underscore notation for large numbers");
    eprintln!("        Example: 1_000_000_000");
    std::process::exit(1);
}

fn build_weather_station_name_list() -> io::Result<Vec<String>> {
    let mut file = File::open("data/weather_stations.csv")?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    let mut stations = Vec::new();

    for line in contents.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some((name, _)) = line.split_once(';') {
            stations.push(name.to_string());
        }
    }

    stations.sort_unstable();
    stations.dedup();
    Ok(stations)
}

fn convert_bytes(mut num: f64) -> String {
    for unit in ["bytes", "KiB", "MiB", "GiB"] {
        if num < 1024.0 {
            return format!("{:.1} {}", num, unit);
        }
        num /= 1024.0;
    }
    format!("{:.1} TiB", num)
}

fn format_elapsed_time(seconds: f64) -> String {
    if seconds < 60.0 {
        format!("{:.3} seconds", seconds)
    } else if seconds < 3600.0 {
        let m = (seconds / 60.0) as u64;
        let s = (seconds % 60.0) as u64;
        format!("{m} minutes {s} seconds")
    } else {
        let h = (seconds / 3600.0) as u64;
        let rem = seconds % 3600.0;
        let m = (rem / 60.0) as u64;
        let s = (rem % 60.0) as u64;
        if m == 0 {
            format!("{h} hours {s} seconds")
        } else {
            format!("{h} hours {m} minutes {s} seconds")
        }
    }
}

fn estimate_file_size(stations: &[String], rows: usize) -> String {
    let total_name_bytes: usize = stations.iter().map(|s| s.as_bytes().len()).sum();
    let avg_name_bytes = total_name_bytes as f64 / stations.len() as f64;
    let avg_temp_bytes = 4.400200100050025_f64;
    // name + ';' + temp + '\n'
    let avg_line_len = avg_name_bytes + avg_temp_bytes + 2.0;
    let total_bytes = rows as f64 * avg_line_len;

    format!(
        "Estimated max file size is:  {}.",
        convert_bytes(total_bytes)
    )
}

fn build_test_data(stations: &[String], rows: usize) -> io::Result<()> {
    let start = Instant::now();

    let coldest = -99.9_f64;
    let hottest = 99.9_f64;
    let temp_dist = Uniform::new_inclusive(coldest, hottest);

    let mut rng = rng();

    // pre-sample 10k station names
    let station_pool: Vec<&String> = stations
        .choose_multiple(&mut rng, 10_000.min(stations.len()))
        .collect();

    let batch_size = 10_000usize;
    let chunks = rows / batch_size;

    println!("Building test data...");

    let mut file = File::create("data/measurements.txt")?;
    let mut progress = 0;

    let mut buffer = String::with_capacity(batch_size * 32);

    for chunk in 0..chunks {
        buffer.clear();

        for _ in 0..batch_size {
            let station = station_pool.choose(&mut rng).unwrap();
            let temp = temp_dist.unwrap().sample(&mut rng);
            buffer.push_str(station);
            buffer.push(';');
            buffer.push_str(&format!("{:.1}", temp));
            buffer.push('\n');
        }

        file.write_all(buffer.as_bytes())?;

        let new_progress = (chunk + 1) * 100 / chunks.max(1);
        if new_progress != progress {
            progress = new_progress;
            let bars = "=".repeat(progress / 2);
            print!("\r[{:<50}] {}%", bars, progress);
            io::stdout().flush().unwrap();
        }
    }

    println!();

    let elapsed = start.elapsed().as_secs_f64();
    let file_size = fs::metadata("data/measurements.txt")?.len() as f64;

    println!("Test data successfully written to data/measurements.txt");
    println!("Actual file size:  {}", convert_bytes(file_size));
    println!("Elapsed time: {}", format_elapsed_time(elapsed));

    Ok(())
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let rows = check_args(&args);

    let stations = build_weather_station_name_list()?;
    println!("{}", estimate_file_size(&stations, rows));

    build_test_data(&stations, rows)?;
    println!("Test data build complete.");

    Ok(())
}
