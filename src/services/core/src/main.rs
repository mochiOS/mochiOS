use core::fmt;
use swiftlib::io;
use swiftlib::process;
use swiftlib::time;

/// サービス定義
struct ServiceDef {
    name: &'static str,
    path: &'static str,
}

const SERVICES: &[ServiceDef] = &[
    ServiceDef { name: "disk.service", path: "disk.service" },
    ServiceDef { name: "fs.service",   path: "fs.service"   },
    ServiceDef { name: "vga.service",  path: "vga.service"  },
];

#[cfg(feature = "run_tests")]
const TEST_PATH: &str = "tests";

struct Stdout;

impl fmt::Write for Stdout {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        io::write_stdout(s.as_bytes());
        Ok(())
    }
}

macro_rules! print {
    ($($arg:tt)*) => ({
        let _ = core::fmt::Write::write_fmt(&mut Stdout, format_args!($($arg)*));
    });
}

macro_rules! println {
    () => (print!("\n"));
    ($($arg:tt)*) => (print!("{}\n", format_args!($($arg)*)));
}

fn start_service(service: &ServiceDef) {
    println!("[CORE] Starting service: {}", service.name);
    match process::exec(service.path) {
        Ok(pid) => {
            println!("[CORE] {} started (PID={})", service.name, pid);
            time::sleep_ms(100);
        }
        Err(_) => println!("[CORE] Failed to start {}", service.name),
    }
}

fn main() {
    println!("[CORE] Service Manager Started");

    for service in SERVICES {
        start_service(service);
    }

    #[cfg(feature = "run_tests")]
    {
        println!("[CORE] Starting test application...");
        match process::exec(TEST_PATH) {
            Ok(pid) => println!("[CORE] tests started (PID={})", pid),
            Err(_) => println!("[CORE] Failed to start tests"),
        }
        time::sleep_ms(100);
    }

    println!("[CORE] Entering monitoring loop...");
    loop {
        time::sleep_ms(1000);
    }
}
