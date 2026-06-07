use std::fs;

fn main() {
    println!("[StdFsSmoke] start");

    let path = "/system/fonts/ter-u12b.bdf";
    let md = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            println!("[StdFsSmoke] metadata failed: {}", e);
            return;
        }
    };
    println!("[StdFsSmoke] size={}", md.len());

    let data = match fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            println!("[StdFsSmoke] read failed: {}", e);
            return;
        }
    };
    println!("[StdFsSmoke] read ok: {} bytes", data.len());

    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            println!("[StdFsSmoke] read_to_string failed: {}", e);
            return;
        }
    };
    println!(
        "[StdFsSmoke] first line: {}",
        text.lines().next().unwrap_or("")
    );
}

