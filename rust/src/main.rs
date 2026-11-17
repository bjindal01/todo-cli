use std::env;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::process::ExitCode;

use todo_cli::{App, TaskStore, app, compat, locale};

fn main() -> ExitCode {
    let zero_digit = locale::init();
    let args: Vec<String> = env::args_os()
        .skip(1)
        .map(|arg| compat::decode_utf8_lossy(arg.as_bytes()))
        .collect();
    let store = TaskStore::new(app::default_file());
    let mut out = io::stdout().lock();
    let mut err = io::stderr().lock();
    let code = App::new(store, &mut out, &mut err)
        .with_zero_digit(zero_digit)
        .run(&args);
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
