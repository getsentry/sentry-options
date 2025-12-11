use std::{thread::sleep, time::Duration};

use sentry_options::Options;

/// An example usage of the Rust options client library
/// Every 3 seconds, prints out the value of example-option, float-option, and bool-option
///
/// Updating values in `../values` will be reflected in stdout
/// ^C to exit
fn main() -> anyhow::Result<()> {
    let options = Options::new()?;

    loop {
        sleep(Duration::from_secs(3));
        let string_value = options.get("testing", "example-option")?;
        let float_value = options.get("testing", "float-option")?;
        let bool_value = options.get("testing", "bool-option")?;
        println!(
            "values: {} | {} | {}",
            string_value, float_value, bool_value
        );
    }
}
