use sentry_options::Options;
use std::fs;
use std::thread;
use std::time::{Duration, Instant};

fn main() {
    let temp = tempfile::TempDir::new().unwrap();
    let schemas = temp.path().join("schemas");
    let values = temp.path().join("values");
    fs::create_dir_all(&schemas).unwrap();

    let schema_dir = schemas.join("bench");
    fs::create_dir_all(&schema_dir).unwrap();
    fs::write(
        schema_dir.join("schema.json"),
        r#"{
            "version": "1.0",
            "type": "object",
            "properties": {
                "key": {"type": "string", "default": "", "description": "test"}
            }
        }"#,
    )
    .unwrap();

    let values_dir = values.join("bench");
    fs::create_dir_all(&values_dir).unwrap();
    let values_file = values_dir.join("values.json");
    fs::write(&values_file, r#"{"options": {"key": "original"}}"#).unwrap();

    let opts = Options::from_directory(temp.path()).unwrap();

    // Warm up - do one get to populate cache
    let _ = opts.get("bench", "key").unwrap();

    let values_path = values_file.clone();
    thread::spawn(move || {
        // Wait 3 seconds, then rewrite the file
        thread::sleep(Duration::from_secs(3));
        fs::write(&values_path, r#"{"options": {"key": "updated"}}"#).unwrap();
        eprintln!("[{:.1}s] --- file rewritten ---", 3.0);
    });

    // Poll get() every 100ms for 12 seconds, print each call's latency
    let start = Instant::now();
    for _ in 0..120 {
        let call_start = Instant::now();
        let val = opts.get("bench", "key").unwrap();
        let elapsed = call_start.elapsed();

        let since_start = start.elapsed().as_secs_f64();
        println!(
            "[{since_start:6.1}s] get() = {:>12} | {:>10.1} us",
            val.as_str().unwrap_or("?"),
            elapsed.as_nanos() as f64 / 1000.0,
        );

        thread::sleep(Duration::from_millis(100));
    }
}
