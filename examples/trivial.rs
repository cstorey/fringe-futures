extern crate fringe;
extern crate futures;
extern crate fringe_futures;

use futures::{Future };
use fringe_futures::FringeFut;

fn main() {
    let f = FringeFut::<usize, ()>::new(|yielder| {
        for n in 0..10 {
            let r = yielder.await(futures::finished::<u32, ()>(n)).expect("await");
            println!("n:{:?}", r);
        }
        Ok(42usize)
    });

    println!("Before:{:?}", f);
    let res = f.wait();
    println!("res:{:?}", res);
}
