extern crate fringe;
extern crate futures;
extern crate tokio_timer;
extern crate fringe_futures;

use futures::{Future };
use fringe_futures::FringeFut;
use tokio_timer::*;
use std::time::Duration;

fn main() {
    let timer = Timer::default();

    let f = FringeFut::<usize, ()>::new(|yielder| {
        for n in 0..10 {
            let dur = Duration::from_millis(100) * n;
            println!("n:{:?}", dur);
            yielder.await(timer.sleep(dur)).expect("await");
        }
        Ok(42usize)
    });

    println!("Before:{:?}", f);
    let res = f.wait();
    println!("res:{:?}", res);
}
