use std::{thread::sleep, time::Duration};

use sentry_options::{init, options};

/// An example usage of the Rust options client library
/// Every 3 seconds, prints out the value of example-option, float-option, and bool-option
///
/// Updating values in `../values` will be reflected in stdout
/// ^C to exit
fn main() -> anyhow::Result<()> {
    init()?;
    let sentry_options = options("testing");

    loop {
        sleep(Duration::from_secs(3));
        let string_value = sentry_options.get("example-option")?;
        let float_value = sentry_options.get("float-option")?;
        let bool_value = sentry_options.get("bool-option")?;
        println!(
            "values: {} | {} | {}",
            string_value, float_value, bool_value
        );
    }
}
