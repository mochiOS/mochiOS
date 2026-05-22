use std::sync::{Arc, Condvar, Mutex};
use std::thread;

fn main() {
    println!("[StdSmoke] start");

    let counter = Arc::new(Mutex::new(0u32));
    let pair = Arc::new((Mutex::new(false), Condvar::new()));

    let c2 = Arc::clone(&counter);
    let p2 = Arc::clone(&pair);
    let t = thread::spawn(move || {
        {
            let mut n = c2.lock().unwrap();
            *n += 1;
        }
        let (lock, cv) = &*p2;
        let mut ready = lock.lock().unwrap();
        *ready = true;
        cv.notify_one();
    });

    // Wait on condvar
    let (lock, cv) = &*pair;
    let mut ready = lock.lock().unwrap();
    while !*ready {
        ready = cv.wait(ready).unwrap();
    }

    t.join().unwrap();

    let n = *counter.lock().unwrap();
    println!("[StdSmoke] counter={}", n);
}

