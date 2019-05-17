#![warn(clippy::all)]

use std::{
    error::Error,
    fs,
    io::{self, prelude::*, Write},
    path::Path,
    sync::Arc,
};

use crossbeam::channel::{unbounded, Sender};
use ignore::WalkBuilder;
use parking_lot::RwLock;
use tempfile::NamedTempFile;

fn main() -> Result<(), Box<dyn Error>> {
    let filenames = Arc::new(RwLock::new(Vec::new()));
    {
        let filenames = Arc::clone(&filenames);
        rayon::scope(move |s| {
            let (tx, rx) = unbounded();

            s.spawn(move |_| {
                walk_directory(tx);
            });

            {
                let filenames = filenames.clone();
                for path in rx.iter() {
                    let filenames = filenames.clone();
                    s.spawn(move |_| {
                        let mut files = filenames.write();
                        files.push(path);
                    });
                }
            }
        });
    }

    let new_lines = {
        let file_path = {
            let mut file = NamedTempFile::new().unwrap();
            let filenames_to_print = {
                let filenames = filenames.read();
                filenames.join("\n")
            };
            file.write_all(filenames_to_print.as_bytes()).unwrap();
            open_file(&mut file);
            file.into_temp_path()
        };

        wait_for_enter_key();

        fs::read_to_string(file_path).unwrap()
    };

    let new_filenames = new_lines.split('\n');
    rayon::scope(move |s| {
        for (i, new_filename) in new_filenames.enumerate() {
            let filenames = Arc::clone(&filenames);
            s.spawn(move |_| {
                let filenames = filenames.read();

                fs::rename(&filenames[i], &new_filename).unwrap();
            });
        }
    });

    Ok(())
}

fn walk_directory(tx: Sender<String>) {
    WalkBuilder::new(Path::new("."))
        .standard_filters(true)
        .build_parallel()
        .run(move || {
            let tx = tx.clone();
            Box::new(move |entry| {
                let entry = match entry {
                    Err(_) => {
                        return ignore::WalkState::Continue;
                    }
                    Ok(e) => e,
                };
                let path = entry
                    .path()
                    .to_str()
                    .unwrap_or_else(|| {
                        panic!("\"{}\" is not UTF-8", entry.path().to_string_lossy())
                    })
                    .to_string();
                tx.send(path).unwrap();
                ignore::WalkState::Continue
            })
        });
}

/// Opens the file on the system
fn open_file(file: &mut NamedTempFile) {
    use std::process::Command;
    if cfg!(target_os = "windows") {
        let out = Command::new("cmd")
            .arg("/C")
            .arg("start")
            .arg(file.path())
            .output()
            .unwrap();
        if !out.stderr.is_empty() {
            println!("{}", std::str::from_utf8(&out.stderr).unwrap());
        }
    } else {
        let out = Command::new("xdg-open").arg(file.path()).output().unwrap();
        if !out.stderr.is_empty() {
            println!("{}", std::str::from_utf8(&out.stderr).unwrap());
        }
    }
}

fn wait_for_enter_key() {
    println!("Press [ENTER] when you have finished editing the list of filenames.");
    let mut line = String::new();
    let stdin = io::stdin();
    stdin
        .lock()
        .read_line(&mut line)
        .expect("Could not read line");
}
