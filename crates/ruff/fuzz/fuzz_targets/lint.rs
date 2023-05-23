#![no_main]

use std::default::Default;
use std::path::Path;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    let root = Path::new("/tmp");
    let file_path = Path::new("/tmp/file");
    let settings = ruff::settings::Settings::from_configuration(Default::default(), root).unwrap();
    let _ = ruff::linter::lint_only(data, file_path, None, &settings, ruff::settings::flags::Noqa::Disabled);
});
